# M06s: Retain the copied boot memory map before paging work

**Status:** Narrow kernel-state/data-retention change; not a page-table implementation or a demonstration of exclusive physical ownership.

## Source-level problem

As of baseline `main` commit `8fdae2bcf4e761333a211282206b08a681e8a319`, `boot_memory::initial_frame_pool` copied up to 128 Limine memory-map entries into a local fixed array, passed the array to `select_boot_pool`, and returned **only** a `FramePool`. The classification array was dropped at function exit. `KernelState` retained the transactional allocator, not the map that was used to choose its original range.

That is adequate for today's single-BSP numerical frame accounting. It is not enough to plan reclamation or an independent authority audit after Limine-owned response pointers may become inaccessible. A numeric `USABLE` region classification is also **not** an authenticated frame capability.

## Bounded change

- `memory::BootMapSnapshot` owns one fixed-capacity `[Region; 128]` and a checked entry count, exposes an immutable slice only, and does not retain any bootloader-owned pointers or permit mutation via its public API.
- `boot_memory::BootPoolSelection` returns the **same** validated numeric frame pool plus the copied array. The original permit consumption, entry-count/pointer checks, selection algorithm, and failure behavior remain unchanged.
- `check_boot_memory` moves the copy and the allocator into `KernelState`; the existing `run_kernel` path checks that the retained map is not empty, without introducing a new success marker.
- Host tests check bound rejection (zero or above-capacity counts), deep-copy ownership, immutable exposure, original region kind preservation, and survival of the move to `KernelState`.
- `MAX_BOOT_REGIONS = 128` and `Region = 3 x u64` imply **3,072 bytes** of region values retained in `KernelState`, plus metadata/padding. The code is `no_std` and heap-free. During the handoff, both the local `[Region; 128]` and the returned/moved snapshot may contribute to peak inherited-BSP-stack usage; Rust's value-move semantics do **not** prove a zero-copy move or a fixed maximum stack footprint. Review generated code, stack allocation and mapping/lifetime assumptions separately before any paging or stack transition.

## Critical nonclaims

1. **Trust boundary unchanged:** `read_volatile` of nonnull Limine pointers still relies on valid, aligned, initialized, mapped bootloader responses. The memory snapshot is copied **after** this trusted read; it does not authenticate hostile pointers or their provenance.
2. **Memory types aren't authority:** `USABLE` is an input classification, not proof that no conflicting CPU aliases, device DMA mappings, stale TLB entries or externally constructed pools exist. `BootMapSnapshot::from_copied_entries` checks array length, **not** region geometry; successful *production selection* separately validates geometry through the selector. Limine guarantees `USABLE` (kind 0) and `BOOTLOADER_RECLAIMABLE` (kind 5) entries are disjoint from every entry; **other non-usable kinds can overlap one another**. A retained snapshot is not a globally disjoint partition and must not be used for reclaim/accounting without consumer-specific validation.
3. **No reclamation:** `BOOTLOADER_RECLAIMABLE` and `ACPI_RECLAIMABLE` entries remain **ineligible** for frame allocation. Preserving their metadata does not allow reuse or release any old Limine stack/page tables, firmware structures or in-flight accesses.
4. **No writes or privileged changes:** No frame zeroing, HHDM dereference, page-table construction or `CR3` replacement. This cannot establish safe writable pointer creation or new hardware-enforced address spaces.
5. **Overlap compatibility:** [PR #104](https://github.com/bit-foundry-labs/embers-kernel/pull/104), merged at `dd63164`, updates the selector to reject overlaps involving kind 0 or 5 while permitting overlap among other non-usable kinds. Snapshot consumers must not assume all regions are mutually disjoint.
6. **Value ownership is not physical lifetime:** The copy no longer borrows Limine's memory-map pointer chain, but the values are accessible only while the physical pages and virtual mappings backing their actual storage remain valid. An independent review of release build `91480b8` found `KernelState` on the inherited Limine stack; that stack is bootloader-reclaimable. Before retiring or unmapping it, the state must be preserved in storage whose backing and mappings survive the transition. The reviewer estimated at least 13,120 bytes for the relevant call chain in that build; this is not a peak bound for other builds, features, or exception paths. Neither a Rust move nor a passing QEMU boot proves reclamation safe.

Limine's upstream [Memory Map feature](https://github.com/limine-bootloader/limine-protocol/blob/trunk/PROTOCOL.md#memory-map-feature) defines the entry types, including usable, bootloader-reclaimable and executable/modules. Its [x86-64 machine-state contract](https://github.com/limine-bootloader/limine-protocol/blob/trunk/PROTOCOL.md#x86-64) gives an initial BSP stack in bootloader-reclaimable memory. Consult these contracts when specifying actual transition lifetime. Compare [seL4's untyped memory capabilities](https://docs.sel4.systems/Tutorials/untyped.html) for authority modeling: Embers' current copy is metadata only.

## Acceptance gates

1. Exact-head Rust format, Clippy, memory/host and both QEMU boot profiles pass; intentional exception modes still work.
2. Independent reviewer confirms all uses of `initial_frame_pool` and `KernelState::new` were updated and that the new retained array does not cross an unintended trust or mutation boundary.
3. Reviewer examines inherited BSP stack pressure from the local `[Region; 128]`, return-by-value `BootPoolSelection`, and transfer into `KernelState`; distinguish the **3,072-byte retained data size** from potentially larger transient stack/storage and compiler-generated move/copy costs. Check stack and object physical backing, aliases and the mapping lifetime required by any future stack or `CR3` transition. A green QEMU boot establishes only the tested boot paths, not a worst-case stack bound or a reclamation proof.
4. The next M06 experiment must demonstrate the **specific** CPU physical translation, permissions, initialization and preservation window for a candidate table frame. Use the [read-only HHDM experiment #83](https://github.com/bit-foundry-labs/embers-kernel/pull/83) as observation, not authority.
5. [Issue #84](https://github.com/bit-foundry-labs/embers-kernel/issues/84) is **closed** following completion of the read-only review work for [#81](https://github.com/bit-foundry-labs/embers-kernel/pull/81) and [#83](https://github.com/bit-foundry-labs/embers-kernel/pull/83). Their observations do **not** authorize a `CR3` change. The active pre-write ownership, backing, and lifetime gates are tracked in [issue #110](https://github.com/bit-foundry-labs/embers-kernel/issues/110); those gates remain open.

**Exit criterion:** The kernel can inspect copied memory classifications after initialization without dereferencing the original Limine memory-map pointer chain. This is a preparatory architectural data-lifetime property, not a new security boundary.
