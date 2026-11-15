# M06m: Live QMP MAXPHYADDR field checks

**Status:** Read-only, bounded host-side observation of Limine's inherited
four-level x86-64 page tables in QEMU. No new page tables, CR3 write, or
kernel-side physical-memory access.

## Why this increment is necessary

The earlier QMP walker masked raw entries with
`0x000f_ffff_ffff_f000` and compared derived physical addresses to ELF/Limine
candidates. That comparison can pass **without identifying an invalid bit
above the physical-address width**: the masked candidate might remain
correct even though the raw entry is architecturally invalid.

PR #74 made the BSP report CPU-advertised `MAXPHYADDR` alongside successful
CR0.WP/EFER.NXE/LMA preflight. PR #75 added a separate host-only Rust model
that rejects out-of-width frame-address fields. This change connects the
**actual paused QEMU PTE walk** to the validated guest boot observations.

## What is checked

1. Before pausing the disposable QEMU VM, the observer invokes the existing
   `check_paging_snapshot.py` validation on the boot serial log. That
   requires one consistent inherited CR3, canonical BSP RSP, physical width
   32–52, advertised linear width **48 or 57**, and the successful protection
   preflight marker. It returns the reported physical-address width.
2. The QMP observer still pauses the VM and verifies live CR3 matches the
   previously reported raw CR3. For the **masked CR3 root candidate** and
   every present PML4, PDPT, PD and PT 4 KiB entry, it checks that the
   physical frame-address field `bits 51:12` contains no set bits at
   or above `MAXPHYADDR`. It rejects before following or interpreting the
   frame number. Nonpresent and unsupported large-page entries still fail.
3. All prior ELF RX/R/RW segment pages and the separate feature-gated
   transition-stack pages remain covered in both QEMU CPU profiles, using
   their corresponding ELF and Limine metadata.
4. The live observer captures one valid code-address chain after QEMU is
   paused. For each of its four entries, it substitutes an invalid
   `1 << MAXPHYADDR` bit **in a host-side Python integer only** and requires
   the walker to reject that entry for the precise reserved-field reason.
   It also rejects an equivalently corrupted numeric CR3 root candidate.
   No guest memory, CPU register or page-table entry is written.
5. Synthetic negative fixtures reject invalid widths and out-of-width bits
   in all levels. If `MAXPHYADDR=52`, no reserved physical-address bit exists
   inside the 51:12 field; that specific injection test is skipped
   explicitly, not counted as having run.

## Security boundary and remaining verification

This is a **selected physical-address-field check**, not a full x86 PTE
validator. The monitor trusts the BSP-reported CPU width and successful
preflight marker; it does not independently read CPUID from QEMU or prove
the integrity of the boot log. Uninterpreted bits 62:52, vendor/feature
dependent reserved bits, PAT, protection keys, PCID, effective U/S/RW/NX
access, global aliases, physical ownership, mapping lifetimes, DMA/IOMMU
and fault behavior require separate experiments and architecture contracts.

The observer's present 4 KiB walk is still bounded to current QEMU RAM and
rejects large pages. All results remain observations of **inherited
Limine mappings**. Nothing here licenses kernel-owned table construction
or CR3 switching.

## Reproduce

```sh
python3 scripts/check_paging_snapshot.py --self-test
python3 scripts/check_active_pte_qmp.py --self-test
# The Rust boot CI matrix performs both ordinary and feature-only live QMP runs.
```

Verify the exact pull-request head's four required workflows and QEMU
profiles; host fixtures alone are not live CPU evidence.
