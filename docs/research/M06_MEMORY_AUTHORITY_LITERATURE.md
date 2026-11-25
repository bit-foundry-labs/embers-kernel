# Research note: evidence for M06 memory authority

Status: research guidance, not an implemented guarantee or a decision to adopt a
verification tool. Baseline: main at `992fc197776076cf428f0a2687050e10e4b15e84`.
The [M06 transition proposal](PAGING_TRANSITION_V1.md) remains authoritative for
the proposed first transition; [issue #110](https://github.com/bit-foundry-labs/embers-kernel/issues/110)
tracks the integration review. Drafts [#109](https://github.com/bit-foundry-labs/embers-kernel/pull/109),
[#106](https://github.com/bit-foundry-labs/embers-kernel/pull/106), and
[#105](https://github.com/bit-foundry-labs/embers-kernel/pull/105) are research
evidence, not merged implementation.

## Why these papers now?

Embers boots in QEMU and has physical-frame bookkeeping, but does not own its
page tables or provide hardware-enforced process isolation. A numeric frame
reservation or successful Rust test cannot by itself establish an accessible,
exclusive, initialized page-table frame. The papers below help separate those
claims and identify what future M06 evidence should cover.

## Research and application

### Rust page tables need semantic invariants

Dai et al., “Verifying Rust Implementation of Page Tables in a Software Enclave
Hypervisor,” ASPLOS 2024,
[DOI: 10.1145/3620665.3640398](https://doi.org/10.1145/3620665.3640398),
[author-hosted paper](https://www.cs.columbia.edu/~rgu/publications/asplos24-dai.pdf).

The authors verify spatial isolation for an early HyperEnclave prototype, using a
model of Rust MIR and layered proofs for its memory subsystem. Their central
observation is that memory-safe code can still configure page tables incorrectly.
Their target is a hypervisor using extended page tables, not the Embers boot path;
its proof and hardware assumptions do not transfer automatically.

**M06 implication:** specify relationships among a reserved frame, the actual
writable virtual alias used to initialize it, the page-table entries written, and
the translations available after switching. For each relationship, name the
evidence source and unsupported assumptions. A correct frame number is only one
piece of that chain.

### A verified Rust microkernel still has a trust boundary

Chen et al., “Atmosphere: Practical Verified Kernels with Rust and Verus,”
SOSP 2025,
[DOI: 10.1145/3731569.3764821](https://doi.org/10.1145/3731569.3764821),
[paper](https://mars-research.github.io/doc/2025-sosp-atmo.pdf),
[project](https://mars-research.github.io/projects/atmo/).

Atmosphere uses Rust and Verus to establish functional correctness against
specifications, including page-table-related reasoning. Its work illustrates
that proofs depend on stated specifications and trusted tools. The authors
identify compiler, solver, specifications and hardware among their trust
assumptions; their noninterference discussion does not cover all user-code
effects or shared-hardware timing channels. Its big-lock design also permits
long operations to block others.

**M06 implication:** maintain a claim ledger: state each property, assumptions,
the artifact checked, and the evidence that connects it to actual kernel code
and hardware. Treat a Kani harness or host model as evidence for its modeled
scope only. Consider Verus later only after defining a small property and
assessing toolchain and maintenance costs; this paper does not establish that
Embers needs it.

### A safe Rust API needs a sound implementation

Peng et al., “ASTERINAS: A Linux ABI-Compatible, Rust-Based Framekernel OS with
a Small and Sound TCB,” USENIX ATC 2025,
[paper and abstract](https://www.usenix.org/conference/atc25/presentation/peng-yuke).

ASTERINAS separates its hardware-oriented framework from services written in
safe Rust, aiming to narrow the trusted base for memory safety. It is a
single-address-space framekernel, not the hardware-isolated microkernel Embers
intends to explore. Its architecture and measurements are not Embers results.

**M06 implication:** for each eventual safe API around physical memory or page
tables, review who can construct its authority, what aliases may exist, how
long backing remains valid, and whether every safe caller preserves the
underlying unsafe operation's preconditions. A `safe` signature alone does not
establish physical ownership.

## Proposed review exercise

Before any privileged page-table write, reviewers can use one candidate table
frame to trace the following evidence chain:

1. **Selection:** identify the boot-map entry and excluded ranges; show checked
   alignment and bounds and explain why the frame is eligible.
2. **Authority:** identify the persistent issuer and reservation, including
   competing pools, aliases, and the actual lifetime; explain which part is
   enforced versus assumed.
3. **Access:** establish the writable virtual mapping and its provenance under
   inherited tables; a computed HHDM candidate is not a mapping proof.
4. **Initialization and publication:** identify how zeroing, legal entries and
   permissions are checked before the frame can become reachable from a root.
5. **Transition and retention:** account for execution, stack, descriptors,
   exception paths, translation-cache rules and non-reuse. Tie each requirement
   to the M06 proposal and applicable architecture documentation.

This is a review checklist, not a finding that any of these properties are
already established. Deliberate failure tests should establish that rejected
preconditions cannot reach a privileged write; actual QEMU transition and fault
evidence remains distinct from host-level model checking. SMP shootdowns,
device DMA, and reclamation require their own later arguments.

## Decision record

Use these sources to sharpen M06 review and future tests. Do not add a new proof
tool, change the paging design, write CR3, claim device safety, or treat a
different project's proof as an Embers proof on the basis of this note.
