# International research: implications for Embers

Status: research notes and test proposals, not implemented kernel guarantees.
Source-language summaries are translated/paraphrased; verify details against the
linked papers before adopting mechanisms. Dates and scope are given below to
distinguish tested systems, educational material, and preprints.

## Memory authority across service boundaries

**Li et al., “Harmonizing Security and Performance in Microkernel File
Servers”, Journal of Computer Science and Technology 40(2), 2025.**
[Chinese journal entry](https://jcst.ict.ac.cn/cn/article/doi/10.1007/s11390-025-3762-3).
The paper observes that a required user-space service may still have broader
access to application data than the application wants to grant. Its Mirage
file-server design examines that security/performance tradeoff.

Embers implication (proposal): do not equate moving a service to user mode with
least privilege. A service should receive bounded object-specific authority;
future file/model services must not automatically inherit all client authority.
Test wrong-object and confused-deputy cases, revocation during a request, and
whether service crashes leak or retain access. These tests require real caller
identity and process isolation that Embers does not yet have.

**Gu et al., “BrickOS: a building-block kernel for heterogeneous hardware”,
Scientia Sinica Informationis 54, 2024** (Chinese).
[Journal paper](https://scis.scichina.com/cn/2024/SSI-2022-0413.pdf).
It explores configurable system components and a hardware-protection
abstraction. It is a useful comparison for placement and isolation tradeoffs,
not a decision to run Embers services in the kernel. We must measure IPC and
fault containment before changing the microkernel trust boundary.

**Ishikawa et al., HR-TECS, Computer Software 31(4), 2014** (Japanese).
[Journal issue and abstract](https://www.jstage.jst.go.jp/browse/jssst/31/4/_contents/-char/ja).
This embedded real-time work studies component interfaces across protected
domains and reports predictable overhead in its evaluation. Consider it a
historical interface-design comparison, not evidence for x86 desktop latency.

## Accelerator authority and device state

**Metzger et al., “Deprivileging Low-Level GPU Drivers Efficiently with
User-Space Processes and CHERI Compartments”, ACM CCS 2025.**
[Cambridge author copy](https://www.cl.cam.ac.uk/~tmj32/papers/docs/metzger25-ccs.pdf).
Investigates lowering GPU-driver privilege with OS processes and CHERI.
Question for Embers: what minimum command-submission, reset and mapping
authority must remain privileged, and how can driver failures be contained?
CHERI is not assumed available on our initial x86-64 target.

**Bertschi et al., “Devlore: Extending Arm CCA to Integrated Devices”,
ETH Zurich, 2024.**
[University publication page](https://syssec.ethz.ch/publications/2024-08-11-devlore/).
Device protection involves interrupt isolation as well as memory. Proposed
adversarial scenarios: unexpected completion after revoke, interrupt delivery
to a replaced owner, hung device, and reset failure. ARM CCA assumptions do
not directly transfer to x86 or commodity GPU hardware.

**Gu et al., “Behind Bars”, USENIX Security 2026.**
[Conference paper](https://www.usenix.org/conference/usenixsecurity26/presentation/gu-cheng).
Researchers report cross-instance GPU cache interference in a particular MIG
setting. Memory access isolation does not imply protection against all timing
channels. Specify attacker, device, and side-channel scope separately.

## Desktop interaction and AI workload experiments

**Xin et al., “Inference in the Shadows: Sereno”, OSDI 2026.**
[Conference paper](https://www.usenix.org/conference/osdi26/presentation/xin).
Studies contention between local inference and foreground mobile applications,
including shared memory bandwidth. Desktop relevance is a hypothesis; reproduce
on target hardware before applying its numerical results.

Future experiment: compare deterministic fixed priorities/budgets, a simple
bandwidth-aware policy, and a bounded adaptive recommendation policy. Under
identical workloads measure p95/p99 frame time, input latency, audio deadline
misses, time to first token, tokens/second, and energy where measurable. Report
hardware, topology, driver, model, workload, sample count, distributions and
failures. Include policy timeout, invalid hints and hung devices; fallback must
not depend on the model.

**Shen et al., “XSched”, OSDI 2025.**
[Conference paper](https://www.usenix.org/conference/osdi25/presentation/shen-weihang).
Compares scheduling mechanisms across heterogeneous accelerators. Represent
capabilities such as supported preemption and reset per device, rather than
asserting universal mid-kernel preemption or zero-copy.

**Zheng et al., “gpu_ext”, arXiv preprint, 2025.**
[Preprint](https://arxiv.org/abs/2512.12615). Studies programmable GPU policies;
reported numbers are workload- and system-specific and require independent
validation. It is not a reason to add untrusted code to ring 0. Compare a narrow
user-space recommendation API with validated, bounded kernel actions first.

## What changes now

1. Keep the current page walk as a host model, with explicit unsupported cases.
2. Before CR3 changes, specify live mappings, persistent frame authority,
   CPU translation invalidation, and failure behavior.
3. Before process-to-process page lending, design authenticated callers,
   alias tracking, revocation, and crash behavior. Before involving devices,
   add IOMMU, in-flight DMA, interrupt, reset, and cache-coherence contracts.
4. Keep accelerator scheduling, confidentiality and desktop claims as future
   hypotheses. Rust ownership is helpful but does not prove hardware isolation.
5. Research is global: compare university and peer-reviewed work in source
   languages where possible. Do not infer novelty from language or geography.
