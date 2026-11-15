# M06n: Controlled QEMU paging-permission fault experiments

**Scope:** Two opt-in, fatal CPL0 tests against the existing Limine page tables.
Neither modifies CR3, page tables, TLB controls or the normal boot path.

## Motivation

The M06 paging preflight verifies selected CR0.WP / IA32_EFER.NXE bits.
The host QMP walker observes the expected raw ELF/PTE RW and NX flags.
Neither alone proves that the CPU actually raises a protection fault when
a supervisor attempts an illegal write or instruction fetch.

## Experiments

| Feature flag | Deliberate action | Expected fatal #PF | Why |
| --- | --- | --- | --- |
| `fault-ro-write` | Store one byte to a dedicated page-aligned immutable static in the ELF R PT_LOAD segment | `CODE=0x3` | P=1 protection, W/R=1 write, U/S=0 supervisor |
| `fault-nx-exec` | Indirect jump to a dedicated page-aligned static in the ELF RW PT_LOAD segment | `CODE=0x11` | P=1 protection, I/D=1 instruction fetch, U/S=0 supervisor |

The NX target begins with `UD2`. If NX enforcement unexpectedly fails,
execution reaches an unintended #UD instead, which CI rejects. The RO
write operates on one private canary byte; if WP fails and the store
returns, boot emits `ERROR` and halts. Both test pages are aligned and
limited to 4 KiB; neither is used as a normal kernel object or active stack.

The ordinary `run_kernel` path excludes both tests. Feature combinations
with existing fatal fault modes and the separate transition-stack smoke
are rejected at compile time. The tests intentionally stop in the
existing nonreturning #PF exception handler.

## Evidence requirements

The M03 exception workflow builds each feature as a **separate ELF and ISO**.
For each feature-specific ELF and ISO, CI boots **both** `-cpu max` and
`-cpu qemu64` as separate disposable guests using Limine's inherited tables.
The original M03 test job name and `max` serial log filenames are preserved;
additional `qemu64` logs use a `.qemu64` suffix. Each profile must pass the
CPU protection preflight marker and `scripts/check_permission_fault.py`:

1. Exactly one target-address diagnostic, with an aligned 48-bit canonical
   address completely within that test image's **R** or **RW** ELF segment.
2. Exactly one #PF report; saved RIP must be valid and, for NX, equal the
   instruction-fetch target.
3. CR2 exactly matches the reported target, and the complete page-fault
   error code is **exactly** `0x3` or `0x11`. A generic `#PF` line does
   **not** pass.
4. No `READY`, no `ERROR`, and no alternative exception report.

A success in one CPU profile never substitutes for a missing or failed
result in the other. Every permission run uses the corresponding feature ELF
for target membership checks, without relying on addresses from another build.

The host-side parser also exercises negative cases for mismatched error
codes, wrong CR2/RIP, missing reports, duplicate reports, wrong ELF segments
and accidental normal boot. Existing normal, #UD, missing-page #PF and #DF
tests must continue to pass. No memory or ownership mechanism is modified.

## Safety boundaries

- Both experiments run **only in disposable QEMU test VMs**, never in
  ordinary boot or production. The two new modes use the existing BSP
  GDT/IDT and nonreturning exception reporting; no new interrupt handler.
- The tests prove only the observed CPU behavior for the selected canary
  addresses under current Limine translations in QEMU. They do not prove
  all kernel pages, aliases, device DMA, ring-3 isolation, boot pointer
  provenance, or behavior under a new kernel-owned CR3 root.
- A page-fault error code provides architectural information about the
  faulting access, but does not independently authenticate every PTE or
  physical frame. Error-code bit definitions follow Intel SDM Vol. 3A
  (P, W/R, U/S, RSVD and I/D).
- The existing handler is **fatal and nonreturning**; neither experiment
  demonstrates recoverable faults or general-purpose exception delivery.

Primary reference: Intel® 64 and IA-32 Architectures Software Developer's
Manual, Vol. 3A, "Page-Fault Exceptions":
https://www.intel.com/content/www/us/en/developer/articles/technical/intel-sdm.html
