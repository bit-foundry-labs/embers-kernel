# M06q: One-shot RSP switch into a dedicated BSS stack

**Status:** QEMU-only experiment. This stack is owned as a private **Rust
static in the kernel image**, not as an exclusively allocated physical frame
reservation. **No kernel-owned CR3, post-switch recovery or hardware isolation.**

## Boundary and experiment

The existing `transition-stack-smoke` feature already validates a dedicated,
4 KiB-aligned 16 KiB BSS buffer using volatile writes under Limine's inherited
mappings, and read-only QMP traverses every stack page. The new
`transition-stack-switch` feature uses the same private storage, but
*actually changes RSP* for a single bounded, no-argument callback.

The switch is isolated in a short x86-64 SysV assembly function. It saves
callee-saved `RBX` on the old stack, saves the old `RSP` in `RBX`, installs
the aligned BSS-stack top in `RSP`, calls a witness returning its observed
callback `RSP`, and then restores the old `RSP`, `RBX`, and returns to
Rust. This is needed because ordinary inline Rust cannot safely assume its
own frame remains valid while the stack pointer moves. It is not an
interrupt-safe or exception-recoverable general context switch.

The single BSP first verifies the private static BSS range using the existing
four-page write/read/restore probe. Range validation requires that the **exclusive
stack top itself** be representable as a 64-bit RSP, not just the last byte of
the allocation. This prevents a numeric range ending at `u64::MAX` from
wrapping its stack-top pointer. The host regression checks this boundary. It
accepts a returned callback pointer
only inside the BSS stack's *top 4 KiB*, with natural alignment, and only when
the RSP sampled before and after the ABI call matches. Failure halts with
an explicit error before READY; this test cannot restore after a fault that
never returns normally.

## Reproducible checks

- `make rust-transition-stack-test`: host-only numeric storage and
  callback/RSP restoration validators, including wrong boundary/alignment and
  simulated non-restoration.
- `python3 scripts/check_transition_stack_switch.py --self-test`: reject
  missing/duplicate/malformed serial evidence, out-of-range callback RSP,
  invalid ELF bounds and missing READY/restoration.
- Boot matrix: build an isolated `--features transition-stack-switch` image
  on each of `max` and `qemu64`; require the explicit post-callback
  restoration marker, matching feature ELF/RSP extent and normal READY.
- The host's existing `check_active_pte_qmp.py --feature-stack` runs on
  this *exact* switch image and ISO, verifies the four stack pages and every
  ELF LOAD page through paused QEMU's inherited CR3 and checks writable PTE
  chains. The feature cannot be combined with fault/ownership/stack smoke
  modes. All normal and fatal-fault regression tests remain required.

## Remaining risk and gates

The probe does not prove physical frame exclusivity, absence of aliases,
DMA/IOMMU isolation, the original Limine stack's lifetime, or any mapping
survival under a *different* CR3. The assembly contains privileged execution
assumptions (single BSP, interrupts already disabled, valid existing GDT,
IDT and IST, SysV ABI callback that returns). Unexpected faults remain fatal;
there is no atomic rollback.

Do **not** interpret the kernel-image BSS storage as a frame allocated by
the boot registry. A future owned-root transition requires independently
authenticated page-table frame backing, mappings for executing code and all
descriptor/exception paths, actual ownership/lifetime evidence, and review of
the CR3 handoff. Draft #66 must remain fail-closed.
