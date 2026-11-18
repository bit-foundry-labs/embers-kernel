# Embers Kernel: Rust-only Limine/QEMU research baseline

This is an original, freestanding Rust no_std x86-64 kernel entry, loaded
directly by **Limine**. Limine is an external bootloader; there is no
intermediate C kernel execution stage.

The former C bootstrap lives in the historical research branch PR #2.
The Rust build reuses a linker script and checksum-pinned Limine ISO
packaging machinery, but neither compiles nor links the C kernel.

## Source and test map for contributors

The executable is [`src/main.rs`](src/main.rs): it coordinates boot and diagnostics,
while [`boot_memory.rs`](src/boot_memory.rs) reads the Limine memory map and
[`memory.rs`](src/memory.rs) contains host-testable selection logic. The modules
below are organized by responsibility rather than milestone number. Start with a
host-only test for pure bookkeeping; use QEMU for boot, privileged CPU state,
or hardware-visible behavior.

| Responsibility | Source | Where to start testing |
| --- | --- | --- |
| Entry, diagnostics, and serial output | [`main.rs`](src/main.rs), [`serial.rs`](src/serial.rs) | `make rust-check`; `make rust-serial-test`; [Rust boot/READY workflow](../.github/workflows/rust-boot.yml) |
| Exception descriptors and fault entry | [`arch.rs`](src/arch.rs) | [M03 exception QEMU workflow](../.github/workflows/rust-exceptions.yml) |
| Limine memory map and frame selection | [`boot_memory.rs`](src/boot_memory.rs), [`memory.rs`](src/memory.rs) | `make rust-memory-test`; [M04 memory workflow](../.github/workflows/rust-memory.yml) |
| Symbolic owner ledger | [`frame_ledger.rs`](src/frame_ledger.rs) (feature `boot-ownership-smoke`) | `make rust-ledger-test`; [M04 ownership smoke](../.github/workflows/rust-memory.yml) |
| Frame reservation registry and state | [`frame_registry.rs`](src/frame_registry.rs), [`kernel_state.rs`](src/kernel_state.rs) | `make rust-frame-registry-test`; [M04 boot integration](../.github/workflows/rust-memory.yml) |
| Boot address provenance and one-time pool gate | [`boot_addresses.rs`](src/boot_addresses.rs), [`address_provenance.rs`](src/address_provenance.rs), [`boot_pool_gate.rs`](src/boot_pool_gate.rs) | `make rust-provenance-test`; `make rust-boot-pool-gate-test` |
| Paging and CPU protection observations | [`paging_mode.rs`](src/paging_mode.rs), [`research/`](research/) | `make rust-paging-mode-test`; `make rust-paging-research-test`; `make rust-pte-fields-test`; [Rust boot QEMU preflight](../.github/workflows/rust-boot.yml) |
| Deliberate permission faults | [`permission_fault.rs`](src/permission_fault.rs) (features `fault-ro-write` or `fault-nx-exec`) | [M03 QEMU fault-mode workflow](../.github/workflows/rust-exceptions.yml) |
| Experimental inherited-stack probe | [`transition_stack.rs`](src/transition_stack.rs) (feature `transition-stack-smoke`) | `make rust-transition-stack-test`; [Rust boot QEMU stack smoke](../.github/workflows/rust-boot.yml) |

Host tests live in [`tests/`](tests/) and in module-local `#[cfg(test)]`
sections. The [`research/`](research/) directory contains numerical
page-table models; it does not implement a live page-table writer. Use
[`UNSAFE_BOUNDARIES.md`](UNSAFE_BOUNDARIES.md) for privileged-code
preconditions, and consult the [milestone notes](M06E_TRANSACTIONAL_RESERVATIONS.md)
for the distinction between validated bookkeeping and enforced isolation.
The unsafe inventory is a source-linked historical audit, not a continuously
verified list of every later change; recheck its assumptions against current code.

**Review rule:** A host test of a model cannot substitute for QEMU evidence
when a change touches a hardware trust boundary. Conversely, a QEMU READY
marker does not prove memory isolation or safety on physical hardware.

## Fresh Ubuntu/WSL reproduction

Install rustup from https://rustup.rs and system prerequisites:

    sudo apt-get update
    sudo apt-get install -y make binutils python3 qemu-system-x86 xorriso curl tar
    cd rust-kernel
    rustup toolchain install 1.90.0 --component clippy,rustfmt --target x86_64-unknown-none
    cd ..
    make rust-check
    make rust-iso

Headless BIOS/QEMU:

    qemu-system-x86_64 -cpu max -m 256M -cdrom build/embers-rust.iso -display none -serial stdio -monitor none -no-reboot

