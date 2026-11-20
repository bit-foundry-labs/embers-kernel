# Embers Kernel: architecture research review for AI-oriented systems

**Status:** Research proposal; not an accepted architecture decision or an implementation plan approved for privileged code.
**Review date:** 2026-11-20.
**Scope:** original Rust `no_std` x86-64 microkernel research, headless QEMU; Linux used as an independent experimental control. Emberglass OS remains a separate Linux-based environment.
**Method:** comparative literature review of primary research publications and official kernel documentation, checked against Embers' default-branch code and design notes. No new performance experiments, security proofs, privileged QEMU transition, or physical-hardware validations were performed for this review.

## Executive decision

**Proposed research thesis:** Adaptive AI-oriented resource policies should run outside the trusted kernel, while a small, deterministic enforcement core authenticates resource authority, enforces bounded admission/accounting, and preserves a safe fallback. Treat this as a falsifiable hypothesis, not an established advantage.

**Near-term decision:** Prioritize verified memory authority and the M06 boot/paging prerequisites. In parallel, experiment with CPU scheduling and memory pressure on Linux and in a pure host model; defer a production Embers scheduler, protected IPC, accelerator broker, Linux compatibility layer, and in-kernel inference until their security and measurement prerequisites exist.

**Reasoning:** A novel OS must demonstrate a property that existing mature systems do not provide adequately for the target workload. A new kernel is *not* automatically a faster or safer platform for AI. Policy control and latency improvement are separable from privileged kernel construction; therefore benchmark them independently first.

## 1. Evidence: current Embers capabilities and gaps

| Existing default-branch artifact | Evidence currently available | Cannot establish |
| --- | --- | --- |
| Rust `no_std` Limine/QEMU boot, serial READY, GDT/TSS/IDT, fault tests | Selected bootstrap/exception behavior on documented emulator profiles | General-purpose OS, production isolation or reliable crash recovery |
| Limine map validation, one-shot production frame-pool permit, `TransactionalFrameAllocator` and resident `KernelState` | Bounded numeric bookkeeping, unique safe-Rust pool issuance within the current image, local overlap rejection and non-recycling reservations | Authenticated physical ownership across all access paths, zeroed frames, page-table lifetime, absent CPU/device aliases |
| Leaf encoder, page-walk host models, draft PRs #101/#102 | Synthetic PTE encoding, partial-staging failures and selected alias behavior in host tests | Actual table construction, kernel-controlled CR3, TLB correctness or proven protection |
| `experiments/rust-policy-guard` | Simulated policy validation, replay limits, CPU-minimum admission and fallback decisions | Delivered CPU time, a scheduler, tail latency, fairness or preemption |
| `docs/IPC_V1_CONTRACT.md`, `research/formal/revocation/` | Proposed nonblocking IPC semantics and finite-state revocation models | Authenticated ring-3 callers, an IPC implementation or proof of concurrent production revocation |

The current root README explicitly says `main` does **not** implement kernel-owned page tables, ring-3 tasks, syscalls, protected capability tables, IPC, DMA isolation or a deterministic multitasking scheduler. Preserve this boundary in future claims.

