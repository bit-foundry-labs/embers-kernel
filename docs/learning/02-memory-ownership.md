# 02 — Who owns a physical frame?

**Question:** How can a kernel reject conflicting ownership claims, and why is recording ownership not the same as enforcing memory isolation?

**Prerequisites:** Basic Rust data types and unit-test literacy. **Mode:** source inspection, with optional host tests.

## Observe

Read [M04 physical memory](../../rust-kernel/M04_PHYSICAL_MEMORY.md), [M05 frame ledger](../../rust-kernel/M05_FRAME_LEDGER.md), and the current [Rust kernel notes](../../rust-kernel/README.md). Find the frame ledger implementation and its tests on `main`. Verify names against the current source; do not rely on an older document's API names.

## Explain

Draw or write a state trace for a single frame:

1. Initially available for accounting.
2. Owner A makes a valid claim.
3. Owner B attempts a conflicting claim.
4. A frame enters a quarantined state, if that transition is supported by the inspected implementation.
5. A later claim attempts to reuse the quarantined frame.

For each step, cite the relevant function or test and state the **observed** accept/reject outcome. If a step is not implemented or not tested, label it **unverified**, rather than filling it in from the intended design.

## Test or inspect

On a supported Ubuntu/WSL setup, follow the current [host-test commands](../../rust-kernel/README.md). For example, `make rust-ledger-test` is documented in the repository; confirm it still exists before running. Do not edit kernel code for this exercise. If working from a phone, inspect existing tests and CI results, but do not claim to have executed them.

## Critique

- What stops two *symbolic owner IDs* from claiming the same frame in this model?
- What hardware mechanism would be needed to prevent a real user process from accessing another process's memory?
- Which test would best challenge an ownership invariant at a boundary?

## Contribute

Propose **one** narrowly scoped negative-test case or documentation clarification with a precise expected outcome and source links. Check for duplicates and discuss it on an existing issue first. Do not alter allocator, page tables, or ownership semantics as a beginner exercise.

**Completion evidence:** a five-step trace, source/test links, a distinction between accounting and hardware protection, and one bounded test proposal.
