# M06l: Read-only CPU paging protection preflight

**Status:** Early-BSP capability and register observation under Limine's
existing four-level x86-64 address space. This stage does not create, modify
or switch page tables.

## Question

The QEMU observer in PRs #71–#73 confirms specific inherited translations
and records PTE R/W and NX bits. Those observations alone cannot establish
whether supervisor write-protection or execute-disable semantics are active,
nor whether a physical frame address contains bits beyond MAXPHYADDR.

This increment reads, without altering, the CPU features relevant to those
questions:

| Source | Sample and requirement |
| --- | --- |
| CPUID extended max leaf | Require `0x80000008` before using extended width fields |
| CPUID `0x80000001`.EDX[20] | NX capability must be advertised |
| CPUID `0x80000008`.EAX[7:0] | MAXPHYADDR must be in the bounded 32–52-bit range |
| CPUID `0x80000008`.EAX[15:8] | Advertised linear width must be exactly 48 or 57 bits; unsupported encodings are rejected |
| CR0[16] | WP must be enabled for supervisor write protection |
| EFER MSR `0xC0000080`[11] | NXE must be enabled |
| EFER[10] | Long mode must be active (LMA) |

The existing paging preflight already requires CR0.PG=1, CR4.PAE=1 and
CR4.LA57=0. Thus 48-bit canonical addresses remain in use even when
the CPU advertises a wider possible linear address mode.

The BSP reads EFER using `RDMSR` only after supported extended CPUID leaves
and NX capability are confirmed. It rejects unsupported configurations
with specific diagnostics rather than modifying control registers to
make the experiment pass. It also checks that the already observed,
4 KiB-aligned CR3 root candidate is nonzero and fits within MAXPHYADDR.
A successful checkpoint reports the numeric widths and a fixed
`PAGING: CPU protection controls active` marker.

## Tests and reproducibility

- Pure Rust host tests in `paging_mode.rs` cover supported states; missing
  CPUID leaves, invalid widths, NX unsupported/disabled, CR0.WP disabled,
  inactive long mode, over-width and misaligned root candidates.
- The existing serial-parser self-tests check valid ordering and reject
  duplicated/missing markers, impossible widths, inconsistent CR3-derived
  roots and noncanonical RSP.
- Both QEMU CPU-profile lanes already run `check_paging_snapshot.py`
  against their live boot serial logs. Existing ordinary ELF PTE walks,
  feature-only stack observations, ownership checks and fault injection
  remain separate.

**A green host test is not hardware evidence.** Use exact-head QEMU CI logs
to establish that these checks operated on both configured emulated CPUs.

## Explicit limits

The sampled controls increase confidence that the observed NX and R/W
fields have their expected architectural context. They do **not**
independently prove each effective page permission: the walker still
needs full reserved-bit validation (including the CPU physical width),
intermediate U/S and permission combination rules, NX semantics at
every level, and any additional protection features. An advertised
MAXPHYADDR is not evidence that any particular physical frame is owned
or available.

There is no verification yet of TLB or PCID state, distinct physical
ownership, direct-map aliases, DMA/IOMMU, GDT/TSS/IDT and exception
mapping lifetimes, or root-switch safety. Neither CR3 nor RSP is changed.

A missing extended CPUID leaf is intentionally rejected by this research
profile instead of using a fallback width not yet tested on Embers.

## Primary specification

- Intel 64 and IA-32 Architectures Software Developer's Manual, Volume 3A,
  sections covering CPUID MAXPHYADDR, NX, supervisor write protection and
  IA32_EFER:
  https://cdrdv2-public.intel.com/812386/253668-sdm-vol-3a.pdf
