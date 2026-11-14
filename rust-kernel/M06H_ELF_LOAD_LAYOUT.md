# M06h: ELF PT_LOAD inventory and Limine base correlation

**Status:** Read-only offline check of the linked kernel ELF plus numeric bootloader
metadata from QEMU. Not a page-table walk or physical memory ownership proof.

## Motivation

Embers boots through Limine. The linker sets an ELF virtual base at
`0xffffffff80000000` and three explicit PT_LOAD program headers:

| Program header | ELF flags | Intended contents |
| --- | --- | --- |
| 0 | RX | kernel code |
| 1 | R | Limine requests, constants and read-only data |
| 2 | RW | data and BSS, including experimental static stack storage |

ELF flags describe the requested load layout, **not** verified page permissions.
`p_paddr` is not treated as proof of the address used by the bootloader.

## New validation

The standard-library-only `scripts/check_elf_loads.py` rejects malformed
ELF64 headers, unexpected x86-64 type, wrong LOAD count/order/flags, W+X
segments, invalid alignments, inconsistent virtual/file offsets, truncated
file ranges, overlapping virtual segments, noncanonical virtual addresses
and an entry point outside the executable segment.

It can also correlate the ELF LOAD virtual offsets with the Limine
executable physical and virtual bases printed by the BSP. The arithmetic is:

```
offset = segment.p_vaddr - limine.executable_virtual_base
physical_candidate = limine.executable_physical_base + offset
```

This calculation is checked for consistency and overflow and is reported
as **Limine-derived PHYSICAL CANDIDATE**. It does not read any active PTE or
show how the CPU actually translates a virtual address. In particular,
neither an expected ELF RX flag nor a computed physical candidate proves
NX, writable permissions, alias freedom, pinned frames, or load survivability
under a new CR3 root.

## Reproduce and evidence

```sh
python3 scripts/check_elf_loads.py --self-test
make rust-check
python3 scripts/check_elf_loads.py \
  rust-kernel/target/x86_64-unknown-none/release/embers-rust-kernel \
  --serial build/embers-rust-serial-control-max.log
```

The Makefile executes the synthetic negative tests and structural ELF
validation on each `rust-check`. Both QEMU boot profiles correlate the
normal ELF with their serial-log Limine executable bases. Verify exact-head
workflow results before interpreting any observations. The optional
transition-stack smoke builds a different feature variant separately;
the measured normal ELF cannot be treated as that variant's precise layout.

## Remaining work

A future **read-only actual page-table walk** must validate current
translations and permissions for every live kernel segment, active stack,
dedicated transition storage, GDT/TSS/IDT, exception entry code and data.
That needs CPU physical-width and NX/WP checks, mapped readable access to
page-table frames, explicit rejection of nonpresent/reserved/huge page
cases until supported, and no trust in derived addresses alone. Only after
authenticated owned page-table construction and safe transition evidence
could a privileged CR3 switch be considered.
