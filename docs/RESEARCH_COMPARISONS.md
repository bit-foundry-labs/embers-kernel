# Research comparisons and Embers hypotheses

These comparisons inform design; they are not claims that Embers implements or
matches the referenced systems.

- [seL4 verification assumptions](https://sel4.systems/Verification/assumptions.html):
  capability authority and proof boundaries motivate explicit revocation and
  device/DMA assumptions. Embers has neither equivalent verification nor
  authenticated capability enforcement.
- [ASTERINAS, USENIX ATC 2025](https://www.usenix.org/conference/atc25/presentation/peng-yuke):
  a Rust framekernel explores a smaller memory-safety trusted computing base.
  Compare its treatment of unsafe boundaries; Embers remains a microkernel
  research project and has no comparable assurance or Linux ABI goal.
- [CONVEROS, USENIX ATC 2025](https://www.usenix.org/conference/atc25/presentation/tang):
  model checking uncovered concurrency defects in Rust OS modules. Investigate
  specification-to-code conformance for revocation before making SMP claims.
- [XSched, OSDI 2025](https://www.usenix.org/conference/osdi25/presentation/shen-weihang):
  heterogeneous accelerator scheduling exposes varied preemption capabilities.
  Treat device quiescence, cancellation, and DMA safety as distinct concerns.

Candidate hypothesis: a user-space adaptive resource policy can be constrained
by deterministic kernel admission and budgets, and a failed policy can fall
back safely. This remains untested. A future experiment needs a deterministic
baseline, failure scenarios, explicit safety invariants, reproducible workloads,
and measured latency, fairness, overhead, and recovery bounds.

## AI workload direction and boundaries

Potential first-class kernel resources include bounded memory regions, device
queues, completion events, and explicit compute budgets. Tensor shape, model
graphs, KV-cache policy, inference calls, and predictive prefetch are candidates
for user-space runtimes. Whether kernel-visible workload hints improve latency
or fairness is an experiment, not a privilege grant or an established result.

Zero-copy is hardware and topology dependent. Safe DMA requires explicit device
permissions, pinning, synchronization, IOMMU mappings where supported, and
quiescence before reuse. Rust borrowing alone cannot establish those properties.
Preemption and cancellation depend on each device; asynchronous Rust alone
does not guarantee bounded latency or zero overhead.

Confidential computing and RDMA are later hardware-specific research topics,
not universal encryption guarantees or defaults. Any trusted extension mechanism
requires a separate threat model and proof scope.

References: [XSched](https://www.usenix.org/conference/osdi25/presentation/shen-weihang),
[InfiniGen](https://www.usenix.org/conference/osdi24/presentation/lee),
[NVIDIA GPUDirect RDMA](https://docs.nvidia.com/cuda/gpudirect-rdma/).

## Desktop workload hypotheses

A future desktop workload may combine interactive rendering, audio, and
background model inference. Evaluate missed audio deadlines, frame-time
percentiles, input latency, model throughput, time to first token, and energy
against deterministic scheduling baselines. Sub-millisecond preemption, hard
real-time behavior, wattage targets, and uninterrupted accelerator execution
are not guarantees; devices and drivers have different preemption behavior.

Research read-only model-weight sharing with per-process private state, explicit
capabilities, and revocation. Shared addressability does not prove zero-copy or
prevent leakage through logs, caches, devices, or outputs. Any peer-to-peer
storage or networking path requires topology and security validation.

Semantic context indexing, model runtimes, display compositing, and audio
graphs are user-space responsibilities. A future system built on Embers may
request narrowly scoped memory, IPC, device, or timing resources; Embers does
not provide a context graph or desktop services. Emberglass OS is an independent
Linux-based project, not an existing Embers desktop implementation.

## Ownership transfer and accelerator isolation

[Xous memory lending](https://xous.dev/kernel/) describes page-aligned
send/lend/mutable-lend operations. Foundation identifies Palladium as a
modified Xous derivative in [KeyOS](https://github.com/Foundation-Devices/KeyOS).
The exact Palladium mapping transition requires code-level verification; do not
assume it has been verified here.

Candidate future invariant: before a recipient obtains exclusive mutable access,
all conflicting sender mappings and aliases must be revoked and relevant
CPU translations invalidated. Copies, other processes, shared mappings, device
DMA, in-flight commands, cache synchronization, crashes, cancellation, and
return/revocation need explicit treatment. A CPU MMU alone cannot control DMA.
No such lending protocol exists in Embers.

[coconutOS](https://github.com/coconut-os/coconutOS) reports a capability-based
GPU isolation design; its README says GPU compute currently uses QEMU emulation
and CPU simulation. Treat its claims as project-reported evidence, not proof on
physical accelerators. GPU page tables, device firmware, queue access, IOMMU
configuration, topology, cleanup, and side channels require independent review.

A learned scheduler may recommend bounded actions, but it does not acquire
authority or execute kernel commands unchecked. Admission, accounting,
revocation, and fallback remain enforced by deterministic kernel mechanisms.
Deterministic validation does not make an adaptive system's behavior
deterministic. Compare adaptive and non-adaptive policies under identical
workloads, and measure failure, fairness, and overhead as well as speed.

## Linux engineering lessons: testable implications for Embers

**Reference, not architecture:** Linux is a predominantly monolithic kernel;
Embers is an original Rust-first microkernel research effort. The sources below
offer failure-mode and development-process lessons, not Linux code to port,
evidence that Embers has implemented these mechanisms, or a reason to expand
the current milestone. Keep this work separate from Linux-based Emberglass OS.

### Page-table aliases and partial failure (M06; highest priority)

The Linux kernel page-table documentation distinguishes mapping permissions
from physical memory ownership. A physical page can have multiple virtual
aliases with different permissions. It also warns that some permission-update
APIs can fail **after modifying only a prefix** of the requested range, without
automatic rollback. Success codes can be insufficient evidence of actual
permission enforcement on unsupported architectures.

- **Embers implication:** an observed present/writable PTE is not authenticated
  frame ownership or an exhaustive absence-of-aliases result. The read-only QMP
  observations proposed in PRs #81 and #83 must retain those limitations.
- **Future contract question:** how will the M06 table builder validate every
  intended mapping, physical frame, ancestor permission and conflicting alias
  before installing a new root, and what happens if construction fails halfway?
- **Bounded follow-up (host-only first):** add fault-injection tests at each
  proposed frame reservation/table-build stage. Assert that failures never
  invoke a mock CR3 writer, never publish an incomplete root, and do not
  recycle a frame whose backing, mapping, or lifetime is uncertain. Include a
  fixture with two modeled virtual aliases to a single physical frame and
  reject it when they violate the intended exclusive-mapping contract.
- **Evidence limit:** rejecting aliases in an enumerated model does **not**
  establish that all live hardware aliases were discovered, that DMA is
  inactive, or that any privileged CR3 switch is safe.

Reference: [Linux kernel page tables: aliases, partial updates and TLB
handling](https://docs.kernel.org/mm/kernel-page-tables.html).
Related Embers work: [M06 transition contract](PAGING_TRANSITION_V1.md),
[page-table ownership](../rust-kernel/PAGE_TABLE_OWNERSHIP.md),
[read-only integration review #84](https://github.com/bit-foundry-labs/embers-kernel/issues/84),
and [fail-closed transition research #65](https://github.com/bit-foundry-labs/embers-kernel/issues/65).

### The missing authority chain: address, reservation, mapping, lifetime

**What existing Embers code proves:** \`boot_memory::initial_frame_pool\`
uses Limine's reported map and a one-use safe-Rust permit;
\`TransactionalFrameAllocator\` keeps a monotonic pool alongside a bounded
registry; \`KernelState\` retains the allocator through the BSP handoff.
These are meaningful software controls. However, the permitted source is still
trusted bootloader metadata, numeric owner IDs are not authenticated, and
\`KernelState\` host tests explicitly demonstrate that independently
constructed synthetic pools can issue the same physical frame. The optional
ownership smoke path and direct raw-pool/registry APIs are still identified as
bypasses in \`M06E_TRANSACTIONAL_RESERVATIONS.md\`.

**Linux comparison:** kernel page-table documentation notes that one physical
page can have multiple virtual mappings and that failed mapping-permission
updates may leave changes partly applied. A reservation record alone cannot
establish current alias coverage, CPU permissions, or safe reuse.

**seL4 comparison:** its untyped-memory capability and retype mechanism
connects an authority-bearing memory region to descendant objects. This is
a useful **design contrast**, not a proposal to copy seL4's API or claim its
formal assurance. Embers currently has no equivalent authenticated retype
path.

For the proposed first Embers-owned page-table frame, require distinct evidence
for each link of the chain:

| Step | Existing evidence | Still needed before real table writes |
| --- | --- | --- |
| **Physical source** | Limine memory map is bounded/validated for numeric consistency; only selected usable regions feed the initial pool. | Review the exact base-revision-6 map and relevant reserved, kernel, and bootloader lifetimes; keep the bootloader as a stated trust assumption. |
| **Single issuer** | One-shot production permit, move-owned allocator and per-registry overlap rejection. | Audit every live allocation route and its ownership lifetime; close raw-pool bypasses. A second independently constructed issuer must not claim the same frame. |
| **Exclusive reservation** | A resident record exists for accepted boot allocations. | Tie every page-table reservation to the sole issuer and exclude image, boot metadata, device and other live allocations. The owner number is not a capability. |
| **Accessible mapping** | Read-only QMP samples and separate address/permission models are under review. | Prove that each specific reserved frame is actually writable through a verified current mapping before zeroing or constructing tables; distinguish sampled aliases from exhaustive coverage. |
| **Initialized content** | No production table-frame zeroing or builder exists. | Establish a bounded, checked zero/init path and verify bytes, failures and table-entry encodings without publishing an incomplete root. |
| **Survival and reclamation** | The M06 design documents code/stack/descriptor survival obligations; reclamation is disabled. | Preserve all live mappings across a gated future CR3 switch; keep uncertain frames quarantined. TLB, multicore, bootloader lifetime and DMA/IOMMU assumptions require separate evidence. |

**Specific next test, when prerequisites exist:** extend the single-issuer
allocator with a typed, non-cloneable table-frame reservation, with host
negative tests for duplicate issuance, excluded map regions and failures
after only part of the table has been staged. Do **not** manufacture an
authority-bearing type around unchecked \`FrameReservation\` metadata; start
with a review of how the validated source becomes a unique live issuer.
Pair any later actual frame zeroing and mapping construction with controlled
QEMU checks, not just a green pure-Rust model.

References: [Limine memory layout/HHDM and memory-map contract](https://github.com/limine-bootloader/limine-protocol/blob/trunk/PROTOCOL.md),
[seL4 untyped memory and retype](https://docs.sel4.systems/Tutorials/untyped.html),
[Linux kernel page-table permissions and failure semantics](https://docs.kernel.org/mm/kernel-page-tables.html),
[Embers boot allocator handoff](../rust-kernel/BOOT_ALLOCATOR_HANDOFF.md),
[M06e allocator caveats](../rust-kernel/M06E_TRANSACTIONAL_RESERVATIONS.md),
[frame registry design](../rust-kernel/M06D_RANGE_REGISTRY.md),
and [M06 transition proposal](PAGING_TRANSITION_V1.md).

### CPU mappings do not confer device authority (later milestone)

Linux's DMA documentation distinguishes CPU virtual addresses, physical RAM
and device-visible DMA addresses. The IOMMU and DMA mapping lifecycle have
different contracts from CPU page-table traversal.

- **Embers implication:** a frame is not safe to transfer or recycle just
  because a CPU PTE was removed, a capability was logically revoked, or a Rust
  reference ceased to exist.
- **Deferred experiment:** construct a host-only state model with separate
  `cpu_mapping`, `device_mapping`, `in_flight` and `quiesced` states.
  Require explicit device quiescence before reclaim; test revocation racing
  with in-flight operations. Avoid claiming physical DMA protection until
  actual hardware/IOMMU paths and device behavior are reviewed.
- **Evidence limit:** such a model checks proposed ordering, not a real
  device, its firmware, or the IOMMU.

Reference: [Linux DMA mapping guide](https://docs.kernel.org/core-api/dma-api-howto.html).
Related Embers work: [research comparisons: ownership transfer](RESEARCH_COMPARISONS.md#ownership-transfer-and-accelerator-isolation).

### Lock ordering, revocation, and concurrency (after isolation)

Linux's `lockdep` validates observed lock-dependency scenarios and can detect
potential deadlock cycles. It is an instructive example of making an implicit
concurrency assumption observable, but not a ready-made algorithm for
capability revocation.

- **Embers implication:** future endpoint locks, frame ownership changes,
  capability-table access and revocation will need declared acquisition order
  and linearization points.
- **Deferred experiment:** model two conflicting resource-acquisition orders
  and assert a detected cycle; then test a proposed single order and explicit
  revocation/IPC commit points. Compare the result with existing finite-state
  revocation research, rather than treating observed schedules as an
  exhaustive hardware-concurrency proof.
- **Prerequisite:** protected tasks, a syscall/IPC boundary and real
  synchronization mechanisms are not yet on `main`.

References: [Linux lock dependency validator](https://docs.kernel.org/locking/lockdep-design.html)
and [Embers IPC v1 contract](IPC_V1_CONTRACT.md).

### Small changes and reviewable evidence (use now)

Linux's contribution guidance emphasizes small, self-contained patches,
clear problem statements, measured optimization claims, and review of
behavioral consequences.

- **Embers application:** preserve one concern per PR and record the relevant
  base/head, exact files, expected negative fixtures, observed QEMU CPU
  profiles, and what each check does **not** prove.
- **Immediate implication:** seek independent, exact-head review for PRs #81
  and #83 as requested in issue #84. Do not combine their unverified
  observations with draft privileged-transition work or infer a safe CR3
  transition from green host tests.
- **Evidence limit:** this is a process discipline; review and CI reduce
  uncertainty but are not a formal proof or hardware security guarantee.

References: [Linux development HOWTO](https://docs.kernel.org/process/howto.html),
[patch submission guide](https://docs.kernel.org/process/submitting-patches.html),
and [Embers review standard](CODE_REVIEW_STANDARD.md).

**Research-to-implementation gate:** the first new code candidate from this
comparison is a **host-only M06 partial-construction/fault-injection test
matrix**, contingent on avoiding duplication with existing research in draft
PR #66 and pending reviewer findings in #81/#83. It must not change the
normal QEMU boot image, write page tables, switch CR3, implement IPC, or
claim a proven isolation boundary. Decide the exact test gap from current
`main` and PR heads before opening a code PR.
