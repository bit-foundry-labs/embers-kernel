# M06b: Active paging-mode preflight

**Status:** Read-only CPU state preflight and BSP context sampling. Not kernel-owned paging.

At the single BSP early-boot checkpoint, Embers reads CR0 and CR4 at CPL0.
The safe validator requires CR0.PG = 1, CR4.PAE = 1, and CR4.LA57 = 0.
It emits `PAGING: active four-level mode validated` only after validation;
otherwise it emits a specific `ERROR:` message and halts.

The mode validator's host tests exercise accepted bit patterns, each rejected
prerequisite, and deterministic multi-error behavior. Additional host tests
check CR3 root masking and 48-bit stack-pointer canonicality. A serial-parser
self-test covers inconsistent, duplicate and missing observations. QEMU boot
requires the preflight checkpoint plus one snapshot before the READY marker.

## Boundaries

- The x86-64 Limine long-mode handoff is trusted. This increment does not
  inspect EFER.LMA, CPUID capabilities, actual table hierarchy, permissions,
  memory ownership, or page-table entry content. It reads CR3 as a value and
  masks its low control/PCID bits to report a **root frame candidate**.
- CR0 and CR4 are sampled on the BSP; there is no AP consistency check.
- The checks never modify control registers, page tables, or virtual mappings.
- The RSP value is observed **inside the sampling function**, after boot began.
  It is not the Limine entry RSP, a proven stack extent, or evidence that this
  stack would remain mapped under a new CR3.
- QEMU checks assert well-formed single-occurrence numeric observations and a
  48-bit canonical RSP. They do not walk the existing mappings, check stack
  permissions, verify page-table frame ownership, or discover physical width.
- A successful result is necessary for the current 48-bit canonical-address
  assumptions, but **not sufficient** for a safe transition to owned tables.
- Host tests do not exercise privileged register reads; QEMU CI must provide
  that evidence before merging.

## Reproduce

```sh
make rust-paging-mode-test
make rust-iso
make rust-run
```

Next: establish the physical backing, permissions and lifetime of a deliberately
allocated transition stack and of each zeroed page-table frame; inspect the
required live code/data/IDT/TSS mappings before any CR3 write. Only then can
a gated transition use positive and negative QEMU evidence. Observed registers
alone cannot authorize a page-table switch.
