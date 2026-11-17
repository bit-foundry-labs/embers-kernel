# M06o: Read-only live descriptor and IST1 mapping evidence

**Scope:** An opt-in x86-64/QEMU experiment under Limine's inherited CR3. It does not construct or install page tables, switch RSP, or change normal boot.

## Guest observation

The `descriptor-evidence` feature runs after GDT/TSS/IDT installation on the single bootstrap CPU. It samples GDTR and IDTR via `SGDT`/`SIDT` and the task register selector via `STR`. The observed GDT/IDT bases and limits must match the kernel statics; the selector must be the installed TSS selector. A raw unaligned read of TSS.IST1 must equal the top of the private 16 KiB double-fault stack.

Only this opt-in feature reports the start and length of the four live objects: GDT (40 bytes), IDT (4096 bytes), TSS (104 bytes), and IST1 (16384 bytes). A mismatch emits ERROR and halts, not READY.

## Host mapping checks

`scripts/check_descriptor_qmp.py` validates exactly one complete, noncontradictory descriptor report. It rejects unexpected selectors, noncanonical or overflowing addresses, sizes that differ from the compiled layout, and any extent outside the matching ELF's RW PT_LOAD segment.

It independently derives the segment's physical candidate using the feature ELF and that guest's Limine-reported executable bases. Once a QEMU guest reaches READY, the host pauses it and compares the live CPU's CR3 with the serial observation. For each descriptor extent, it walks the inherited four-level 4 KiB page tables at the first byte, every crossed page boundary, and the final byte. Every translated address must match the expected physical candidate and each PTE chain must allow writes.

The double-fault IST buffer is only **16-byte aligned**. A 16 KiB interval can therefore intersect **five** pages rather than four. The checker calculates the actual intersection count instead of assuming four aligned pages.

A host-only self-test covers malformed and duplicated reports, wrong selector and TSS length, noncanonical and truncated ranges, bad physical mappings, missing PTEs, and read-only PTEs. The boot workflow requires this feature on both `max` and `qemu64` CPU profiles; existing fault, stack and ELF checks remain in place.

## Evidence limits and next dependency

This proves only the reported numeric state and the observed inherited memory translations in disposable QEMU. `STR` does not expose the hidden TR cache; this snapshot does not prove its cached base. Neither does it establish the *complete* Limine entry-stack extent, global alias freedom, all reserved bits, independently authorized physical frames, mapping lifetimes under replacement tables, hardware interrupts under a new CR3, or DMA isolation.

The fallible mock gate in draft PR #66 must remain blocked. A separate stage needs complete live-stack provenance and a reviewed physical-frame ownership and mapping construction mechanism before any actual CR3 transition.
