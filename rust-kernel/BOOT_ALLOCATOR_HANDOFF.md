# Boot allocator handoff contract: proposal

Status: partially implemented boot-time handoff, with unresolved hardware authority. As of
`main` commit `ab2d36d` (PRs #59 and #60), the BSP moves the
`TransactionalFrameAllocator` into `KernelState` and then into `run_kernel`.
The kernel still halts without a general runtime allocator or a page-table builder.
A single-production-issuer permit restricts safe Rust pool construction in this
kernel image; it does not prove exclusive access to physical frames, mappings,
or devices.

- One bootstrap owner receives the validated pool and reservation registry
  together. The pool must not be reconstructed for the same physical range.
- The owner moves into the kernel runtime state before that runtime can allocate.
  No copied allocator, independent overlapping pool, or unaudited raw-pool
  access may issue frames.
- Table and transition-stack frames are reserved before use. Each reservation
  remains recorded for as long as a CPU might walk or access those frames.
- Initialization failure leaves no published partially initialized runtime.
  Reserved frames are not recycled merely because a handle or owner is dropped.
- Start with a single BSP and interrupts disabled. Before interrupts or multiple
  CPUs use this state, define synchronization, TLB invalidation, and quiescence.
- Device DMA, IOMMU ownership, reclaim, and real caller authentication require
  separate designs. No isolation or exclusive physical ownership is established.

Current evidence: host tests exercise moving the owner, retaining the initial
reservation and making a disjoint subsequent reservation. Production pool
construction uses a one-shot, non-cloneable permit; compiler-negative tests
reject a hidden constructor, cloning and use after move. Existing QEMU boot
regressions passed for PRs #59 and #60 on their tested heads. None of this
validates exclusive physical-memory access or new page-table mappings.

Future acceptance work: reserve and initialize actual table/transition-stack
frames through the retained owner, prove the live mapping and lifetime contract,
verify no `CR3` writes on failed preparation, and collect gated QEMU observations.
The kernel must not treat host-model tests as hardware protection evidence.
