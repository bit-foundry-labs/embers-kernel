# IPC v1: Admission, copying and revocation state-machine proposal

**Status:** Design candidate only; no implemented IPC, executable model or verified security guarantee. Tracking: [#40](https://github.com/bit-foundry-labs/embers-kernel/issues/40). Baseline: [ee4bbb9](https://github.com/bit-foundry-labs/embers-kernel/commit/ee4bbb9150f1e09d1f0960a95b96553c2df5b069). Extends the [proposed IPC contract](IPC_V1_CONTRACT.md) without changing it; any semantic refinements below require explicit acceptance.

## 1. Research boundary

A bounded, nonblocking, copied FIFO is our candidate baseline, not a scheduler or a syscall ABI. The kernel alone authenticates callers, checks SEND/RECEIVE rights, owns message storage and controls endpoint lifetime. An AI service is just another untrusted caller. It cannot grant authority or decide when a buffer is safe to reuse.

The [existing revocation research](../research/formal/revocation/README.md) distinguishes logical closure from tracked in-flight work. Reuse that distinction, not its simulated hardware claims. [#112](https://github.com/bit-foundry-labs/embers-kernel/pull/112) is host-only attenuation/budget research, not an IPC capability table. Pending #111/#113 are not prerequisites for this design note.

## 2. Abstract state

- Endpoint identity: protected object slot and generation; lifecycle Open, Closing, Retired. Generation exhaustion retires the slot rather than wrapping. Closing and capability revocation are separate events.
- Authority: protected caller-space lookup, endpoint generation, rights, effective delegation/revocation state, expiration. Caller-supplied identifiers are not authority.
- Queue: FIFO of distinct message IDs, immutable kernel-owned byte sequences of length 0..M, capacity Q. M and Q are positive fixed parameters; no capability transfer or shared memory.
- Send staging: bounded, private kernel copy associated with a tracked operation and endpoint generation. A staged message is not yet queued.
- Receive reservation: at most one exclusive reservation of the queue head per endpoint, bound to a tracked operation, authenticated receiver and stable destination mapping. A reserved head still counts against Q.
- Lifetime: operation holds protect endpoint and relevant mappings/storage; a capability becoming invalid is not proof that a held reference or copy has ended.

For an initial finite model choose M=2 bytes, Q=2 messages, two senders, two receivers, two endpoint slots, a small nonwrapping generation domain, and at most one staging slot per actor. These are exploration bounds, not production sizing. Use a finite payload alphabet and explicit fresh message IDs; bound total operations as necessary to keep the state space finite.

## 3. Candidate transition protocol

The following actions are atomic at the abstract level. A future implementation must justify synchronization across the capability table, endpoint and memory lifetimes. Do not hold a spinlock across a faultable user copy. Acquiring a hold and validating authority must be atomic with respect to revocation/closure.

| Action | Preconditions and effect |
| --- | --- |
| BeginSend | Authenticate caller; verify live SEND authority, open endpoint, bounded length, available staging/hold budget. Acquire tracked hold. No queue mutation. |
| StageSend | Copy from caller into private bounded staging under a stable mapping/lifetime contract; on failure abort and release resources. No queue mutation. Concurrent mutation of source may affect copied bytes; no atomic source snapshot is promised. |
| CommitSend | Revalidate effective SEND authority, expiry, endpoint identity/generation and Open state under the same serialization as revoke/close; require queue space. Atomically publish immutable staged bytes at tail and transfer storage accounting, then release hold. Otherwise discard staging and release hold. Successful enqueue linearizes here. |
| AdmitReceive | Authenticate and validate live RECEIVE authority, Open endpoint, available head, sufficient capacity and stable destination mapping; acquire exclusive head reservation and tracked hold atomically before exposing bytes. No dequeue. Empty returns WouldBlock; short buffer retains message. A competing reservation returns a proposed retryable Busy outcome, subject to contract acceptance. |
| CopyReceive | Only the admitted operation may copy the reserved immutable bytes to its bound receiver destination. A failed copy may have exposed a prefix to that authorized receiver; report error and retain message. Never expose bytes to an unadmitted receiver. |
| FinishReceive | After successful full copy, atomically remove exactly the reserved head and release reservation/hold. On copy failure or aborted operation retain head and release only once copying has stopped. Dequeue linearizes here. |
| Revoke | Atomically invalidate authority (including affected descendants under future capability semantics); deny subsequent admissions and send commits using it. Already committed messages remain queued. Previously admitted receive holds may complete or safely abort. |
| Close | Authorized lifecycle action marks endpoint Closing, denies new admissions and send commits, and preserves outstanding holds. Existing admitted receive can finish or abort. No slot reuse yet. |
| Retire | Only after all holds and reservations drain, copy accesses cease, queued/staged storage is accounted for and scrubbed before reassignment; retire endpoint or advance to fresh generation. A stalled operation is not evidence of quiescence. |

### The receive/revoke distinction

Send has no receiver-visible effect until enqueue; rechecking at commit is safe. Receive can disclose bytes before dequeue. Rechecking only at dequeue is insufficient: it cannot undo a disclosure. The proposal therefore authorizes disclosure at AdmitReceive and tracks the specific outstanding use. A receive admitted before revocation may complete after it; a receive admitted afterward must fail. Closing behaves similarly but denies all new endpoint use.

This is a proposed split-phase refinement of the original contract's synchronization requirement. Admission and dequeue are distinct observable protocol events; do **not** claim whole-call linearizability against revoke for this multi-event protocol until a suitable abstract specification and refinement proof exist. If the project instead requires revocation to prevent all future byte exposure, it needs a different copy/cancellation or revocation-completion contract. Do not pretend revocation can retract bytes already copied.

## 4. Invariants and accounting

| ID | Candidate invariant |
| --- | --- |
| I1 | Every admission uses authenticated caller, correct generation, effective right, live authority and open endpoint at its admission point. No post-revoke admission. |
| I2 | Each successful enqueue revalidates authority at commit; no cached authorization may publish after revocation. |
| I3 | Queue length is 0..Q, payload length 0..M, and dequeue order follows successful enqueue order; each message ID dequeued at most once. Closure discard is separately recorded. |
| I4 | A reserved head cannot be overtaken or removed by another receiver; failure retains it. A payload disclosure must belong to the admitted receiver and its stable authorized destination. |
| I5 | Endpoint/storage/mapping reuse cannot occur while holds or accesses remain. Stale generations cannot authorize new admissions. |
| I6 | Queue, staging, reservation metadata and holds are finite; allocation failure leaves queue and accounting consistent. No double charge or double release. |

Budget fixed-capacity storage, not only live payload bytes: for Q fixed queue slots and S staging slots, payload capacity is at most (Q+S)*M bytes per endpoint, plus separately bounded metadata/alignment/holds and any copy scratch storage. The reservation aliases kernel-owned queued bytes in the abstract model, not user memory, so it consumes no extra payload slot unless a concrete implementation copies it. Use checked arithmetic, a global endpoint count limit, per-owner quotas and an atomic reserve/charge rule. Choose either copy into an already allocated queue slot at commit or transfer a separately allocated slot with equivalent accounting; never charge the same storage as both free and occupied. Zero-byte messages still occupy one slot.

## 5. Required traces and negative controls (not executed)

| Trace | Expected result |
| --- | --- |
| BeginSend(A), StageSend(A), Revoke(A), CommitSend(A) | Denied; FIFO unchanged, staging/hold released exactly once. |
| CommitSend(A), Revoke(A), AdmitReceive(B), FinishReceive(B) after successful copy | B receives committed bytes if independently authorized. |
| AdmitReceive(B), Revoke(B), CopyReceive(B), FinishReceive(B) | Allowed only for the already tracked reservation, per candidate semantics; later admission denied. |
| Revoke(B), AdmitReceive(B) | Denied before any disclosure. |
| AdmitReceive(B), AdmitReceive(C), copy failure(B), retry(C) | C cannot overtake reserved head; B failure retains message and C can later reserve it. |
| AdmitReceive(B), Close, attempted Retire, copy completion or safe abort | Retire denied until all holds/accesses cease and storage cleanup is accounted for. |
| Queue full, staging exhausted, short buffer, oversized payload, zero-byte message | Typed outcomes; no unbounded allocation; failure preserves FIFO and balances. |
| Retire and reuse slot, old handle attempts admission | Denied; exhausted generation retires slot. |
| Caller mapping changes or receiver dies during copy | Model must not declare quiescence until copy/mapping lifetime ends; no unauthorized destination reuse. |

Next experiment: one independent host-only finite-state explorer under research/formal/ipc (no dependency or kernel edits), with a trace oracle independent of transition guards. Enumerate schedules for a bounded domain and deliberately remove one guard at a time: final send revalidation (I2), receive admission check (I1/I4), exclusive head reservation (I3/I4), and retire hold check (I5). Require a counterexample for the intended invariant, not an unrelated exception. Include exact domain, explored states, commands and counterexample traces. No claims of exhaustive real-system behavior.

## 6. Decisions and implementation gate

- Choose production capacities, accounting granularity, error precedence, retryable Busy semantics, endpoint creator/destructor rights and revocation completion semantics.
- Establish stable user mappings, fault-recovery and authenticated syscall origin first. Define lock order, interrupt/preemption behavior, cancellation and teardown. Do not infer safe hardware references from model booleans.
- Measure worst-case copy and admission cost; nonblocking here means no wait for another process, not a guarantee of wait-free execution, fairness or bounded hardware latency.
- No user-facing syscall, actual user copy, protected capability table, scheduling, shared memory, DMA or hardware work in this increment. M06 pre-write safety gate remains unchanged.

## References and transfer limits

- Herlihy and Wing, *Linearizability: A Correctness Condition for Concurrent Objects*, ACM TOPLAS 12(3), 1990, 463–492, [DOI](https://doi.org/10.1145/78969.78972). Use the distinction between abstract operation effects and implementation steps as a correctness target, not a proof of this candidate.
- [seL4 IPC tutorial](https://docs.sel4.systems/Tutorials/ipc): capability-controlled endpoint IPC is a useful comparison, but seL4's small-message synchronous rendezvous is not Embers' proposed bounded asynchronous queue. Do not transfer its guarantees to Embers.
- [Embers IPC contract](IPC_V1_CONTRACT.md) and [split-phase revocation experiment](../research/formal/revocation/README.md): project-specific constraints and modeled precedents.

No executable test or model checker run accompanies this note. Design acceptance, model validation and hardware verification remain distinct gates.
