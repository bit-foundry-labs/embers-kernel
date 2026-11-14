# M06 proposal: First kernel-owned page-table transition

**Status:** Design specification only; no paging implementation or hardware-enforced process isolation exists on `main`.
**Scope:** x86-64, one bootstrap CPU, headless QEMU, Rust `no_std`, Limine base revision 6.
**Original reference baseline:** `f12d9d87d2aa5262435b59522f7d70cc8a187681` (PR #41). **Updated boot-bookkeeping checkpoint:** `ab2d36d` on `main` (PRs #59 and #60). The checkpoint is not evidence of a page-table transition. See [`main.rs`](../rust-kernel/src/main.rs), [`kernel_state.rs`](../rust-kernel/src/kernel_state.rs), [`boot_memory.rs`](../rust-kernel/src/boot_memory.rs), and [`memory.rs`](../rust-kernel/src/memory.rs).

## Purpose and explicit non-goals

This milestone will eventually replace the page-table root inherited from Limine with one constructed and owned by Embers, **without losing access to the instructions, stacks, descriptors, or fault-reporting paths in use at the switch**. It establishes a prerequisite for later protected address spaces, not a user-process or capability interface.

Not in scope: ring-3 execution, syscalls, IPC, process isolation, SMP, TLB shootdowns, DMA/IOMMU isolation, frame reclamation, dynamic allocation beyond explicitly reserved boot pages, and a general virtual-memory manager. **Do not change CR3 as part of this documentation PR.**

## Known baseline and gaps

- Limine currently supplies long mode, the initial `CR3` mappings, the executing stack, and response structures. The kernel installs GDT/TSS/IDT, including a kernel-owned 16 KiB double-fault IST stack. The entry stack is still bootloader-provided.
- `boot_memory::initial_frame_pool` consumes a one-shot production permit before parsing Limine map values; safe Rust cannot reconstruct another production `FramePool` through the exposed constructor. `kernel_entry` moves the transactional allocator and registry into `KernelState`, then into `run_kernel`, which checks the first reservation and halts. This is a single-image software issuance boundary, **not** an authenticated runtime allocator, frame clearing or mapping, a protection mechanism against unsafe/privileged code, or global hardware ownership.
- `frame_ledger::FrameLedger` is only used in an optional boot ownership smoke test; it does not enforce exclusive physical-frame access. Its identity is not globally unique across independently overlapping frame pools.
- Boot metadata preflight now reads and validates Limine HHDM and executable-base responses and calculates candidate addresses. It does not prove that arbitrary physical frames are mapped or writable. **Do not assume identity mapping, a fixed HHDM offset, or `physical == virtual`.**
- Current exception stubs are fatal and nonreturning. They must not be repurposed as resumable page-fault recovery or ring-3 trap entry.

## Mapping survival contract

All addresses below must be resolved to **verified physical backing and exact virtual ranges** before constructing or installing new tables. A pointer being non-null is not evidence of backing or mapping validity.

| Required mapping at transition | Why it must remain accessible | Evidence or design task |
| --- | --- | --- |
| Current instruction and all executable kernel code, including exception stubs and fatal halt path | Instruction fetch must work immediately before and after loading `CR3` | Obtain physical layout and mapped ELF segment ranges; validate kernel entry and linker layout rather than assuming a constant slide |
| Kernel read-only constants, mutable data, BSS and Limine request statics | Diagnostics and active references rely on them | Resolve each segment to physical backing; later apply minimal read/write/execute permissions at page granularity |
| A deliberately allocated kernel transition stack, mapped in both old and new address spaces | Old Limine stack extent and lifetime are not yet proven; switching stacks before `CR3` avoids relying on them afterward | Add a bounded, aligned stack and a separately reviewed stack switch; prove its pages remain mapped through both phases |
| `arch.rs` double-fault IST stack, GDT, TSS, IDT and their referenced handler code | A fault during transition must not fault again because the fault infrastructure disappeared | Verify canonical, mapped addresses and sufficient writable stack extent; preserve `GDTR`, `IDTR`, `TR` references |
| New page-table frames and required temporary mapping for table construction | `CR3` takes a physical root address; the kernel needs an accessible virtual view to initialize and inspect table bytes | Reserve unique, zero-initialized 4 KiB frames; establish a proven existing writable mapping before modifying them |
| Any bootloader memory-map response/entry data still being used | Switching tables could invalidate Limine-owned pointers | Copy all needed values into kernel-owned storage before changing mappings; never dereference abandoned Limine pointers afterward |

The COM1 UART is accessed with x86 port-I/O instructions, not through a memory-mapped serial register page. **Its kernel code, data and stack still need valid mappings**, however.

## Preconditions for the first implementation PR

1. **CPU paging mode:** Read and validate the active paging configuration (`CR0`, `CR3`, `CR4`, `EFER` and relevant CPUID capabilities). Specify whether the first implementation supports only 4-level paging; explicitly reject an unexpected LA57 mode instead of interpreting five-level tables as four-level.
2. **Physical provenance:** Select and validate Limine protocol features needed to locate the loaded executable's physical backing and the HHDM base, or provide another demonstrated physical-to-virtual mapping method. Use the protocol's reported values, never hard-coded offsets.
3. **Exclusive reservations:** Maintain a persistent single-owner record of all new table pages and the transition stack, excluding the kernel image and every reserved/bootloader-reclaimable region. Reject duplicate claims or overlapping separately constructed frame pools. The optional 8-slot M05 smoke ledger is insufficient for this.
4. **Initialized tables:** Require page-aligned, zeroed table pages and checked physical-address width, canonical virtual addresses, legal reserved bits and page-table entry permissions. Reject overflow and unsupported huge-page/PCID combinations rather than silently changing meaning.
5. **Permission model:** Initially create only supervisor mappings (`U/S=0`) for kernel resources. Separate executable text from writable data where supported by the segment/page layout. Enable and rely on NX only after checking CPU support and `EFER.NXE`; never mark data execute-protected if hardware cannot enforce it. State how the WP bit and global translations affect intended protection.
6. **Transition protocol:** Keep interrupts disabled on the single BSP, switch to the mapped transition stack before writing `CR3`, and ensure the new page-table hierarchy contains every live translation. Specify handling of `CR3` low bits, PCID and global TLB entries; a `CR3` write alone must not be assumed to invalidate every global translation.
7. **Failure policy:** If preflight or construction fails, do not write `CR3`; report an explicit diagnostic and halt under the existing trusted mappings. A post-switch fault is fatal and must retain the mapped #PF/#DF diagnostic path. Do not promise rollback or reclamation without a separately verified mechanism.
8. **Ownership lifetime:** Do not recycle new table frames, the transition stack, old Limine tables, or related bootloader-owned storage until their mapping and use lifetimes are established. No DMA or multicore quiescence claim is made.

## Proposed observable checkpoints

The **future implementation** should emit a distinct bounded serial checkpoint before constructing the table hierarchy, immediately before the switch, and after successful execution under the new root. It should log only non-sensitive boot metadata required for tests, not an assertion of process isolation.

| Future test | Required observation |
| --- | --- |
| Normal boot, no paging experiment feature | Existing `READY` and M03/M04/M05 regression behavior remain unchanged. |
| Preflight rejects unsupported paging mode or missing required address provenance | Explicit failure marker; never writes `CR3`. |
| Table build runs out of reserved frames or encounters an invalid mapping | Explicit failure marker; no partial installation or frame reuse. |
| Successful QEMU paging-transition smoke | New `CR3` root read-back, correct post-switch serial marker, preserved kernel instruction/data/stack access, expected normal halt. |
| Deliberately absent mapping in a separate fault-test profile | Expected fatal #PF marker, valid saved RIP and CR2, no false post-switch success marker. |
| Supervisor-only and read/write/execute permission probes, where supported | Documented access behavior with expected faults; probes do not silently weaken the normal mapping profile. |
| Repeated builds and distinct QEMU CPU profiles | Deterministic failure/success within declared compatibility bounds, with full serial logs retained. |

Use explicit feature gates so deliberate failure runs cannot be confused with normal boot. Preserve negative-path tests for `fault-ud`, `fault-pf` and `fault-df`. Passing a host-side model or static ELF check is **not** evidence that the new tables executed correctly; a real QEMU `CR3` switch trace is required for that claim.

## Suggested bounded implementation sequence

1. **M06a, preparatory:** Build on the existing metadata preflight and one-shot software frame issuer; add verified access, reservation and zeroing of real table frames, and test failure behavior without changing `CR3`.
2. **M06b, transition smoke:** Add a verified transition stack and a minimal new supervisor-only mapping set; switch `CR3` once in a gated QEMU run and check post-switch diagnostics.
3. **M06c, permissions:** Tighten page permissions and add explicit expected-fault probes. Review fault-stack mappings and safe TLB semantics.
4. **Later, isolation:** Only after these pass, design separate address spaces, an authenticated user-mode trap/syscall ABI and IPC. No step here establishes ring-3 safety.

## Review questions before code

- How will we discover and validate the executable physical address and the HHDM offset under our pinned Limine revision?
- Should the first transition deliberately support only four-level, 4 KiB paging; how do we reject unsupported CPU modes?
- Which kernel image segments and data currently share physical pages, preventing clean W^X separation?
- How will we place, align, map and prove the lifetime of a dedicated transition stack?
- How can the current one-shot safe-Rust pool issuer and boot reservation ledger become verified, persistent authority for actual page-table frames, including alias detection and use lifetimes?

## References

- [Limine boot protocol, base revisions and x86-64 memory mapping](https://github.com/limine-bootloader/limine-protocol/blob/trunk/PROTOCOL.md).
- [Intel SDM Volume 3, system programming and paging](https://www.intel.com/content/www/us/en/developer/articles/technical/intel-sdm.html).
- [M03 exception foundation](../rust-kernel/M03_EXCEPTIONS.md), [M04 physical-memory validation](../rust-kernel/M04_PHYSICAL_MEMORY.md) and [M05 ownership ledger](../rust-kernel/M05_FRAME_LEDGER.md).
- [Issue #40, proposed IPC guarantees](https://github.com/bit-foundry-labs/embers-kernel/issues/40). IPC is downstream of this transition.
