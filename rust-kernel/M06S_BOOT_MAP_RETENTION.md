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
- `MAX_BOOT_REGIONS = 128` and `Region = 3 x u64` imply **3,072 bytes** of region values stored in the state, plus metadata/padding. The code is `no_std` and heap-free. This adds an owned object to early-boot state and must pass both QEMU profiles before review.

## Critical nonclaims

1. **Trust boundary unchanged:** `read_volatile` of nonnull Limine pointers still relies on valid, aligned, initialized, mapped bootloader responses. The memory snapshot is copied **after** this trusted read; it does not authenticate hostile pointers or their provenance.
2. **Memory types aren't authority:** `USABLE` is an input classification, not proof that no conflicting CPU aliases, device DMA mappings, stale TLB entries or externally constructed pools exist. `BootMapSnapshot::from_copied_entries` checks array length, **not** region geometry; successful *production selection* separately validates geometry through the existing selector.
3. **No reclamation:** `BOOTLOADER_RECLAIMABLE` and `ACPI_RECLAIMABLE` entries remain **ineligible** for frame allocation. Preserving their metadata does not allow reuse or release any old Limine stack/page tables, firmware structures or in-flight accesses.
4. **No writes or privileged changes:** No frame zeroing, HHDM dereference, page-table construction or `CR3` replacement. This cannot establish safe writable pointer creation or new hardware-enforced address spaces.
5. **Keep overlap compatibility separate:** [Draft PR #104](https://github.com/bit-foundry-labs/embers-kernel/pull/104) adjusts Limine non-usable overlap semantics. This change intentionally leaves the existing selector unchanged so its exact-base review remains distinct.
6. **Lifetime is only that of the Rust-owned copy:** The values survive after bootloader response mappings disappear, but the current inherited stack and kernel object physical backing still require separate mapping-lifetime and ownership evidence.

Limine's upstream [Memory Map feature](https://github.com/limine-bootloader/limine-protocol/blob/trunk/PROTOCOL.md#memory-map-feature) defines the entry types, including usable, bootloader-reclaimable and executable/modules. Its [x86-64 machine-state contract](https://github.com/limine-bootloader/limine-protocol/blob/trunk/PROTOCOL.md#x86-64) gives an initial BSP stack in bootloader-reclaimable memory. Consult these contracts when specifying actual transition lifetime. Compare [seL4's untyped memory capabilities](https://docs.sel4.systems/Tutorials/untyped.html) for authority modeling: Embers' current copy is metadata only.

## Acceptance gates

1. Exact-head Rust format, Clippy, memory/host and both QEMU boot profiles pass; intentional exception modes still work.
2. Independent reviewer confirms all uses of `initial_frame_pool` and `KernelState::new` were updated and that the new retained array does not cross an unintended trust or mutation boundary.
3. Reviewer examines early-boot stack/storage and code-size impact; a green QEMU run is evidence for tested profiles only.
4. The next M06 experiment must demonstrate the **specific** CPU physical translation, permissions, initialization and preservation window for a candidate table frame. Use the [read-only HHDM experiment #83](https://github.com/bit-foundry-labs/embers-kernel/pull/83) as observation, not authority.
5. [Issue #84](https://github.com/bit-foundry-labs/embers-kernel/issues/84) still gates independent reviews of read-only stack/HHDM observation PRs #81 and #83. Do not alter `CR3` while that gate is open.

**Exit criterion:** The kernel can inspect copied memory classifications after initialization without dereferencing the original Limine memory-map pointer chain. This is a preparatory architectural data-lifetime property, not a new security boundary.
