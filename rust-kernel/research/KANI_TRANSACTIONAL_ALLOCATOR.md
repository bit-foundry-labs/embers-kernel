# Kani research: transactional frame-allocation contracts

**Status (2026-11-20):** Bounded Kani assertions and coverage **passed for the first source-complete checkpoint** `6f0f12af1c7df271aed2bef0873d6266bd205a3a` in [Kani CI run 44820387890](https://github.com/bit-foundry-labs/embers-kernel/actions/runs/44820387890). Independent review and exact-head CI after subsequent documentation changes remain outstanding. This is a stacked follow-up to [draft PR #105](https://github.com/bit-foundry-labs/embers-kernel/pull/105).

## What this verifies

[`kani_transactional_allocator.rs`](kani_transactional_allocator.rs) imports the exact production [`memory.rs`](../src/memory.rs) and [`frame_registry.rs`](../src/frame_registry.rs) modules. It invokes the production `claim_boot_pool_permit`, `select_boot_pool`, `TransactionalFrameAllocator::with_boot_reservation`, and `TransactionalFrameAllocator::reserve` paths. **No alternate allocator implementation, fake token constructor, or changes to privileged runtime code are used.**

The harness assumes a single, aligned `USABLE` physical span starting at `MIN_PHYSICAL_ADDRESS`, containing **eight pages**; this is a *synthetic trusted input*, not proof that any real Limine span, HHDM mapping, or page-frame ownership is valid. Initial boot bookkeeping claims two pages as owner 1; six remain. Each proof harness begins independently with a fresh one-shot permit. The relevant registry sizes are one or two entries.

| Harness | Symbolic inputs | Claims checked |
| --- | --- | --- |
| `rejection_preserves_pool_and_every_exposed_slot` | Page count `u8` in `0..=10`; owner `u8` in `0..=2` | Any rejected transaction preserves remaining pages and every exposed reservation slot; separately cover success, zero pages, insufficient frames, and invalid owner |
| `accepted_reservation_consumes_exact_number_of_frames` | `1..=6` pages; owner `1..=2` | Exactly the requested pages are committed, the old boot reservation is unchanged, the new record is contiguous, in-span and matches the caller's request |
| `full_registry_rejects_after_pool_preflight_without_consumption` | `1..=6` pages with valid owner 2; registry has one occupied slot | A request that passes pool preflight but fails registry capacity checks does not consume frames or change registry slots |
| `full_width_page_count_cannot_consume_frames_on_rejection` | Arbitrary `u64` page count; owner 1 | Rejecting overflow, shortage or zero-length counts leaves remaining pages and both records unchanged; cover arithmetic overflow and acceptance |

The observed state is `remaining_pages()` and every `reservation(index)` slot in the registry. A snapshot of `remaining_pages()` is equivalent to the monotonic pool cursor **under the assumed fixed pool end**, but the harness does not inspect private byte-level representations or every possible allocator state.

This research concerns single-threaded atomicity of validation and commit. It does **not** show concurrency linearizability, arbitrary constructor authority, starvation freedom, safe reclamation, DMA/device quiescence, the absence of duplicate physical mappings, or hardware-enforced isolation.

## Verified bounded result and remaining caveat

The cited source checkpoint ran **Kani 0.68.0 and CBMC 6.11.0**. All four transactional harnesses finished `VERIFICATION:- SUCCESSFUL` and reported **zero failed checks**. Coverage was satisfied: **4/4** in rejection-state preservation, **1/1** in accepted full-capacity boundary, **1/1** in registry-capacity rejection, and **2/2** in full-width arithmetic. Each report also included one `UNREACHABLE` check, not a failed assertion. An independent reviewer should still inspect the full log and unreachable-check context before accepting a stronger assurance claim.

**Important sequential-only limitation:** Kani warned that `atomic_cxchg` operations (used by the boot-pool permit construction) are modeled **sequentially**; Kani 0.68.0 does not establish concurrent atomic linearizability here. The proof therefore assumes a single bootstrap thread, consistent with this research milestone. The one-shot boot-permit's real SMP behavior, hardware frame authority and unsafe pointer provenance remain outside the proof.

The initial attempt on this source checkpoint failed `rustfmt --check` because an assertion needed multiline formatting. That formatting was fixed **without changing the allocator** before the successful Kani re-run linked above. CI must be inspected on the latest PR head separately.

## Required execution and reviewer evidence

The Kani workflow inherited from draft PR #105 is extended in this stacked change. It installs **Kani 0.68.0** in a read-only, ephemeral GitHub Actions runner, runs the existing registry proofs and then executes:

```bash
kani rust-kernel/research/kani_transactional_allocator.rs
```

The ordinary memory CI also checks this standalone file with:

```bash
rustfmt --edition=2021 --check rust-kernel/research/kani_transactional_allocator.rs
rustc --edition=2021 rust-kernel/research/kani_transactional_allocator.rs -o /tmp/embers-transactional-harness-check
```

**Ordinary Rust compilation does not execute the Kani harnesses**, because `#[cfg(kani)]` excludes them outside the verifier. A successful build, a green QEMU run or registry-only Kani proof is not evidence that these new assertions passed.

Before recording a proof result, inspect the exact-head Kani output for each named harness, assert/coverage status, unsupported or unwound reachable constructs, resource/time limits, Kani+CBMC versions, and warnings. A `cover!` target must be *satisfied*, not merely present in source. If an assertion fails, capture a reproducible counterexample and classify it before modifying the invariant or implementation. As a negative control, a reviewer can intentionally weaken a checked invariant **in a disposable local copy** to confirm counterexample sensitivity, without committing broken test code.

If the one-shot boot-permit path is unsupported by Kani, report that as a tool-compatibility limitation; do **not** use a fake boot permit or silently replace the production allocator with a model merely to obtain green results.

## Research gates and follow-on decisions

1. Only claim bounded allocator properties if all four exact-head harnesses pass with their intended coverage.
2. Check the source import paths and ensure any future `memory.rs` and `frame_registry.rs` updates trigger the Kani workflow. This is essential: a successful proof on stale code does not cover a later implementation.
3. Keep this PR stacked on draft #105; do not merge or flatten until the prerequisite is reviewed.
4. Then consider whether the next most valuable experiment is the M06 page-table frame *lifetime* boundary, not a third symbolic reservation wrapper.

Further sources: [official Kani documentation](https://model-checking.github.io/kani/), [Embers architecture-security draft #103](https://github.com/bit-foundry-labs/embers-kernel/pull/103), [M06 physical frame inventory](../../rust-kernel/M06E_TRANSACTIONAL_RESERVATIONS.md), [M06 paging prerequisites](../../docs/PAGING_TRANSITION_V1.md).