**Source-of-truth links:** [current state](../README.md), [M06 paging contract](PAGING_TRANSITION_V1.md), [allocator handoff](../rust-kernel/BOOT_ALLOCATOR_HANDOFF.md), [transactional-reservation caveats](../rust-kernel/M06E_TRANSACTIONAL_RESERVATIONS.md), [IPC v1 proposal](IPC_V1_CONTRACT.md), [policy experiment](../experiments/rust-policy-guard/README.md), [formal revocation harness](../research/formal/revocation/README.md), [research comparison](RESEARCH_COMPARISONS.md). [Draft #101](https://github.com/bit-foundry-labs/embers-kernel/pull/101) and [draft #102](https://github.com/bit-foundry-labs/embers-kernel/pull/102) are not assumed to be merged.

## 2. Architecture alternatives, with trade-offs

| Candidate | Strength | Risk / cost | Current position |
| --- | --- | --- | --- |
| Linux-based user-space resource broker (Linux cgroups v2, PSI, optional sched_ext, Landlock) | Rich drivers, functional hardware support, mature measurement tools, immediate usable baseline | Kernel-dependent behavior; capabilities and features vary by kernel/configuration; security depends on complete sandbox setup | **Preferred near-term experimental platform**, not Embers code |
| Original small capability-oriented microkernel (Embers) | Narrow authority boundaries, user-space services, independently specified admission and resource lifetime | Bootstrapping complexity, IPC/context-switch costs, driver isolation and hardware lifetime proofs | **Long-term research target**, contingent on staged evidence |
| Rust framekernel-style OS (ASTERINAS comparison) | Separates safe/unsafe components while targeting practical OS services and compatibility | Different security boundary and ABI goals; copying the design would obscure Embers' microkernel hypothesis | Comparative reference only |
| GPU-side resource management (Nixie, LithOS, XSched, MoonBright) | Hardware-specific improvements to contention, latency, scheduling or memory management | Hardware/driver dependence, licensing/tooling, preemption and DMA assumptions; many mechanisms live above the CPU kernel | Later *user-space/device-service* research, not a present kernel subsystem |

An outcome in which Linux plus a broker performs as well or better under the chosen security assumptions is a **valid research result**. It may motivate evolving Emberglass OS sooner while preserving Embers as a narrower systems-research project.

## 3. Research tracks and evidence requirements

### P0. Physical memory authority and lifetime before M06

**Question:** Can all allocations for an eventual page-table root be traced to one authenticated issuer and retained until no CPU or device can reference them?

- Compare [seL4's untyped-memory retype and derivation model](https://docs.sel4.systems/Tutorials/untyped.html) with Embers' numeric reservation registry. Retyping requires an authority-bearing capability; a recorded physical number is not equivalent.
- Inventory existing Embers bypasses: direct raw pool, independent registries, smoke-ledger paths, trusted bootloader pointers, unexpected aliases. Capture exact call sites rather than claiming they are closed.
- Specify a *candidate* non-cloneable reservation token issued only by a sole kernel-owned source; do **not** wrap unchecked caller-supplied metadata in a token and call it secure.
- Separate these proof obligations: eligible region; unique issuer; reservation; verified old mapping; zero/init; no unaccounted conflicting CPU aliases; survival of code/stack/IDT/TSS across a possible root switch; durable lifetime; eventual TLB/device quiescence.
- Negative experiments: independently created overlapping sources, out-of-map frame, forged owner ID, arithmetic overflow, untracked alias, failure during construction, attempted reuse before quiescence.
- **Gate:** No new page-table writes or CR3 transition until independently reviewed M06 mapping/stack/provenance evidence is resolved. The read-only QMP PRs #81/#83 remain constrained by [issue #84](https://github.com/bit-foundry-labs/embers-kernel/issues/84); draft #66 is not a demonstrated hardware transition.
- **Evidence limit:** Limine inputs are bootloader-trusted; tests about symbolic ownership cannot prove absence of hardware aliases or DMA.

[Linux page-table aliases and non-atomic permission updates](https://docs.kernel.org/mm/kernel-page-tables.html) specifically motivate refusing to recycle uncertain frames after partial failure. [Linux DMA mapping](https://docs.kernel.org/core-api/dma-api-howto.html) distinguishes CPU and device-visible addresses.

### P1. Safe, policy-neutral resource admission and fallback

**Question:** Can untrusted, replaceable policies improve mixed-workload service while being unable to create authority or exceed reserved budgets?

- Compare [Linux sched_ext](https://docs.kernel.org/scheduler/sched-ext.html) and its fallback semantics, [cgroups v2 CPU/memory controllers](https://docs.kernel.org/admin-guide/cgroup-v2.html), and [seL4 mixed-criticality scheduling contexts](https://docs.sel4.systems/Tutorials/mcs.html).
- Kernel-side *future contract*: authenticate principal and generation; check capability rights; reserve finite CPU/memory resources; reject or admit atomically; account delivered service; enforce cancellation/revocation and bounded recovery; retain a baseline independent of advisory policies.
- Advisory inputs: class (interactive/background), requested capacity, deadline or priority hint, validity epoch, and optional workload observation. These inputs are **never** credentials, raw device permissions or an unconstrained kernel command.
- Extend existing `rust-policy-guard` only after a concrete gap review; it already tests validation and minimum-reservation arithmetic. A separately scoped discrete-event scheduler simulator would test actual per-tick service, starvation, timeouts and policy crashes.
- Define baseline policies *before* seeing the results: FIFO, fixed-priority with protected interactive reserve, and optionally weighted round-robin. Compare a clearly specified adaptive candidate; no neural model is required.
- **Safety gate:** accepted plans may be suboptimal but must not exceed authority/budgets. On policy failure, recover the baseline without silently abandoning already admitted reservations. State any unknown starvation bound rather than inventing one.
- **Evidence limit:** host simulation is not proof of real interrupt accounting, scheduler preemption or hardware-level temporal isolation.

### P1. Linux as a reproducible measurement control

**Question:** Do observable CPU/memory contention signals improve policy decisions under realistic local AI-like workloads?

- A read-only first pass can record `uname -r`, `/proc/pressure/{cpu,memory,io}`, cgroup v2 mount/configuration, and the actual availability of `sched_ext`, BPF and DAMON. Do not assume availability in WSL2 or a particular distribution.
- Use three classes: latency-sensitive interactive requests, a bursty background indexing task, and memory-heavy local inference or a declared synthetic surrogate. Record workload identity and whether a GPU is used.
- Linux [PSI](https://docs.kernel.org/accounting/psi.html) measures resource stalls; [DAMON](https://docs.kernel.org/mm/damon/design.html) samples access patterns with accuracy/overhead trade-offs. Neither is a universal proof of useful prediction.
- Experimental design: fixed input traces and resource limits, repeated runs, controlled warm-up, logged CPU/GPU configuration and thermal mode, randomized order of candidate versus baseline, p50/p95/p99 latency, throughput, missed deadlines, stall pressure, memory peaks, fairness and measurement overhead.
- Interventions such as changing cgroup limits, loading a BPF scheduler or enabling DAMON are **separate opt-in experiments** on a disposable Linux environment with recorded permissions and configuration; never run them on a live development machine by implication.
- Pre-register what constitutes an improvement *after establishing variance and a baseline*. Reject results where p99 or starvation regresses materially despite lower mean latency. Preserve raw traces and scripts.
- **Gate:** Demonstrate a reproducible advantage over a competent Linux baseline before claiming that Embers needs a custom AI-aware scheduler.

### P1. Capability revocation + IPC ordering

**Question:** Can resource authorization, message commit, revocation and reclamation be linearized without permitting late use or premature reuse?

- Build directly on [IPC v1 contract](IPC_V1_CONTRACT.md), [issue #14](https://github.com/bit-foundry-labs/embers-kernel/issues/14) and the existing `research/formal/revocation/` TLA+ artifacts.
- Model `GRANTED -> ACTIVE -> REVOKING -> QUIESCING -> RECLAIMABLE` as proposed *abstract states*, not implemented APIs.
- Distinguish a queued, already committed message from one admitted after revocation; ensure stale capability generations fail; keep in-flight resources retained until the required completion acknowledgements occur.
- In the first model assume one CPU and no device DMA. Add concurrency, device queues, IOMMU mappings and crash recovery as explicit later model refinements, not assumptions hidden inside the first proof.
- Compare [CONVEROS's incremental model-checking approach](https://www.usenix.org/conference/atc25/presentation/tang) and [Atmosphere's Rust/Verus microkernel verification](https://mars-research.github.io/projects/atmo/). Neither automatically proves Embers.
- **Gate:** Model-check selected safety invariants; produce a counterexample for at least one deliberately weakened variant; document state-space limits, fairness assumptions and the plan to check eventual code conformity.

### P2. Accelerator lifecycle and interference

**Question:** Can device work be treated as a capability-bounded, asynchronous resource even when cancellation or preemption is unavailable?

- Compare [XSched (OSDI 2025)](https://www.usenix.org/conference/osdi25/presentation/shen-weihang), [LithOS (SOSP 2025)](https://pdl.cmu.edu/ftp/BigLearning/lithos_sosp25_abs.shtml), [Nixie (OSDI 2026)](https://www.usenix.org/conference/osdi26/presentation/xu-yechen), and [MoonBright (OSDI 2026)](https://www.usenix.org/conference/osdi26/presentation/zhang-yangyu).
- These systems tackle *different* problems: heterogeneous command preemption, GPU execution/partitioning, consumer GPU time-sharing, and GPU-side page-table materialization. Their benchmark improvements are not interchangeable, and none demonstrates that the Embers CPU kernel should implement their techniques.
- Future interface fields: `job_handle`, `authorized_device`, `memory_lease`, `budget`, `completion_token`, `cancel_supported`, `in_flight`, `quiesced`; specific names are placeholders.
- Host fault model: non-preemptible operation, delayed completion, stale completion token, device reset, failed cancellation, memory lease revoked while DMA is still pending, and crash before acknowledgement.
- **Gate:** Do not reuse memory while a device can still access it; defer GPU drivers and zero-copy promises until hardware, driver, IOMMU and cache-coherence behavior are independently checked.

## 4. Threat model and trust-boundary inventory

| Boundary | Threat or failure | Current mitigation / missing evidence |
| --- | --- | --- |
| Limine -> kernel boot metadata | malformed/misinterpreted memory map or unexpected lifetime | Bounded validation; bootloader remains trusted; current checks do not establish physical authority |
| Physical allocator -> frame registry | duplicate issuance or bypass through independent pool | One-shot safe-Rust issuer and retained allocator; raw/alternative access and hardware alias proofs remain |
| User policy -> resource admission | forged authority, replay, quota expansion or policy compromise | Host-only policy guard; no authenticated user-to-kernel call site |
| Future task -> IPC endpoint | stale/forged handle, TOCTOU race, partial copy, queue exhaustion | Contract and models only; no ring-3/syscall protection |
| CPU mappings -> GPU/device memory | untracked DMA, stale translation, missed completion | No IOMMU or driver integration; require later device-specific evidence |
| Measurement -> claim | selection bias, thermal throttling, synthetic-only speedups | Pre-registered baselines, repeated trials, p99/fairness/overhead and independent reproduction planned |

**TCB target:** Identify each privileged and `unsafe` operation and its justification; keep model inference, parsing of untrusted rich data, orchestration, graphics, network protocols and model execution in user-space services. [ASTERINAS (USENIX ATC 2025)](https://www.usenix.org/conference/atc25/presentation/peng-yuke) is a useful memory-safety TCB comparison, but its framekernel approach differs from Embers' proposed microkernel. An actual TCB size and soundness claim require a measured inventory and review.

## 5. Sequenced research program with stop conditions

| Sequence | Proposed artifact | Do not proceed until | What would invalidate the hypothesis |
| --- | --- | --- | --- |
| R1 | Read-only M06 frame authority graph + explicit allocator bypass inventory | Source call sites and review limitations cited | A supposedly unique token can be forged or issued twice through a live path |
| R2 | State-machine revocation/IPC safety model extension | Assumptions and linearization points reviewed | A counterexample permits post-revocation admission or unsafe reuse |
| R3 | Linux feature inventory + versioned experimental protocol | Workload and baseline are specified, intervention is opt-in | No reproducible effect over a strong Linux baseline |
| R4 | Discrete-event CPU budget + fallback simulator | Existing policy guard duplication avoided; finite-service semantics declared | Policy failure can strand runnable work or violate accepted reservations |
| R5 | Device completion/quiescence simulator | CPU-only authority and revocation rules stabilize | A simulated delayed completion permits early frame reclamation |
| R6 | Privileged M06 transition work (separate future approval) | Independent review, verified mapping provenance, stack/descriptor survival, unique initialized frames, exact-head QEMU gates | Any uncertain frame ownership, alias, CPU mode, or post-switch fault path |

This sequence is **dependency driven**, not a schedule or a commitment to implement all tracks. Avoid opening overlapping work where existing PRs or contributor issues already own the scope. Existing [#65](https://github.com/bit-foundry-labs/embers-kernel/issues/65), [#84](https://github.com/bit-foundry-labs/embers-kernel/issues/84), [#40](https://github.com/bit-foundry-labs/embers-kernel/issues/40), [#14](https://github.com/bit-foundry-labs/embers-kernel/issues/14), and [#4](https://github.com/bit-foundry-labs/embers-kernel/issues/4) should be reconciled before creating new implementation issues.

## 6. Unresolved architecture decisions requiring explicit ADRs

| Decision | Options to evaluate | Provisional preference |
| --- | --- | --- |
| Frame authority representation | Numeric registry only; token bound to sole issuer; seL4-like derived capabilities | Sole-issuer token after bypass audit, without inventing stronger guarantees |
| CPU policy extension | Hardcoded kernel scheduler; user-space advisory service; Linux sched_ext control | User-space advisory contract in Embers; Linux sched_ext only as experimental control |
| Temporal enforcement | Best-effort weights; minimum budgets; mixed-criticality scheduling contexts | Fixed minimum/budget semantics first; no real-time claim |
| IPC v1 transfer | Copy into kernel queue; shared memory; ownership lending | Keep proposed bounded copy semantics, measure copy cost first |
| Accelerator sharing | GPU driver in kernel; isolated user-space broker; no direct support | Defer physical broker, model asynchronous lifecycle first |
| Compatibility / product track | Linux ABI in Embers; purpose-built IPC; Linux-based Emberglass environment | Preserve a narrow research microkernel; use Emberglass/Linux for nearer-term usable systems |

## 7. Primary reading map and evidence grades

Sources are a curated starting set, not a systematic literature review. All reported performance effects are paper-specific and must not be transferred to Embers without replication.

| Source | Contribution relevant to Embers | Classification and caveat |
| --- | --- | --- |
| [Linux kernel page tables](https://docs.kernel.org/mm/kernel-page-tables.html) | Mapping aliases, partial permission updates, TLB assumptions | Maintainer documentation; platform-specific details |
| [Linux sched_ext](https://docs.kernel.org/scheduler/sched-ext.html) | Replaceable scheduler policies and fallback | Maintainer documentation; availability/behavior version-specific |
| [Linux cgroup v2](https://docs.kernel.org/admin-guide/cgroup-v2.html), [PSI](https://docs.kernel.org/accounting/psi.html), [DAMON](https://docs.kernel.org/mm/damon/design.html) | Resource enforcement and observation with overhead | Maintainer documentation; feature/configuration dependent |
| [Linux Landlock](https://docs.kernel.org/userspace-api/landlock.html), [DMA guide](https://docs.kernel.org/core-api/dma-api-howto.html) | User-space authority restrictions; device-memory lifecycle | Maintainer documentation; not an Embers implementation |
| [seL4 untyped capabilities](https://docs.sel4.systems/Tutorials/untyped.html), [MCS](https://docs.sel4.systems/Tutorials/mcs.html) | Authority-bearing object creation and CPU-time budgets | Primary project technical docs; proof scope/configuration matters |
| [ASTERINAS](https://www.usenix.org/conference/atc25/presentation/peng-yuke), [CONVEROS](https://www.usenix.org/conference/atc25/presentation/tang) (ATC 2025) | Rust TCB reduction and concurrency model checking | Peer-reviewed published systems research; distinct architecture |
| [Atmosphere](https://mars-research.github.io/projects/atmo/) (SOSP 2025) | Rust/Verus microkernel correctness | Peer-reviewed project/paper; proof assumptions not inherited |
| [XSched](https://www.usenix.org/conference/osdi25/presentation/shen-weihang) (OSDI 2025) | Device preemption abstractions | Peer-reviewed; hardware-specific |
| [LithOS](https://pdl.cmu.edu/ftp/BigLearning/lithos_sosp25_abs.shtml) (SOSP 2025) | GPU scheduling and workload atomization | Peer-reviewed; data-center GPU context |
| [Nixie](https://www.usenix.org/conference/osdi26/presentation/xu-yechen), [MoonBright](https://www.usenix.org/conference/osdi26/presentation/zhang-yangyu) (OSDI 2026) | Consumer GPU temporal sharing; GPU memory management | Peer-reviewed; not portable CPU-kernel speedup claims |

## 8. R1 initial source trace: what the allocator actually controls

**Read-only checkpoint, 2026-11-20:** the following call-path inventory was checked in the `main` source at PR creation base `8fdae2bcf4e761333a211282206b08a681e8a319`. This is a targeted source trace, **not** an exhaustive whole-repository audit or proof of exclusive hardware ownership.

| Step | Concrete source | Interpretation |
| --- | --- | --- |
| Permit / initial pool | [`memory.rs` lines 117-151](https://github.com/bit-foundry-labs/embers-kernel/blob/8fdae2bcf4e761333a211282206b08a681e8a319/rust-kernel/src/memory.rs#L117-L151), [`boot_memory.rs`](../rust-kernel/src/boot_memory.rs) | An atomic one-shot permit is consumed before selecting a pool from Limine's reported regions. It prevents normal safe-Rust re-issuance **through that permit**, not an independent privileged memory access. |
| Boot handoff | [`main.rs` lines 237-248 and 327-385](https://github.com/bit-foundry-labs/embers-kernel/blob/8fdae2bcf4e761333a211282206b08a681e8a319/rust-kernel/src/main.rs#L327-L385) | Normal boot constructs the transactional allocator and moves it into `KernelState`; a second `initial_frame_pool()` is deliberately rejected. |
| Numeric reservation | [`frame_registry.rs` lines 127-184](https://github.com/bit-foundry-labs/embers-kernel/blob/8fdae2bcf4e761333a211282206b08a681e8a319/rust-kernel/src/frame_registry.rs#L127-L184) | `reserve()` previews the run, validates against the local registry, advances the same pool and records the result. This is coherent bookkeeping in the current single-writer path. |
| Low-level API exposure | [`memory.rs` lines 67-114](https://github.com/bit-foundry-labs/embers-kernel/blob/8fdae2bcf4e761333a211282206b08a681e8a319/rust-kernel/src/memory.rs#L67-L114) | A holder of a mutable `FramePool` may call `reserve_contiguous()` without the registry. This is an **API design risk for future code**, not proof of a bypass exercised by ordinary boot today. |
| Feature-gated smoke path | [`main.rs` lines 445-460](https://github.com/bit-foundry-labs/embers-kernel/blob/8fdae2bcf4e761333a211282206b08a681e8a319/rust-kernel/src/main.rs#L445-L460), [`frame_registry.rs` lines 179-183](https://github.com/bit-foundry-labs/embers-kernel/blob/8fdae2bcf4e761333a211282206b08a681e8a319/rust-kernel/src/frame_registry.rs#L179-L183), [`frame_ledger.rs` lines 78-101](https://github.com/bit-foundry-labs/embers-kernel/blob/8fdae2bcf4e761333a211282206b08a681e8a319/rust-kernel/src/frame_ledger.rs#L78-L101) | The optional `boot-ownership-smoke` feature obtains the raw pool and claims frames for an isolated smoke ledger. It is not the default path and ledger IDs are not globally authenticated. |
| Synthetic duplicated sources | [`kernel_state.rs` lines 32-62](https://github.com/bit-foundry-labs/embers-kernel/blob/8fdae2bcf4e761333a211282206b08a681e8a319/rust-kernel/src/kernel_state.rs#L32-L62) | Host tests intentionally create independent overlapping pools to show why numeric frame IDs alone cannot grant global authority. The unrestricted `choose_pool()` selector in `memory.rs` is `cfg(test)`, not ordinary production pool creation. |

**Initial assessment:** The current single-BSP normal path is narrower than an unrestricted allocator, and the tests explicitly acknowledge that distinction. The security gap is *not* simply “two allocators currently run on normal boot.” It is that future table construction could treat a legitimate numeric reservation as proof of exclusive ownership, live mapping, zeroed content, and revocation safety, even though those properties have not been established.

**R1 follow-up acceptance:** (a) inspect all direct/indirect `FramePool` and physical-memory writers, including feature combinations, `unsafe` operations and assembly; (b) map real Limine region exclusions and lifetimes; (c) model or test duplicate issuance through every permitted constructor without fabricating auth; (d) trace the lifetime of each frame into any later table mapping; (e) document which assumptions remain at the CPU and DMA layers. This follow-up remains review-only until its scope is agreed.

## 9. Research ethics, reproducibility, and contribution boundaries

- Do not describe the proposed design as secure, verified, faster than Linux, or AI-optimized without the appropriate implementation and reproducible evidence.
- Preserve license notices; this document paraphrases engineering concepts and links sources, not paper text or copied code.
- Log exact test heads, CPU profile, kernel configuration, benchmark inputs and failure traces. Passing CI alone does not establish physical isolation.
- Independent review is especially valuable for paging, `unsafe` code, concurrency/linearization, capabilities and experimental measurement design.
- Do not use real hardware or change privileged transition logic merely because a host model passes.
- This review does not approve feature development, merging pending PRs, changing issue ownership, or changing governance.

**Recommended immediate review action:** review this document against [PR #101](https://github.com/bit-foundry-labs/embers-kernel/pull/101), [PR #102](https://github.com/bit-foundry-labs/embers-kernel/pull/102), and the active M06 review gate [#84](https://github.com/bit-foundry-labs/embers-kernel/issues/84). Then select **R1 frame-authority inventory** as a read-only code-tracing task, not a new privileged implementation.
