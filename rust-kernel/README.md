# Embers Kernel: Rust-only Limine/QEMU research baseline

This is an original, freestanding Rust no_std x86-64 kernel entry, loaded
directly by **Limine**. Limine is an external bootloader; there is no
intermediate C kernel execution stage.

The former C bootstrap lives in the historical research branch PR #2.
The Rust build reuses a linker script and checksum-pinned Limine ISO
packaging machinery, but neither compiles nor links the C kernel.

## Fresh Ubuntu/WSL reproduction

Install rustup from https://rustup.rs and system prerequisites:

    sudo apt-get update
    sudo apt-get install -y make binutils gcc libc6-dev python3 qemu-system-x86 xorriso curl tar
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

`gcc` and `libc6-dev` are required even though the kernel itself links
without them: the host tests link through the system `cc`, and
`make rust-iso` compiles the Limine `limine` utility from C. GitHub-hosted
Ubuntu runners preinstall a C toolchain, so CI does not detect its absence.
In a clean `ubuntu:26.04` container, the package list without them let
`make rust-check` pass, but `make rust-memory-test`, `make rust-ledger-test`
and `make rust-iso` failed with ``linker `cc` not found``. Adding only `gcc`
still failed, because `Scrt1.o`, `crti.o`, `libutil` and `stdio.h` come from
`libc6-dev`.

If rustup is installed with Homebrew instead of https://rustup.rs, the formula
is keg-only. Add `$(brew --prefix rustup)/bin` to `PATH` so that `cargo` and
`rustc` resolve to the rustup proxies that honor `rust-toolchain.toml`.

## Ubuntu 26.04 reproduction record (issue #8)

This record is **emulator-only** evidence. The clean-environment check ran
the commands above in a fresh `ubuntu:26.04` container (image digest
`sha256:d66b366dac61b575bee2aef07e9c26e8070096fbe84a3d4d3eb0da069b5d76db`)
with rustup from https://rustup.rs. With the updated package list, every
command below passed, and QEMU (software emulation, `-cpu max`) reached
`READY` with no `ERROR:` line. The table records a workstation run, where
Rust was not installed beforehand.

- Host OS: Ubuntu 26.04.1 LTS, x86_64
- QEMU: `QEMU emulator version 10.2.1 (Debian 1:10.2.1+ds-1ubuntu3.2)` (apt)
- Rust: `rustc 1.90.0 (1159e78c4 2025-09-14)` and
  `cargo 1.90.0 (840b83a10 2025-07-30)` via Homebrew rustup and
  `rust-toolchain.toml`; xorriso `1.5.8.pl02` via Homebrew
- Build commit: `db9850c23713453d2ebd450b8e910f7b51a78558`

| Command | Result |
| --- | --- |
| `make rust-check` | Passed |
| `make rust-memory-test` | Passed: 16 tests |
| `make rust-ledger-test` | Passed: 26 tests |
| `make rust-iso` | Passed; Limine v12.9.6 SHA-256 verified |
| Headless QEMU, `-cpu max` and `-cpu qemu64` | `READY` reached; no `ERROR:` line |
| Ctrl+C with `-serial stdio` | QEMU reported `terminating on signal 2` and exited with status `0` |

Serial output from the workstation run with `-cpu qemu64` follows. It is
reproduced in full; it contains no host-specific information.

    EMBERS: Rust no_std / x86_64 / boot M02
    BOOT: Limine base revision accepted
    BOOT: x86-64 GDT/TSS/IDT installed
    PAGING: active four-level mode validated
    PAGING: inherited CR3 raw = 0x000000000ff9c000
    PAGING: root frame candidate = 0x000000000ff9c000
    PAGING: observed BSP RSP = 0xffff80000ff94d90
    PAGING: MAXPHYADDR bits = 0x0000000000000028
    PAGING: maximum linear bits = 0x0000000000000030
    PAGING: CPU protection controls active (CR0.WP, EFER.NXE, EFER.LMA)
    ADDRESS: Limine HHDM and executable bases validated
    ADDRESS: HHDM offset = 0xffff800000000000
    ADDRESS: executable physical base = 0x000000000ff42000
    ADDRESS: executable virtual base = 0xffffffff80000000
    MEMORY: validated Limine map for physical frame accounting
    MEMORY: frame claim 1 = 0x0000000000100000
    MEMORY: frame claim 2 = 0x0000000000101000
    MEMORY: remaining pages in selected span = 0x000000000000fe3c
    MEMORY: boot reservation retained after initialization
    READY: Embers Rust no_std entry reached.

The `-cpu max` log is identical except for one line,
`PAGING: maximum linear bits = 0x0000000000000039`, because that model
reports 57-bit linear addresses. The kernel still validates four-level
paging before `READY`.

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
