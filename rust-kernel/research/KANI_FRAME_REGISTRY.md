# Kani pilot: frame-registry rejection and overlap invariants

**Status (2026-11-20):** Experimental, proposed verification harnesses. A separate, read-only [Kani CI workflow](../../.github/workflows/kani-frame-registry.yml) now attempts the actual bounded verification using pinned Kani 0.68.0. **No proof result is claimed until its exact-head run completes and the assertion, coverage, and unwinding reports are inspected.** Ordinary Rust compile/format CI still checks source integration only.

This pilot checks the **actual** `FrameRegistry::reserve` implementation in [`src/frame_registry.rs`](../src/frame_registry.rs), which is used by the normal `TransactionalFrameAllocator::reserve` path. It is intentionally the first, smaller proof obligation rather than a claim about the entire transactional allocator.

## Why this source and not a parallel model?

The standalone [`kani_frame_registry.rs`](kani_frame_registry.rs) includes the production memory/registry modules through Rust `#[path]` module imports. It invokes the same `reserve`, `count`, and `entry` code that host tests use, not a copied implementation. No Kani dependency is introduced into `Cargo.toml`, the boot binary, or the CI release path.

Under normal Rust compilation, the `#[cfg(kani)]` harnesses are excluded. The source can still be syntax/type-checked without Kani. When invoked by Kani, the verifier supplies `cfg(kani)` and the proof API.

## Properties to verify

| Harness | Preconditions | Assertion to establish |
| --- | --- | --- |
| `rejected_request_preserves_every_record` | Two-entry registry already contains one valid two-page reservation; bounded symbolic second request | Every rejected request preserves `count` and both exposed slots exactly; covers success and rejection to catch vacuous checking |
| `accepted_request_is_recorded_and_disjoint` | Same initial reservation and symbolic second request | Every accepted request becomes the second record, preserves the first, and starts at or beyond the initial two-page range |
| `capacity_exhaustion_never_changes_state` | One-entry registry already full; symbolic second request | Every attempt is rejected and preserves the original record; explicitly check that capacity exhaustion is reachable |

The symbolic request comprises 6 possible page slots, 5 page-count values (including zero), 3 numeric owner IDs (including invalid zero), and an optional byte misalignment. **This is a finite, bounded domain**; it is not a proof for arbitrary 64-bit addresses, overflow extremes, every number of registry entries, or externally corrupted registry internals.

**Important:** `kani::cover!` reachability is not itself a security proof. A reviewer must inspect the coverage results: successful accepted/rejected paths and capacity exhaustion must all be reachable, with zero unwinding failures or undetermined checks.

## Run and record the pilot

Use an isolated Linux x86-64 environment. According to the [official Kani installer](https://model-checking.github.io/kani/install-guide.html), a Rust toolchain managed with rustup is required. Pin the verifier version for reproducibility; `0.68.0` is a candidate version as of this review, not a claim that we installed it:

```bash
cargo install --locked kani-verifier --version 0.68.0
cargo kani setup
kani --version
kani rust-kernel/research/kani_frame_registry.rs
```

Run from the **repository root**. The [standalone-file usage](https://model-checking.github.io/kani/usage.html) is intentional. If the tool fails to compile, requires a different option, exhausts resources, or reports an unsupported operation, record the exact diagnostic instead of claiming verification or weakening an invariant.

For a non-verifying source smoke check with Rust 1.90.0:

```bash
rustfmt --edition=2021 --check rust-kernel/research/kani_frame_registry.rs
rustc --edition=2021 rust-kernel/research/kani_frame_registry.rs -o /tmp/embers-kani-frame-registry-compile
```

Do **not** execute the resulting standalone binary as evidence of proof: it contains no running verification logic under ordinary `rustc` compilation.

Reviewer record must include: exact Git SHA, Kani version, target/toolchain, each harness status, `cover` status, unwinding completeness, solver/resources if relevant, and any counterexample. As a sanity check, a reviewer may temporarily invert one property **in a disposable local copy** and confirm that Kani finds a counterexample; do not commit a broken assertion to the main proof file.

## What this does not establish

- This does **not** prove `TransactionalFrameAllocator::reserve` is atomic across pool cursor advancement and registry insertion. That larger property needs a separate executable proof using the actual transaction and a construction path that does not fabricate authority.
- It does **not** prove globally unique physical memory, authenticated owner IDs, current page-table mappings, pointer provenance, absence of aliases, safe DMA/IOMMU access, reclaim-after-revocation, or a safe CR3 transition.
- Rust and Kani verification is not a substitute for a reviewed hardware and bootloader threat model or independent exact-head QEMU tests.

**Next bounded decision:** Only after confirming the harnesses compile and produce meaningful successful coverage should we consider extending the proof to `TransactionalFrameAllocator`, with a documented source-construction contract and failure-state snapshot. Preserve PR #104's Limine-map fix as a separate decision and do not merge either PR as part of this pilot.

Sources: [Kani usage](https://model-checking.github.io/kani/usage.html), [Kani coverage semantics](https://model-checking.github.io/kani/verification-results.html), [Kani unwinding requirements](https://model-checking.github.io/kani/reference/attributes.html), [Embers R1 architecture research draft PR #103](https://github.com/bit-foundry-labs/embers-kernel/pull/103), and [Limine-map compatibility draft PR #104](https://github.com/bit-foundry-labs/embers-kernel/pull/104).
