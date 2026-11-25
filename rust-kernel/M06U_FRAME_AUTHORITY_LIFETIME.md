# M06u: Physical-frame authority and lifetime contract

**Status (2026-11-20):** Host-only research state machine and design gate. **No physical frame is actually claimed, initialized, mapped, activated, retired, or released by this PR.** No new `unsafe`, page-table writes, guest memory access, CR3 change, external crate, or production kernel integration.

## Why this milestone exists

The boot loader's region descriptions, a numeric `FrameReservation`, HHDM address arithmetic, and QEMU evidence of a mapping are **not interchangeable with authority to use physical memory**. Existing Embers paths deliberately separate these claims:

- `memory.rs` selects a usable numeric pool under a one-shot BSP permit; `frame_registry.rs` records checked reservations. Bounded Kani pilots [#105](https://github.com/bit-foundry-labs/embers-kernel/pull/105) and [#106](https://github.com/bit-foundry-labs/embers-kernel/pull/106) explore failure atomicity, not authenticated ownership.
- [#107](https://github.com/bit-foundry-labs/embers-kernel/pull/107) proposes an owned copy of Limine's numeric memory classifications so references to reclaimable bootloader buffers can eventually be dropped.
- [#81](https://github.com/bit-foundry-labs/embers-kernel/pull/81) observes two RSP-adjacent points; [#82](https://github.com/bit-foundry-labs/embers-kernel/pull/82) explores a BSS stack switch **under inherited CR3**.
- [#83](https://github.com/bit-foundry-labs/embers-kernel/pull/83) and stacked [#108](https://github.com/bit-foundry-labs/embers-kernel/pull/108) observe HHDM backing and disjointness from **visited** active page-table pages; these are not complete alias, lifetime, or DMA proofs.
- Draft [#66](https://github.com/bit-foundry-labs/embers-kernel/pull/66) always blocks its mock CR3 transition because a real table builder and stack-survival evidence do not exist. This change **does not unlock #66**.

## The proposed monotonic lifecycle

The host model in [`research/frame_lifetime_contract.rs`](research/frame_lifetime_contract.rs) records one numeric span and its intended role (page table or transition stack). Its phases are:

| Phase | Meaning and required next-gate evidence | Forbidden assumption |
| --- | --- | --- |
| **Candidate** | Aligned, nonzero, nonoverflowing physical span, caller-classified as Limine `USABLE`; numerically disjoint from provided exclusions | A region type is not a kernel capability; input exclusions can be incomplete |
| **Reserved** | Candidate passes an asserted sole-issuer/no-other-claim condition | The host `true` flag cannot authenticate production allocation authority |
| **Prepared** | Verified physical backing and writable inherited mapping; exclusive write access; actual zero initialization | Numerical HHDM validity or read-only QMP checks do not justify Rust dereferences or prove zeroed bytes |
| **Active** | Complete preservation of executing text, data, stack, GDT/IDT/TSS/IST/fault path, old root/boot storage and single-BSP interrupt discipline | A passing host gate cannot install a new root or guarantee post-CR3 survival |
| **Retired** | No new references may be acquired; old active use is ending, but existing CPU/device references may remain | Retired does not mean free or safe to overwrite |
| **Quarantined** | Asserted CPU translation quiescence, device translation quiescence, and drained references | Even this host state has **no release or reuse operation**; actual revocation needs a separate implementation and review |

Every `advance()` rejects the wrong phase **before** considering evidence and leaves the original record unchanged if a guard is missing. Numeric span construction rejects arithmetic overflow, unaligned addresses and overlap against the supplied exclusions. No `advance()` step turns a quarantined record back into a candidate, and the model issues no Rust pointer. However, `FrameRecord` has public fields and is `Copy`: callers can construct or alter a record's phase or retain earlier copies. The API therefore does not enforce monotonicity, unique ownership, irreversible quarantine, or prevention of reuse.

**All `ClaimedEvidence` fields are deliberately caller-forgeable and untrusted.** This is a *requirements model*, not the future runtime API. The real implementation must replace each boolean with proven/enforced invariants tied to the sole allocator, actual translations and memory ownership. It must not wire this toy model into the boot path or let an AI/policy agent supply trusted proof flags.

## The future hardware/security proof obligations

1. **Single frame-authority root:** Precisely connect one trusted boot `USABLE` inventory to the production allocator; rule out independent overlapping issuers (including optional M05 smoke and module-specific bypass routes). Persist unique ownership identity and non-reusable reservations.
2. **Physical backing and safe initialization:** Confirm actual, writable, correctly typed and mapped 4 KiB frames before creating raw pointers; establish Rust provenance, alignment, alias exclusion, initialization and CPU memory-type/cache requirements. **An HHDM candidate alone cannot establish these facts.**
3. **Complete mapping-survival set:** Enumerate the executing instruction pages; kernel text/rodata/data/BSS; transition stack; exception IST, IDT/TSS/GDT; active allocator/ledger/boot-map storage; and old-root reachability. Pin exact physical backing throughout the move. The original Limine stack, response arrays and inherited tables remain held until their last users are gone.
4. **Table construction and CR3 gate:** Construct complete validated supervisor-only entries with checked widths, PTE permission semantics, zero initialization and appropriate synchronization. Never enter the new root until independent review and dedicated gated fault tests demonstrate mapped execution and diagnostic paths.
5. **Retirement vs. reclamation:** Document the CPU/TLB and IOMMU/DMA/accelerator quiescence conditions separately. In the initial one-BSP/QEMU scope, explicitly leave device isolation and generic reclamation **unsupported** rather than declaring a release based on an absent-device assumption. Global entries and SMP require their own TLB/epoch protocol before promotion.
6. **AI-native resource admission:** Future local AI services, agents, GPUs and NPUs may propose resource allocations, but proposal, prioritization and budgets remain in **untrusted user-space policy**. Only a kernel-authenticated request with rights, resource limits, persistent accounting and enforceable isolation could ever cause a transition in the real memory manager. No policy preference can override lifetime or quiescence.

## Host-test evidence and limits

The standalone module has no dependency on kernel boot execution. GitHub CI runs **the actual Rust source** through rustfmt and Clippy with `-D warnings`, then executes its tests:

```sh
cd rust-kernel
rustfmt --edition=2021 --check research/frame_lifetime_contract.rs
clippy-driver --edition=2021 --test research/frame_lifetime_contract.rs -D warnings -o /tmp/embers-m06-lifetime-gate
/tmp/embers-m06-lifetime-gate --nocapture
```

Tests cover rejected invalid ranges, non-usable inputs, overlaps, missing sole issuer, skipped phases, each missing initialization/mapping requirement, each absent activation-survival obligation, rejected early retirement, and each missing CPU/device/reference quiescence condition. The model has no release-to-free operation, but its public, copyable records do not enforce non-reuse or irreversible quarantine.

**These are host contract tests, not Kani proofs or live x86-64 security tests.** A green memory/QEMU CI suite confirms no observed regression on its tested profiles, not a verified physical-frame authority design. The proposed gate is a *review checklist*; only calls to `advance()` with missing evidence or an unexpected phase fail closed.

## External primary research

- [Limine boot protocol specification](https://github.com/limine-bootloader/limine-protocol/blob/trunk/PROTOCOL.md): usable, reserved, bootloader-reclaimable memory and inherited x86-64 machine state. Limine memory classifications are external boot inputs.
- [seL4 untyped memory tutorial](https://docs.sel4.systems/Tutorials/untyped.html) and [capability tutorial](https://docs.sel4.systems/Tutorials/capabilities.html): memory retyping and capability derivation/revocation. Adapt the *separation of authority from identity*, not an unverifiable claim that Embers already implements seL4's guarantees.
- [Linux kernel page-table documentation](https://docs.kernel.org/mm/page_tables.html), [kernel page tables](https://docs.kernel.org/mm/kernel-page-tables.html), and [process address locking/teardown](https://docs.kernel.org/mm/process_addrs.html): mapping structures, permission updates and teardown synchronization. Linux's mechanisms are comparative evidence, not drop-in solutions for the Embers BSP microkernel.

## Review/acceptance gates

- [ ] Exact-head standalone formatting/Clippy/host tests plus required normal boot, exception, and both QEMU CPU profiles pass.
- [ ] Independently inspect **no runtime use**, code duplication, numeric overflow/overlap logic, missing stage gates and the limits of quarantine within the `advance()` API.
- [ ] Confirm bootloader storage cannot accidentally be classified as free or reused; current host model trusts its supplied Limine kind and exclusions, **so no security claim is allowed**.
- [ ] Recheck external protocol details and document any mismatch between real hardware evidence and the model's hypothetical inputs.
- [ ] [Issue #84](https://github.com/bit-foundry-labs/embers-kernel/issues/84) is closed after the #81/#83 read-only review; the active pre-write ownership, mapping and lifetime gate remains [issue #110](https://github.com/bit-foundry-labs/embers-kernel/issues/110). Neither that review nor this model authorizes privileged memory changes.
- [ ] No live memory writes, stack switch or CR3 change until independently authorized in a **different** PR.
