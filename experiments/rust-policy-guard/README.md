# Emberglass Rust Policy Guard (experimental)

A small, dependency-free, no_std-compatible Rust reference model for AI policy validation and simplified CPU minimum-reservation admission.

It is **not** a scheduler, kernel, protected capability table, hardware driver or mathematical proof of fairness. The constructor for Authority is deliberately public for host simulation; in a real kernel, authority must come from protected caller-local capability handles. No CPU reservations are delivered or timed here.

Test: `cargo test` in this directory.

## Invariants exercised
- No advisory policy for another tenant
- Reject excessive weight and memory requests instead of silently clamping
- Epoch expiry and replay rejection
- Model absence produces baseline fallback, independent of model inference
- Simplified sum of CPU minimum requests cannot exceed an epoch's available capacity
- A rejected proposal leaves the gate's replay state unchanged, so the next valid proposal is still accepted
- Accepted advice is exactly the proposed tenant, weight and pages, never a widened or clamped value
- Multi-tenant CPU minimums are admitted at exact capacity, rejected one tick over, and rejected rather than wrapped on `u32` overflow

The policy-gate and admission properties are also checked exhaustively over fixed boundary values: two tenants; epochs around the replay and expiry limits, including `u64::MAX`; weights and page counts at 0, at the limit, one past it and at the type maximum; and up to three tenants with small tick counts. These deterministic tests need no dependencies. They cover only the enumerated values and are not a proof over all inputs.

## Admission validation is not scheduler fairness

`PolicyGate` decides only whether a proposal falls within a simulated authority. `admit_minimums` decides only whether a set of minimum reservations fits the capacity left after a system reserve in one simplified single-CPU epoch. Neither delivers CPU time, bounds latency, prevents starvation, accounts for preemption, interrupts or context-switch overhead, or models more than one CPU. A real scheduler must enforce budgets at run time and needs its own tests or proofs of those properties; passing admission here establishes none of them.

Next: scheduler runtime fairness, capability handle generations, IPC, fault injection, and formal property harnesses.

Related: [ADR-0003](../../docs/adr/0003-ai-policy-validation-and-admission.md) in the research PR #3.

## Delegation and budget attenuation (host-test-only)

The test-only `src/delegation_research.rs` models a finite table of grants. It is
included using `#[cfg(test)]`, so it is **not part of the policy-guard library
compiled for normal use**, much less the privileged kernel. Run the existing
`cargo test` and `cargo clippy --all-targets -- -D warnings` in this directory.

In this deliberately small model, only the table issues child grants. A parent
must possess `DELEGATE`; a child's `CPU`, `MEMORY`, and `DELEGATE` rights
must be a subset of its parent's rights. CPU-tick and page-count budgets
are transferred out of the same *remaining* balance used by the parent's own
modeled charges. This prevents overallocating through multiple children or
charging the parent's full budget after delegation. Invalid child owners,
unknown rights bits, resource/right mismatches, empty grants, full tables,
and over-budget requests are rejected without changing either grant record.
Tests include an exhaustive **small** two-child budget matrix, not all u16
values or arbitrary depth.

**Important unimplemented boundaries:** Owner numbers are forged directly by
host tests, and table slots are plain identifiers, not protected handles.
There is no process-derived identity, generations, concurrent mutation,
revocation, draining, device quiescence, actual scheduler admission, physical
frame assignment, or authority-revocation proof. No budgets are refunded or
reused. The model exercises local accounting policy only. Integrating it as a
real `CapTable` would be unsafe without those mechanisms, independently
reviewed contracts, and hardware-isolation evidence.

The distinct [finite-state revocation model](../../research/formal/revocation/README.md)
explores admission/revocation order and quiescence assumptions; neither
experiment establishes kernel capability enforcement. See also [issue #14](https://github.com/bit-foundry-labs/embers-kernel/issues/14).

## Generation-tagged handles and delegation interleavings (stacked research)

The `delegation_research/interleavings.rs` tests extend the **test-only**
delegation model introduced in draft PR #112. A `Handle` contains a slot and
generation; reusing a test-fixture slot increments its generation, so the old
handle fails validation. A separate parent version increments on successful
delegation and test-fixture retirement. Two requests may observe the same
parent version, but the modeled serialized commit rechecks the current
version, owner, generation and budget **before** changing the table. A rejected
stale observation leaves all records unchanged. Exhaustive small interleavings
cover both commit orders and a fresh retry for request sizes 1..=8. Checked
counter exhaustion denies operations without wraparound or partial state
changes. Test with `cargo test` in this directory.

**Boundary conditions:** This is sequential exploration of two stale
observations, **not** threads racing on a real protected table. The modeled
commit is atomic by assumption and has no lock/linearization implementation.
Caller IDs, observations and generations are forgeable inside the host test;
handles are **not secrets or authenticated capabilities**. The test-only
retirement helper assumes all in-flight uses already ended and provides
neither logical revocation nor hardware/device quiescence. It never refunds
transferred CPU/page budgets. No CPU scheduler, process identity, generational
reuse guarantee under concurrency, DMA/IOMMU, privileged operations or kernel
capability enforcement is implemented here. Formal review must independently
examine that the future production synchronization and lifetime protocols
actually enforce these assumptions.
