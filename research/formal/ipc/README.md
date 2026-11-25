# IPC finite-state reference experiment

This is a deliberately smaller executable slice of the [IPC design candidate](../../../docs/IPC_V1_STATE_MACHINE.md), not a full implementation or proof.

Run from repository root with Python 3.8+ and no third-party packages:

    python3 research/formal/ipc/check_reference.py
    python3 -m py_compile research/formal/ipc/check_reference.py

## Domain and oracle

One endpoint, capacity two, two senders and two receivers. Each sender may commit at most one distinct symbolic message, each receiver may finish at most one admitted attempt. Rights can be revoked independently, staging can abort, a reserved head can copy successfully or fail, and closure/retirement can interleave with held operations. Abstract copy completion and hold release are atomic. The breadth-first search enumerates reachable abstract states and checks every generated edge, including edges to previously seen states. The transition oracle does not consult the mutation selector.

Observed local output (Python execution, not GitHub CI):

| Variant | States discovered | Edges checked | Result |
| --- | ---: | ---: | --- |
| Safe | 2608 | 9744 | PASS, search exhausted |
| Send revalidation removed | 58 | 101 | late_send |
| Receive admission check removed | 111 | 224 | late_receive |
| Exclusive reservation removed | 152 | 316 | exclusive_head |
| Retirement hold check removed | 76 | 140 | early_retire |

Mutants stop at the first violation; counts are not their total reachable states. Example traces are printed by the script. A mutant must fail with its expected invariant label, not an arbitrary failure.

## Limits and next steps

This slice does not represent bytes, actual copy faults or partial disclosure, buffer permissions, authenticated callers, generations/reuse, delegation, explicit budget balances, error precedence, multiple endpoints, starvation, or hardware. A rejected commit is disabled in the abstract transition system; explicit typed error and cleanup behavior is not modeled (abort is separate). Copy failure retains the queue but this does not establish actual confidentiality. The model cannot establish bounded real execution, complete linearizability, kernel memory safety, or secure IPC. A finite model can have omitted transitions and weak assertions; the negative controls demonstrate sensitivity only to four selected faults.

Next: add an explicit admission/commit rejection and cleanup transition, and a generation-tagged reuse model with stale-handle denial. Later model disclosure and mapping lifetimes separately before attempting real user copies. No changes to the M06 pre-write gate.

## Rust reference implementation (primary implementation research)

The dependency-free [Rust integration test](../../../experiments/rust-policy-guard/tests/ipc_research.rs) uses `no_std`, forbids unsafe code, and models a single endpoint with two actors, two fixed queue slots, four-byte payload capacity, bounded staging and one exclusive receive reservation. Generation-tagged handles and distinct send/receive operation tickets reject stale operations; checked counters refuse wraparound. A rejected valid-ticket commit releases exactly its staging hold. This is a serialized host-test model, not production IPC or a protected capability table.

Run the focused suite from the repository root:

    cargo test --manifest-path experiments/rust-policy-guard/Cargo.toml --test ipc_research

The existing policy-guard workflow also runs cargo fmt, cargo test and Clippy. Nine authored tests cover ownership/FIFO, revoked and full-queue commit cleanup, tracked receive completion, copy-failure retention, zero/oversized/short messages, closure/reuse, generation/serial exhaustion, and stale/foreign operation tickets. Rust toolchain was unavailable locally; compilation, formatting, tests and lints remain unverified until exact-head CI is inspected.

The Python state explorer above remains an independent abstract reference; its counts are not Rust test results. No differential test has been implemented. The Rust model assumes authenticated actors, trusted mint/revoke/lifecycle operations, atomic serialized transitions and simulated copy completion. It does not model actual mapping stability, partial byte disclosure, cancellation, delegation, multi-endpoint budgets, physical cleanup or concurrent synchronization. No kernel changes, dependencies or new workflows.
