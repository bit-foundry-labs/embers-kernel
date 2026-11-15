# M06p: Two inherited BSP stack point samples

**Scope:** Read-only, host-side observations of two eight-byte windows under
the active Limine page-table root. No privileged code, CR3 write, or stack
switch is introduced.

## What is measured

The current normal Rust boot already emits a numeric early BSP RSP snapshot.
The host QMP harness now also captures the RSP at the paused post-READY CPU
state alongside the matching CR3. For each pointer it inspects **RSP-8
through RSP-1**, using the existing bounded four-level/4 KiB read-only physical
page walker. It rejects missing/duplicated snapshots, noncanonical and
misaligned values, nonpresent entries, read-only PTE paths, unsupported huge
pages and translations outside the configured 256 MiB observed QEMU RAM.

These two tiny samples are deliberately disjoint observations in time, and
may share a backing page; the test is **not** a stack scan or bound discovery.
The host does not read or modify stack contents. Inherited page tables are
inspected using QMP physical reads after pausing the guest.

The normal boot matrix invokes the enhanced checker for both CPU models.
Synthetic host negatives cover malformed values, absence, duplicate reports,
out-of-mode pointers, unmapped leaves, read-only mappings and unsupported
large-page paths. The opt-in transition-stack BSS test remains separate.

## What is not proved

The serial RSP sample is inside a called Rust function, **not** the original
Limine entry RSP. The second sample is later in the halt path. Neither sample
establishes the entire bootloader stack's base/top, guard pages, allocation
provenance, permitted lifetime, aliases, supervisor-only access or physical
frame authority. RW PTE bits are not themselves an effective-permission
fault test. In particular, swapping a valid leaf mapping to a different
in-RAM physical frame is not detectable without independent provenance.

A transition must therefore use a separately reserved and proven kernel-owned
stack, and retire reliance on the Limine stack before a CR3 root change. The
fail-closed CR3 research gate in draft PR #66 remains blocked.
