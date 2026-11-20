# M01: Split-phase revocation, finite two-core model

**Research experiment, not a kernel or hardware proof.** This models two independent core actors, a finite generation-tagged handle, two held operations, and one simulated accelerator with an optional permanent hang.

## What each file does

- **RevocationBuggy.tla** intentionally permits Core A to commit a cached authorization after Core B revokes it. TLC should produce a counterexample to NoLateCommit.
- **RevocationSafe.tla** requires admission to acquire a tracked hold atomically, closes admission on revoke, and does not permit physical reclamation until CPU holds drain and a simulated device is quiesced.
- The Safe model also explores a stale handle after one generation increment, hung device quarantine, and unrelated work that remains enabled.
- **check_reference.py** is a standard-library-only exhaustive explorer of a companion Python state machine. It does not parse or execute the TLA+ model.

## Run Python companion without downloading anything

    python3 research/formal/revocation/check_reference.py

Expected results: the unsafe trace should include Core A validate, Core B revoke, then Core A commit cached authorization; the corrected Python companion should explore **630** states without violating its assertions. This number applies to the Python companion, not to the TLA+ TLC search.

## Run actual TLA+ TLC model checker

Java 11+, curl and sha256sum required. TLC release **v1.7.4** is pinned; the script verifies the downloaded JAR's SHA-256 before execution.

    cd research/formal/revocation
    TLA_DOWNLOAD=1 bash check_tlc.sh

If a checked JAR already exists:

    TLA_TOOLS_JAR=/absolute/path/to/tla2tools.jar bash check_tlc.sh

The script expects RevocationSafe to pass and RevocationBuggy to fail *specifically* on NoLateCommit. It then creates three temporary **mutations of the actual safe TLA+ module**, rechecks each with the same SHA-256-verified TLC JAR, and requires the indicated invariant to fail:

| Deliberately removed guard | Required TLC failure |
| --- | --- |
| Admission phase must still be Active | NoPostRevokeAdmission |
| Handle generation must match the live slot | NoStaleHandleAdmission |
| Device must be Idle before quiescence | NoEarlyReclaim |

`check_mutations.py` matches exact action-scoped source snippets once; if the model changes, mutation generation fails instead of silently checking the wrong line. The run writes individual TLC counterexample logs to `.check-logs/mutant-*.log` and fails if a mutant passes, times out, or produces a different error. Run `python3 research/formal/revocation/check_mutations.py --self-test` to validate the mutation anchors **without** claiming to have run TLC.

These tests show the bounded model checker remains sensitive to three particular missing guards; they do **not** prove the safe model is complete, establish a runtime Rust revocation implementation, or authenticate actual accelerator quiescence. A successful Python companion run **does not** establish that TLC parsed or checked the TLA+ files.

## Invariants modeled

- No untracked commit after a revocation; previously admitted tracked work may finish.
- No *new* admission after logical closure; no admission using stale slot generation after reuse.
- No physical reclamation while CPU holds or simulated device access remains outstanding.
- Quarantined memory is never reassigned unless the device is quiesced.
- Unrelated progress actions remain available without implying zero latency or any liveness theorem.

## Security scope / assumptions

Trusted caller identity, protected capability storage and atomic kernel actions are assumed rather than implemented. There is no real derivation tree, IPC, userspace isolation, TLB/IOMMU/PASID behavior, DMA fence, or trusted hardware reset. The explicit ResetDevice transition models a reset *only if* it can prove safe quiescence on a selected device; without it, hanging work may remain quarantined forever. The model does not assert unconditional liveness or production fairness.

**Next work:** connect Rust CapTable authorization, held admissions, logical revocation tickets, and physical memory-grant quiescence; test revocation racing with cached ValidatedPolicy from PR #11. Do not merge security-critical kernel changes based only on abstract model checks.

Design [ADR-0004](https://github.com/bit-foundry-labs/emberglass-kernel/blob/docs/adr-0001-ai-progressive-microkernel/docs/adr/0004-split-phase-capability-revocation.md), tracked by [Issue #14](https://github.com/bit-foundry-labs/emberglass-kernel/issues/14).

Primary tools reference: https://github.com/tlaplus/tlaplus/blob/master/USE.md
