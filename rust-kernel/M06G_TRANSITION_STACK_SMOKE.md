# M06: Feature-gated transition-stack storage smoke

**Status:** Opt-in QEMU measurement under Limine's inherited page tables.
No new CR3 root, no stack-pointer switch, no page-table writes.

## Why this exists

PR #68 observed the inherited CR3 and BSP stack pointer, but these numbers
do not establish any transition-stack extent. Before a future root switch,
Embers needs deliberately allocated storage that remains mapped and writable
in both the old and the new hierarchy. Its instruction stream, GDT/TSS/IDT,
exception handlers, constants, data and boot references must survive as well.

This incremental experiment defines a 16 KiB kernel-image BSS buffer aligned
to 4096 bytes. The `transition-stack-smoke` feature validates the numeric
start/end range, then performs volatile write/read/restore operations at the
first and last byte of each of its four pages. A QEMU pass demonstrates that
these sampled addresses were readable and writable through the **existing**
Limine mappings on the tested CPU profiles.

The buffer is **never loaded into RSP**. It is not allocated by the M06 frame
registry and does not prove separate physical frames, lack of aliasing,
device protection, post-CR3 mapping survival or complete stack ownership.
Neither the current BSP stack nor the double-fault IST stack is modified.

## Evidence and reproduction

- `make rust-transition-stack-test`: dependency-free host tests for valid
  aligned/canonical ranges and rejected null, misaligned, wrongly sized,
  noncanonical and overflowing ranges.
- `make rust-iso RUST_FEATURES="--features transition-stack-smoke"`:
  explicitly build the gated experiment; default kernel boot excludes it.
- Rust boot CI independently builds and boots the feature profile under
  QEMU `qemu64` and `max`, requiring the STACK smoke marker and READY,
  with no ERROR marker. The ordinary boot profile and its CR3/RSP observations
  remain separate.
- Fault-injection feature combinations with the stack smoke are compile-time
  rejected. CI checks feature Clippy and host tests, and retains serial logs.

QEMU results must be read from the exact PR-head Actions run. A successful
host test or ordinary READY marker cannot substitute for the gated smoke.

## Remaining safety prerequisites

1. Verify kernel ELF PT_LOAD physical placement, actual loaded mappings and
   rights for live text, read-only data, BSS, stacks, descriptor tables and
   exception code.
2. Reserve and zero real page-table frames with persistent ownership and
   established writable access. HHDM arithmetic alone proves no mapping.
3. Design and independently test a stack switch, new-root page-table contents,
   CR3 transition protocol, and expected #PF/#DF survival in gated QEMU.
4. Retain deterministic no-write-on-failure behavior and distinguish software
   ownership from actual hardware or DMA isolation.

The result supports a small observation, not protected address spaces.
