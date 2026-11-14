# M06k: Feature-only stack physical-translation observations

**Scope:** Experimental QEMU verification of the four static BSS buffer pages
under the *inherited* Limine page tables; no CR3 or RSP modification.

## What is different from the normal ELF test?

PR #72 inspected the complete RX/R/RW segment page coverage for the normal
kernel image. With `transition-stack-smoke`, Rust includes a 16 KiB,
4 KiB-aligned `static mut` buffer in the feature image's BSS. That changes
the linked ELF layout and can change Limine's physical placement.

This increment makes the BSP report the buffer's **virtual start address**
only in the feature-gated diagnostic, after the existing volatile write/read/
restore smoke succeeds. The host observer must not reuse the normal kernel
image's memory layout or its physical-address candidates.

## Live QEMU test

The existing Rust boot matrix separately builds the feature-gated ELF and
ISO. In an additional disposable QEMU instance on both `qemu64` and
`max`, the host-only `check_active_pte_qmp.py --feature-stack`:

1. Waits for READY plus exactly one feature stack address and smoke marker.
2. Confirms the reported 16 KiB virtual extent is page-aligned, canonical
   and wholly inside the *feature* ELF's RW PT_LOAD segment.
3. Pauses the guest via QMP and confirms its actual CR3 matches the boot
   serial checkpoint.
4. Uses read-only physical `xp` monitor queries to inspect **every page**
   of that ELF's RX/R/RW LOAD segments and the first and last byte of each
   of the four static-stack pages, comparing the resulting physical addresses
   against independently derived Limine candidates.
5. Rejects missing/nonpresent entries, unsupported large pages, mismatched
   backing, four-page aliasing **within this sampled buffer**, and PTE paths
   whose observed RW bits do not permit writes.
6. Reports NX and RW bits as observations, without asserting hardware
   enforcement or mapping safety under a different root.

Host negative tests cover malformed/missing or duplicated feature metadata,
out-of-range and unaligned buffer addresses, wrong intermediate physical
backing, missing page presence, and read-only leaf entries. QMP failures are
not masked, and the ordinary kernel remains unchanged.

## Nonclaims and next milestones

The static buffer is **not the CPU stack**. A successful test is a snapshot
of four mapped pages under Limine's tables in the tested emulator profile.
It does **not** establish global absence of aliases, actual frame ownership,
DMA/IOMMU protection, live exception/IST/descriptor translations, or memory
safety following a new CR3 root. The test also does not resolve MAXPHYADDR,
reserved entry bits, EFER.NXE or CR0.WP enforcement.

An eventual address-space transition must use separately authorized, zeroed
and tracked page-table frames and must preserve all live code/data, CPU
descriptor and fault paths, with explicit failure-atomic rollback behavior.
No CR3 write is introduced or permitted by this work.
