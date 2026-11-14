# M06j: Bounded page-level inspection of every ELF load segment

**Status:** Host-side QEMU research evidence only; no page-table writes, no
new physical frame authority, and no kernel-side physical-memory dereferences.

## Research question

M06i matched the active 4 KiB translation of the **first kernel code byte**
to the Limine-derived physical candidate. This is insufficient to infer that
all of kernel text, read-only data or writable data/BSS has the expected
physical backing. A missing or incorrectly mapped interior page could be
invisible to a single-address test.

## This increment

`scripts/check_active_pte_qmp.py` continues to boot a separate ordinary
QEMU guest, wait for READY, pause through QMP, and compare current CR3 to
the value reported before the boot halted.

It now inspects **each intersected 4 KiB page** of all three ELF PT_LOAD
segments (RX, R, RW), plus the last valid byte of each segment. For every
sampled address, it walks four present 4 KiB page-table levels through
read-only QEMU guest-physical `xp` commands. The resulting physical address
must equal the independently computed candidate:

```text
expected_physical = limine_executable_physical_base
                  + (sampled_virtual - limine_executable_virtual_base)
```

The script aborts if the kernel LOAD inventory is incomplete, any page
is nonpresent, any physical translation differs, an intermediate large-page
entry is encountered, or a segment exceeds 64 intersected pages. QMP
entry reads are cached only while the emulator is paused; no mutation
occurs between observations. It also reports the observed RW-chain and
NX-bit patterns **as raw PTE-bit observations**, not effective access-control
proofs.

The host self-tests include multiple page boundaries, the final segment
byte, invalid inventory, nonpresent interior and final pages, unexpected
physical backing, and oversized scan requests. Both QEMU CPU-profile jobs
run the same bounded check against the normal kernel image, independently
of the feature-only transition-stack image.

## What passing would mean

For the specific ordinary kernel ELF and paused QEMU snapshot, every sampled
4 KiB ELF load page and the final byte has a present translation consistent
with Limine's executable addresses. This extends beyond one text entry, but
does **not** prove permissions under future tables, physical-frame uniqueness,
alias freedom, MAXPHYADDR, reserved-bit validity, EFER.NXE/CR0.WP behavior,
interrupt/IST mapping lifetime, or IOMMU/DMA isolation.

The 16 KiB **feature-gated** transition stack is a different ELF image.
Its page-table coverage needs a separate build and matching QMP observation;
the normal ELF physical offsets must never be substituted for that image's.

No CR3 or RSP write is authorized by this test. Future work should
validate physical-width and permission semantics, and then inventory
descriptor tables, active stacks and exception paths with comparable
read-only evidence before any mapping transition research.
