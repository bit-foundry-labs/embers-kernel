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
