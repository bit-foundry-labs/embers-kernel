# Unsafe boundaries in the early Rust kernel

**Audit scope:** Early Rust boot, memory, paging-state, QEMU-only permission faults and experimental stack paths, reviewed against `main` at `ab5fe78b8c430e6c8e9a990542994a775c21963e` (through merged #77). Recheck this inventory whenever feature gates or the privileged boot boundary changes.

**Status:** Source-level inventory, not formal verification or proof that external pointers and hardware state are safe. The companion host-only PTE field tests are pure numerical models; no privileged code or page tables are changed. Follow the function names rather than line numbers.

The current execution model is one bootstrap CPU (BSP), CPL0, interrupts disabled, Limine-owned initial page tables and boot stack, no user address spaces, and no DMA isolation. All QEMU results apply to this environment, not arbitrary hardware or multicore configurations.

## Mechanism-by-mechanism inventory

| Site | Required precondition and trust boundary | Failure behavior or risk | Existing evidence; untested assumption |
| --- | --- | --- | --- |
| [`main.rs::port_write` / `port_read`](src/main.rs) | Caller has CPL0 I/O privilege; legacy COM1 port responds as expected. `asm!` uses `out` / `in` and explicit register operands. | Inappropriate privilege or port availability can cause a CPU fault or nonfunctional diagnostics. | Normal and fault-mode QEMU serial logs exercise this path. **Untested:** physical UART availability and I/O restrictions outside QEMU. |
| [`main.rs::serial_init`, `serial_write_byte`, `serial_write`](src/main.rs) | Exclusive BSP access during early boot; UART register layout; bounded ready polling. | A transmit timeout makes the diagnostic writer halt, with no recursive recovery output. Incomplete serial logs are possible. | M02 boot/READY CI and bounded serial host tests. **Untested:** concurrent writers, real UART timing, and hardware-fault recovery. |
| [`main.rs::limine_revision_supported`](src/main.rs) | Limine has populated `BASE_REVISION` before handoff; writable `static mut` lies in a mapped segment; no later concurrent writer. The volatile read only observes memory. | Unsupported revision leads to an error marker and halt; **invalid pointers or corrupted bootloader state are not authenticated** by this check. | M02 normal QEMU boot exercises supported revision. **Untested:** corrupted revision storage and hostile bootloader behavior. |
| [`main.rs::kernel_entry`](src/main.rs) | Limine enters at CPL0 with valid long-mode CPU state, mapped kernel image and writable stack, and interrupts disabled. It is the single initializing core. | A failed boot prerequisite can halt or trigger a fault; there is no recovery path. | M02 QEMU READY; M03 fault tests. **Untested:** arbitrary hardware, SMP boot, other Limine configurations, or interrupted initialization. |
| [`arch.rs::install`](src/arch.rs) | Runs once on the BSP at CPL0, with mapped, writable `GDT`, `TSS`, `IDT`, and double-fault stack; no CPU concurrently uses partly initialized descriptors. Assembly executes `cli`, `lgdt`, far return, segment loads, `ltr`, and `lidt`. | A malformed descriptor or stack can fault during exception delivery, potentially escalating to a double/triple fault. | M03 QEMU #UD/#PF/#DF tests exercise installed descriptors. **Untested:** ring-3 transitions, SMP descriptor ownership, RSP0 use, and resumable interrupts. |
| [`arch.rs::global_asm!` exception stubs](src/arch.rs) | x86-64 CPU exception frame layout is exactly the assumed one; #UD has no error code, #DF/#GP/#PF push an error code; the #DF handler receives IST1 stack. Stubs align RSP and pass vector/RIP/error code to a nonreturning Rust reporter. | Wrong frame offsets, stack alignment, or a missing mapped stack can misreport RIP or fault again. Stubs do **not** preserve interrupted registers or return. | M03 asserts fault markers and saved RIP in QEMU. **Untested:** user-origin traps, privilege-change frames, resumable interrupts and all unhandled vectors. |
| [`arch.rs::trigger_page_fault`](src/arch.rs) | Only enabled by `fault-pf`/`fault-df`; a selected canonical virtual address is unmapped in the test image; CPL0 test runs in disposable QEMU. | An accidental mapping may prevent the intended #PF; a fault is intentionally fatal. | M03 QEMU #PF/#DF markers. **Untested:** other address layouts or bootloader mapping configurations. |
| [`arch.rs::remove_page_fault_gate_for_test`](src/arch.rs) | Only compiled with `fault-df`; sole core has exclusive write access to the mapped IDT; removing the #PF gate is **only** a fault-injection technique. | Deliberately prevents ordinary #PF delivery and provokes #DF; unsafe outside the isolated test. | M03 QEMU #DF with saved RIP. **Untested:** concurrent interrupts, real hardware and fault containment beyond the test. |
| [`arch.rs::page_fault_address`](src/arch.rs) | Reading `cr2` occurs at CPL0 in the synchronous exception reporter, before an unrelated fault could replace the intended address. | Could report an unrelated fault address if future concurrency or nested faults change CR2. | M03 QEMU page-fault log includes CR2. **Untested:** nested/concurrent faults and user-origin page faults. |
| [`main.rs::embers_report_exception`, `halt_forever`](src/main.rs) | Called from nonreturning privileged stubs, or fatal boot paths. `cli` and `hlt` assume CPL0; halt loop intentionally cannot resume scheduling. | Execution stops; missing UART output is not evidence of successful isolation or fault recovery. | M03 QEMU fault log and M02 boot marker. **Untested:** multicore shutdown and IRQ-driven recovery. |
| [`boot_memory.rs::initial_frame_pool`](src/boot_memory.rs) | Limine is the sole writer before handoff and supplies a stable, correctly aligned, initialized, mapped response struct; `read_volatile` does **not** validate pointer provenance. | Null response, zero/excessive count, null entry and malformed region *values* return errors; a nonnull dangling/unmapped pointer can still cause undefined behavior or a CPU fault before the value checks. | M04 host tests cover copied region validation; QEMU validates a normal Limine map. **Untested:** malicious/dangling response pointers, concurrent bootloader mutation and post-CR3 mapping lifetime. |
| [`boot_memory.rs` pointer-array walk](src/boot_memory.rs) | Limine's `entries` pointer references at least the bounded `entry_count` valid, aligned pointers; each nonnull entry references an initialized `LimineMemoryMapEntry` with the expected ABI. | Count is bounded at 128, but this cannot establish that the pointed-to bytes are allocated or readable. Bad pointers can violate Rust safety before `choose_pool` runs. | M04 QEMU with an ordinary map; pure-Rust region tests. **Untested:** corrupted pointer array, ABI mismatch, pointer aliasing and transient mappings. |

## Newer research and unsafe sites (post-PR #41)

| Site | Assumption and obligation | Failure mode / unresolved risk | Existing evidence and missing tests |
| --- | --- | --- | --- |
| [`boot_addresses.rs::load`](src/boot_addresses.rs) | The Limine HHDM and executable response pointers must be nonnull, correctly aligned, initialized and readable under inherited mappings. The BSP owns the handoff; `read_volatile` does not authenticate pointer provenance. | A nonnull but dangling/unmapped pointer can cause a fault or undefined behavior **before** numeric address validation. Reclaiming Limine-owned response memory before copying it would invalidate the assumption. | Normal QEMU boot and pure arithmetic host tests; **no** adversarial proof of external response-pointer validity. |
| [`paging_mode.rs::check_active`](src/paging_mode.rs) | The BSP is at CPL0 in x86-64 long mode before using read-only `mov` from CR0/CR4. | Invalid privilege or hardware handoff can fault. Selected bits alone do not establish PTE reservation, physical width or table ownership. | QEMU `qemu64` and `max` boot plus pure host bit tests; no hardware-independent proof. |
| [`paging_mode.rs::sample_active_context`](src/paging_mode.rs) | CPL0, active Limine page tables and an initialized stack; read-only `mov` from CR3 and RSP in a single BSP. | RSP is sampled inside the function, not the Limine entry point; masking CR3 **discards control/reserved-bit context** and produces only a numeric root candidate. | QEMU register serial checks and paused host-side PTE observations; **no** proof of root frame ownership, whole-stack lifetime or all reserved bits. |
| [`transition_stack.rs::probe_inherited_mappings`](src/transition_stack.rs) | Only the `transition-stack-smoke` feature, single BSP, interrupts disabled and a private aligned BSS buffer under writable Limine mappings. `static mut` is accessed through raw pointers without creating Rust references. | A missing page can fault the opt-in QEMU guest. The probe restores sampled bytes but does not establish effective security protections, alias freedom or safety after a root switch. | Feature-only volatile smoke and read-only QMP snapshots of four pages on both CPU profiles; **not** a live stack switch, device isolation or lifetime proof. |

| [`paging_mode.rs::sample_protection_state`](src/paging_mode.rs) (PR #74) | Requires a single CPL0 BSP, a valid Limine long-mode handoff and supported extended CPUID leaves. It queries advertised NX and physical/linear widths, then reads CR0 and IA32_EFER via `RDMSR`; no control register or MSR writes occur. | A missing CPUID feature or WP/NXE/LMA bit is rejected; an invalid privilege or MSR assumption may fault. A width or active-bit sample cannot authenticate page-table contents or effective permissions. | Pure Rust validators, Python serial rejection fixtures and both QEMU CPU profiles on the reviewed #74 branch. **Untested:** non-QEMU processors, reserved PTE bits, all CPUID vendor edge cases, effective fault-based permission enforcement. |

| [`permission_fault.rs::trigger_supervisor_write`](src/permission_fault.rs) (feature-only) | CPL0 BSP, CR0.WP enabled, private page aligned immutable static placed in the ELF R segment and mapped read-only by Limine. Attempted store is deliberate QEMU fault injection. | If write protection is absent, one private canary byte is modified; the code emits an error and halts. A missing page or other fault type is rejected by the test parser. | Separate `fault-ro-write` QEMU boot, exact #PF error-code and CR2 checking; **not** production-safe write handling or physical ownership. |
| [`permission_fault.rs::trigger_instruction_fetch`](src/permission_fault.rs) (feature-only) | CPL0 BSP, EFER.NXE enabled and a private page-aligned static in the ELF RW/NX load segment. Indirect jump deliberately attempts an instruction fetch; embedded `UD2` is a fail-closed canary if execution succeeds. | An unexpected instruction fetch yields #UD or another exception, not the required #PF; no handler recovery is possible. | Separate `fault-nx-exec` QEMU boot, exact #PF error-code, CR2 and RIP match; **not** global executable-permission or DMA proof. |

**Audit scope note:** CPU protection preflight (#74), selected live MAXPHYADDR checks (#77), and these QEMU-only fault experiments remain distinct evidence layers. They do **not** collectively certify the inherited hierarchy or authorize a new CR3 root.

**No page-table writer:** The QMP scripts read guest physical table entries from the **host** while QEMU is paused; they do not dereference unvalidated physical pointers in kernel mode. The `research/pte_fields.rs` helper checks only caller-supplied MAXPHYADDR/NXE against a small 4 KiB field subset. It cannot prove live CPU protections, ownership, alias freedom, or safe `CR3` switching.


| [`arch.rs::sample_descriptor_evidence`](src/arch.rs) (feature-only) | A single CPL0 BSP after `install()`; existing descriptor and IST1 statics are mapped. SGDT/SIDT/STR observe installed register state. An unaligned raw-pointer load reads packed TSS.IST1. | Unexpected base/limit/selector/top rejects the feature boot; no CPU register or page table is modified. Host QMP checks translated descriptor/stack extents under inherited CR3, but cannot prove the hidden TR cache, the Limine BSP stack lifetime, or mappings surviving a new root. |

## Trusted data vs. actual authority

- [`memory.rs::choose_pool`](src/memory.rs) is safe Rust numeric validation. It checks zero lengths, wraparound, overlaps (including reserved regions), alignment, and a minimum usable span. The resulting [`FramePool`](src/memory.rs) returns **physical address numbers**, not mapped pages or hardware ownership.
- [`frame_ledger.rs::FrameLedger`](src/frame_ledger.rs) records symbolic owners and quarantines frames in a single-writer demonstration, compiled into the QEMU boot only with `boot-ownership-smoke`. Tokens are **not** kernel-authenticated process capabilities; independently constructed overlapping pools can reuse an identity. Permanent quarantine is a bookkeeping prohibition, not a DMA fence.
- The kernel inherits Limine's address mappings, including the executable image, stack, request/response structures, GDT/TSS/IDT statics and COM1 I/O assumptions. It does **not** presently verify page permissions or that physical frames are exclusively inaccessible to other agents.

These are **three different obligations** that must not be conflated:

1. **Rust memory safety:** Unsafe code must never dereference invalid pointers, construct invalid references, or violate aliasing and initialization requirements. Bounding the *number* of Limine pointers does not prove their validity.
2. **Privilege and capability authority:** Hardware page permissions and kernel-validated caller rights must prevent unauthorized access even when a user guesses a physical address or identifier. The present owner ledger cannot do this.
3. **CPU/device reclamation safety:** Removing future authorization does not establish that in-flight CPU accesses, device DMA or IOMMU translations have stopped. Safe frame reuse needs mapping teardown, synchronization/quiescence and explicit hardware-aware evidence.

## Gates before kernel-owned page tables

1. Specify which existing Limine mappings must remain usable during transition, including the executing instruction, current stack, exception stacks, serial diagnostics, bootloader data and page-table frames.
2. Ensure every new page-table frame is reserved from a uniquely owned, nonoverlapping physical pool; define initialization/zeroing access and prevent its reuse before old mappings are retired.
3. Define page-table ownership, canonical virtual addresses, page alignment, permission defaults (supervisor-only, non-executable data where supported), and what constitutes a valid mapped pointer.
4. Prove a transition sequence with failure containment before changing CR3; add QEMU smoke and deliberate negative-path faults. A successful host test is not evidence of page-table isolation.
5. Revisit CPU feature requirements, TLB invalidation, and SMP/DMA assumptions before extending beyond the single-core emulator baseline.

## How to reproduce existing checks

From the repository root on Ubuntu/WSL, with the pinned toolchain and QEMU dependencies:

```sh
make rust-check
make rust-memory-test
make rust-ledger-test
make rust-pte-fields-test
make rust-iso
```

The repository's M02, M03 and M04/M05 GitHub Actions workflows additionally run QEMU normal/fault/ownership-smoke checks. The local audit environment used to prepare this document did **not** have `cargo`, `rustc`, or `qemu-system-x86_64`; therefore **no local Rust build or emulator result is claimed**. Evaluate the PR checks independently, and do not treat passing CI as exhaustive proof.

## Review triggers

Update this inventory when introducing a page-table writer, changing Limine protocol requirements, accepting new user pointers, adding a user/privilege transition, enabling interrupts or SMP, or implementing a device/IOMMU interface. Sources: [M03](M03_EXCEPTIONS.md), [M04](M04_PHYSICAL_MEMORY.md), [M05](M05_FRAME_LEDGER.md).
