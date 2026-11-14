# M06f: fail-closed CR3 transition gate research harness

**Status:** Host-only research harness. There is no production CR3 writer,
page-table builder, or stack/mapping evidence API.

## Production seam and draft reconciliation

The current boot orchestration is `kernel_entry` in `src/main.rs`. It calls
`paging_mode::check_active()` and later `check_boot_memory()`, where provenance
and frame reservations are checked. These checks do not meet at a transition
writer: no production transition function or CR3 write exists yet. The future
writer and all preflight/construction failure branches must meet in one
single-BSP function called from `kernel_entry`; this is a design constraint,
not an implementation in this change. The new gate is therefore isolated at
[`research/cr3_transition_gate.rs`](research/cr3_transition_gate.rs); it is not
connected to the kernel boot path and must not be described as a hardware
transition guarantee.

Open drafts #55 and #56 add host-only page-entry/table/walk models, while #59
explores carrying frame bookkeeping through kernel state. This change does not
depend on or copy those drafts: the gate builds no page tables and treats
construction and stack/mapping proof as blocked until authoritative APIs exist.
The existing range registry is used only to revalidate reservation values;
this is not proof that arbitrary caller-supplied records represent unique
physical ownership.

## Invariant and failure behavior

The harness validates the existing CR0/CR4 checks, revalidates provenance
metadata, validates reservation ranges and overlap through `FrameRegistry`, and
checks the HHDM candidate for the last frame in each reservation. Error order
is fixed: paging mode, provenance, reservation set, HHDM candidate, table
construction, then stack/mapping evidence.

The table-construction and stack/mapping checks are not implemented. The
harness always emits `TableConstructionIncomplete` and
`StackMappingEvidenceMissing`, so it cannot reach its mock callback. Each
rejection test verifies zero mock calls and that its simulated old root remains
unchanged. A positive mock-only path is intentionally omitted: enabling one
would require inventing proof for the missing builder and mappings.

The mock callback is ordinary host Rust control flow. It neither executes
privileged assembly nor proves that future assembly will obey the same gate.
Any future CR3 implementation must independently connect all validation and
construction failures to a no-write branch, then be verified in gated QEMU
before making a hardware-level claim.

## Files and validation

- `research/cr3_transition_gate.rs`: research-only preflight/error model and
  mock callback ordering.
- `tests/cr3_transition_gate_host.rs`: negative-path and deterministic-order
  host tests.
- `Makefile` and `.github/workflows/rust-boot.yml`: expose and run the host test
  with the existing boot workflow.
- `src/main.rs` and the real CR3 state are unchanged.

Run `make rust-transition-gate-test` for the focused host test. The existing
`make rust-paging-mode-test`, Rust boot, M03 exception, and M04/M05 workflows
remain regression gates. These tests do not demonstrate a real CR3 transition;
there is no transition feature for QEMU to exercise yet.

Recorded local results: `make rust-transition-gate-test` passed 42 tests;
`make rust-paging-mode-test` passed 5; `make rust-provenance-test` passed 21;
`make rust-frame-registry-test` passed 24; `make rust-check` passed the release
build and ELF-layout check; and release Clippy passed with warnings denied.
`cargo fmt --all -- --check` and the explicit research-module rustfmt check are
also required. QEMU and ISO tools are unavailable in the local environment, so
these results are not a substitute for the existing QEMU CI on the exact head.
