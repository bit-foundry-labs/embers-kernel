# M06t: observed reserved-frame disjointness from inherited paging-structure pages

**Status:** A read-only, host-side QEMU **observation**, extending the merged [M06r PR #83](https://github.com/bit-foundry-labs/embers-kernel/pull/83). **No guest writes, ownership transfer, stack switch, or CR3 replacement.** Any claim below is limited to the exact build, paused guest, observed inherited CR3, and the table-entry paths actually traversed.

## The question not answered by the existing HHDM checks

M06r's [HHDM checker](../scripts/check_reserved_frame_hhdm_qmp.py) verifies the first and last byte of each of two numerically reserved boot frames translate to the reported physical addresses through the inherited Limine page tables. It already checks selected present, writable, effective supervisor, huge-leaf, MAXPHYADDR and physical-RAM bounds. However, a valid-looking HHDM leaf mapping by itself does **not** prove that the same physical frame isn't being used to store a live page-table structure.

The next safe observation is: **for the page-table walks used by this existing HHDM checker, is any candidate reservation frame also one of the active paging-structure pages being read?**

## Narrow implementation

The existing `inspect_reservations` function now wraps its page-table-entry reader to track the 4 KiB physical page containing every PML4E, PDPTE, PDE, and PTE actually visited by the two candidates' first/last-byte HHDM walks. **Before reading** each entry, it rejects a paging-structure page that equals either candidate reservation. After successful translation, it confirms the inherited root was included. The observed set is reported as a count, not promoted into a capability or emitted as a guest physical-address disclosure.

This reuses `walk_hhdm`, `Qmp`, the existing read-only `xp` path, CPU model matrix, and test fixture helpers. No second page-table walker, new production module, extra dependency, or new CI workflow is introduced.

Pure host fixtures specifically exercise:
- Clean 4 KiB walks: four visited paging-structure pages, all disjoint from the two candidate reservations.
- Clean 2 MiB walks: three visited paging-structure pages and no imaginary leaf PT.
- Deliberate collision where either candidate is the inherited CR3 root.
- Deliberate collision where an intermediate PDE points to a candidate frame as its page-table page, even when a supposed leaf translation could otherwise look correct.
- Existing wrong-backing, malformed PTE, supervisor permissions, MAXPHYADDR and huge-page rejection cases remain intact.

**Execution:** the existing M06r workflow runs `python3 scripts/check_reserved_frame_hhdm_qmp.py --self-test` and the same live checker for `--cpu qemu64` and `--cpu max` on the matching normal-boot ISO.

## Why this is not a lifetime or isolation proof

- **Limited enumeration:** only tables on the four examined HHDM paths are inspected. Other translations, global aliases, executable mappings, inherited stack mappings, idle paging branches and device/IOMMU mappings are not enumerated. A candidate could still alias another active use outside this sample. Do not label the result “globally alias-free.”
- **Temporal bound:** a paused QEMU observation has a single snapshot time. It does not establish that page tables, bootloader response allocations, kernel stacks or DMA have stopped using memory later.
- **Protocol assumption:** Limine documents its inherited page tables and BSP stack as **bootloader-reclaimable**, distinct from normal `USABLE` entries. The numeric frame pool trusts this classification. The new check helps catch a mismatch **on observed paths**, but does not authenticate the bootloader or replace complete map validation.
- **Stack difference:** existing [PR #81](https://github.com/bit-foundry-labs/embers-kernel/pull/81) samples two small RSP-adjacent locations. [PR #82](https://github.com/bit-foundry-labs/embers-kernel/pull/82) investigates a dedicated, kernel-image BSS stack under the **old** mappings. Neither proves the **entire inherited BSP stack's physical extent**, safe reclamation, or survival of a new CR3. This change does not add a stack transition.
- **Candidate vs. owned table frame:** these are still two boot-accounting frame numbers. No frame clearing, physical write, page-table installation, CPU/DMA isolation or enforced ownership exists. A successful disjointness check is necessary supporting evidence for future work, not authorization to dereference the frame from Rust or overwrite it.

## Next M06 gates, in order

1. Independently review this change against current main, including the already merged [PR #81](https://github.com/bit-foundry-labs/embers-kernel/pull/81) and [PR #83](https://github.com/bit-foundry-labs/embers-kernel/pull/83). Recheck the resulting commit and integration tests when the base changes.
2. Resolve the *authority* and *lifetime* problem: build on the retained boot-map classifications from merged [PR #107](https://github.com/bit-foundry-labs/embers-kernel/pull/107), which are not physical authority or a lifetime proof; define table-frame reservation, initialization, ownership and forbidden aliasing paths; prove the dedicated stack and exception paths persist across old/new roots.
3. Only after evidence and independent approval, design a **separate** gated table-frame zeroing and mapping experiment. The fail-closed CR3 model in [draft PR #66](https://github.com/bit-foundry-labs/embers-kernel/pull/66) does not authorize a live root switch.

Primary contracts: [Limine boot protocol: memory layout, paging structures and stack](https://github.com/limine-bootloader/limine-protocol/blob/trunk/PROTOCOL.md); [Embers M06 paging transition requirements](../docs/PAGING_TRANSITION_V1.md). This research does not claim production safety or a complete page-table enumeration.