Expected serial marker:

    READY: Embers Rust no_std entry reached.

The kernel intentionally stops with interrupts disabled in a halt loop.
Stop QEMU with Ctrl+C.

## M02 tested QEMU CPU baseline (issue #9)

This section documents measured **emulator-only** compatibility evidence for the
Rust kernel boot path. It does not claim physical-hardware compatibility.

- Host OS: Ubuntu 24.04.5 LTS (`PRETTY_NAME="Ubuntu 24.04.5 LTS"`)
- QEMU: `QEMU emulator version 8.2.2 (Debian 1:8.2.2+ds-0ubuntu1.18)`
- Pinned Rust toolchain: `1.90.0` from `rust-toolchain.toml`
- Active build toolchain in `rust-kernel/`: `rustc 1.90.0`, `cargo 1.90.0`
- Build commit: `acd442192b5e0aa46eadf5ccb13ce721b399a2e0`

Method: for each CPU model, run `make clean && make rust-iso`, then boot once
with `-d int,guest_errors -D <debug-log>` and serial capture.

| QEMU CPU model | Why included | Exit status | READY | ERROR marker | QEMU exception/debug notes |
| --- | --- | --- | --- | --- | --- |
| `max` | Existing permissive control profile | `124` (timeout after successful boot halt-loop) | yes | no | Only repeating `Servicing hardware INT=0x08`; no `guest_errors` exceptions observed |
| `qemu64` | Conservative named baseline model from `qemu-system-x86_64 -cpu help` | `124` | yes | no | Same as control; no additional exception diagnostics |
| `max,-avx` | Feature-subtracted control to check AVX independence on this QEMU version | `124` | yes | no | Same as control; no additional exception diagnostics |

No model in this matrix failed. Therefore failure classification by
fault address/instruction/compiler flags is not applicable for this run.

Recommended CI smoke profile: keep `-cpu max` as control and add a focused
`-cpu qemu64` lane so regressions against the conservative tested emulator model
are caught automatically. Minimum **tested emulator model** from this evidence:
`qemu64` on QEMU 8.2.2. This remains an emulator measurement, not a real-hardware
or production-support guarantee.

The ISO script downloads an explicitly pinned Limine v12.9.6 archive
and verifies its SHA-256 before extraction. No hosted model services
or paid APIs are involved.

## Unsafe boundary audit

Before changing CPU mappings, bootloader handoff, descriptor tables or fault
entry paths, review the [source-linked unsafe inventory](UNSAFE_BOUNDARIES.md).
It separates Rust memory-safety assumptions from privilege enforcement and
CPU/device quiescence, none of which the current bookkeeping proves.

## Safety limits

- Limine provides long mode, page tables and a stack. These are
  inherited, not independently controlled or proven by the Rust kernel.
- Privileged UART I/O, bootloader response reads, CLI/HLT require
  narrowly scoped unsafe Rust, with explicit entry assumptions.
- No processes, kernel-owned page tables, ring-3 isolation, IPC,
  capability enforcement, revocation, resource scheduler or DMA safety.
- The ELF script verifies structure; CI's QEMU READY assertion is
  separate execution evidence, not production hardware validation.

## Verified follow-on milestones

- [M03](M03_EXCEPTIONS.md): kernel GDT/TSS/IDT and QEMU exception fault tests.
- [M04](M04_PHYSICAL_MEMORY.md): Limine map validation and monotonic frame accounting.
- [M05](M05_FRAME_LEDGER.md): bounded symbolic frame owners and quarantine-only
  transitions. This demonstration is not persistent hardware ownership.
- [M06a address provenance](M06A_BOOT_PROVENANCE.md): Limine HHDM and executable
  base metadata preflight; no dereferenced new mappings or page-table switch.
- [M06e transactional reservations](M06E_TRANSACTIONAL_RESERVATIONS.md): one
  BSP-owned path that stages pool+registry validation before committing a
  reservation record; still bookkeeping, not hardware memory isolation.

Run host tests from repository root:

    make rust-memory-test
    make rust-ledger-test
    make rust-provenance-test

The normal kernel boot no longer executes the ownership test scenario. To run
the M05 QEMU smoke test explicitly, build and boot with:

    make rust-iso RUST_FEATURES="--features boot-ownership-smoke"
    make rust-run RUST_FEATURES="--features boot-ownership-smoke"

The M04/M05 memory CI runs this feature and checks cross-owner rejection and
permanent quarantine. This evidence is limited to the emulator demonstration.

The next major isolation milestone requires persistent frame ownership and
kernel-controlled page tables; then a validated ring-3 boundary and IPC.
These do not exist in the current kernel.
