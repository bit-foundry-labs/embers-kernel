# M06r: Read-only reserved-frame HHDM translation evidence

**Status:** Host-side QEMU observation under the existing Limine CR3. This is a prerequisite to studying real page-table backing; it is **not** a frame-ownership, zeroing, page-table-construction, or isolation milestone.

## Question

The live kernel obtains one monotonic physical frame pool from a validated Limine memory map and reserves two adjacent 4 KiB frames for boot diagnostics. It also observes the Limine Higher-Half Direct Map (HHDM) offset, but only checks the arithmetic needed to derive virtual-address candidates. Before storing anything in a reserved frame, verify whether the inherited page tables actually map those candidates to the reported physical frames.

## Test design

The new host checker, scripts/check_reserved_frame_hhdm_qmp.py, boots the **unmodified normal Rust kernel image** with QEMU, reads its existing serial checkpoints, and requires exactly one HHDM offset, the existing `PAGING: active four-level mode validated` checkpoint, and both adjacent, page-aligned frame claims. It verifies canonicality, numeric bounds and placement inside the test's configured 256 MiB physical-address bound.

After READY, the host pauses its disposable QEMU guest through QMP. It compares the live CPU's CR3 against the serial snapshot and validates MAXPHYADDR using the existing paging preflight parser. It then uses **read-only physical xp queries** to walk the inherited four-level hierarchy for the first and last bytes of each reserved frame's HHDM mapping.

The walker accepts standard x86-64 **4 KiB or 2 MiB** leaves, because a direct map may use large pages. It rejects 1 GiB PDPTE leaves until independently logged Page1GB capability evidence is available; this keeps the `qemu64` profile fail-closed. At each traversed entry it requires presence and writable PTE bits and checks address-width constraints. It computes the effective U/S restriction across the full path: an ancestor or leaf with U/S=0 blocks user access, while U/S=1 on every level is rejected. This avoids mistaking a user-set intermediate entry for an effectively user-accessible mapping. Huge leaves additionally reject reserved low physical-address bits. Both translated endpoints must equal the exact reserved physical frame plus byte offset. Observed leaf sizes are reported, not assumed.

Pure host fixtures assert the expected rejection for each targeted guard: invalid/duplicate boot metadata, missing four-level-mode evidence, nonadjacent reservations, noncanonical/overflowing addresses, invalid CR3 roots, table and leaf RAM bounds, absent/read-only/user-accessible paths including 2 MiB leaves, malformed huge leaves, unsupported 1 GiB leaves, MAXPHYADDR failures, permitted 2 MiB PAT bit 12, and wrong physical backing. CI runs both fixtures and the live checker under qemu64 and max after existing normal-boot ELF/QMP checks.

## Important nonclaims

- The existing two frames are **boot accounting reservations**, not authorized or initialized new page-table frames.
- **No guest physical address is dereferenced or written** by this experiment. In particular, the checked frame bytes are not inspected, zeroed, or assigned to a page-table role.
- HHDM translations and PTE RW bits do not establish effective permissions, full reserved-bit validity, physical memory type, unambiguous alias freedom, IOMMU/DMA ownership, exclusive allocation against other subsystems, or safe lifetime after Limine memory reclamation.
- The checker validates selected fields only. It checks effective U/S across the walked path, but not CR0.WP, SMEP, SMAP, PKE, NX, cache controls, complete PAT semantics, global entries, or TLB invalidation.
- The configured `0..256 MiB` QEMU physical-address bound is a bounded test range, not proof that every address in that interval is usable RAM.
- No change to CR3, to the privileged Rust kernel, to its ordinary READY behavior, or to the independent split-phase device revocation research.
- Draft PR #66's fail-closed CR3 transition gate **must remain blocked**. A future table builder needs a unique, persistently reserved frame source; verified writable access, zero initialization; complete mapping coverage; and a separately reviewed switch protocol.

## Reproduce

- Run python3 scripts/check_reserved_frame_hhdm_qmp.py --self-test to exercise bounded host fixtures without launching QEMU.
- Use make rust-iso, then python3 scripts/check_reserved_frame_hhdm_qmp.py --cpu qemu64 --iso build/embers-rust.iso (repeat for max) to inspect disposable QEMU's inherited translations.

Treat results as specific to the **exact tested build, guest configuration, and live CR3**, never proof that arbitrary physical RAM is safe to modify.
