# Research note: evidence for M06 memory authority

Status: research guidance, not an implemented guarantee or a decision to adopt a
verification tool. Baseline: main at `992fc197776076cf428f0a2687050e10e4b15e84`.
The [M06 transition proposal](../PAGING_TRANSITION_V1.md) remains authoritative for
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

## Worked trace against main

The following is a source review of the first boot diagnostic reservation at
`992fc197776076cf428f0a2687050e10e4b15e84`, not an observed physical
address or a claim that this reservation is a page-table frame.

- **Selection:** [boot_memory.rs](../../rust-kernel/src/boot_memory.rs)
  copies up to 128 Limine descriptors while trusting their mapped pointers.
  [memory.rs](../../rust-kernel/src/memory.rs) rejects zero-length or
  overflowing ranges and overlaps involving usable or reclaimable entries,
  selects the lowest sufficiently large usable span at or above 1 MiB, and
  aligns inward to 4 KiB. This relies on truthful bootloader metadata.
- **Reservation:** the one-shot permit restricts production pool issuance
  through this module; [frame_registry.rs](../../rust-kernel/src/frame_registry.rs)
  reserves two numeric pages in the initial record and rejects conflicting
  records within its registry. [kernel_state.rs](../../rust-kernel/src/kernel_state.rs)
  retains that allocator and map. Owner labels are not authenticated and the
  record does not exclude existing hardware aliases or establish global
  physical ownership. Test-only selectors can construct overlapping pools.
- **Access:** [main.rs](../../rust-kernel/src/main.rs) calculates and checks
  HHDM candidates for the two frame numbers. It does not establish that those
  candidates provide an exclusive writable mapping or dereference the frames.
  Inherited page-table frames and other live hardware uses need a separately
  justified exclusion; draft [#108](https://github.com/bit-foundry-labs/embers-kernel/pull/108)
  is related research, not merged proof of complete exclusion.
- **Initialization and publication:** this diagnostic reservation does not
  zero the actual frames, populate a hierarchy, or publish a new root.
- **Transition and retention:** no new CR3 is installed by this path. The
  registry has no release operation, but non-reuse within this bookkeeping
  system is not a proof of global quiescence or device safety.

**Next bounded engineering question:** how can a future M06 frame preparation
API establish a writable inherited mapping and exclude conflicting active
uses before zeroing a reserved frame? Specify its assumptions and negative
paths first, then check it against #108, #109, and the #110 integration queue.
Avoid creating a parallel model before examining those drafts.

This trace is a source inspection only; existing test names indicate intended
coverage, but no tests or QEMU run were performed for this note.

## Comparison with existing drafts

Checked against #108 head `5bcc3930cea29b5fc4f519226dac7950a94dc6f7`,
#109 head `1c61e25e06da015fb0f69cd6e4d5e134ace72db2`, and the current
checkpoint in #110. These are draft-source observations, not new test results.

| Question | Existing evidence | Still needed before a privileged write |
| --- | --- | --- |
| Does a candidate translate to its expected physical page under inherited mappings? | Merged #83 and draft #108 inspect first and last bytes of two candidate frames in paused QEMU; #108 also checks for collisions with table pages visited along those four walks. | Evidence is specific to sampled paths and one snapshot. It does not establish complete alias exclusion, lasting authority, Rust pointer validity, or device isolation. |
| Is a candidate authorized for preparation? | #109 models a transition requiring a sole-issuer assertion, writable backing, exclusive access, and completed zeroing. | Those inputs are caller-supplied booleans; the records are copyable and forgeable. A real design must bind each obligation to actual enforced ownership and valid mapping evidence, including live-use exclusions. |
| Can preparation start? | #110 section D lists the integration blockers. | The blockers remain open; do not zero a frame, write page tables, or switch CR3. |

**Review question for #110:** What bounded, read-only observation could most
reduce uncertainty about conflicting live use beyond #108's sampled paths,
while clearly documenting coverage and unobserved uses? Any candidate needs
its own review and does not by itself establish exclusive write authority.
Review #108 and #109 before proposing additional overlapping implementation.

## Decision record

Use these sources to sharpen M06 review and future tests. Do not add a new proof
tool, change the paging design, write CR3, claim device safety, or treat a
different project's proof as an Embers proof on the basis of this note.
