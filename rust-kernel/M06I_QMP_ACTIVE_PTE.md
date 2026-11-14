# M06i: Read-only QEMU physical page-table observation

**Status:** QEMU research experiment. No privileged register writes and no
guest page-table modification.

## Why an external monitor?

Embers has a validated ELF load layout and inherited CR3 observation. Reading
CR3 is not sufficient to know where the CPU translates a virtual address.
Dereferencing a guessed physical address through HHDM in ring 0 would add a new
unsafe access before mapping provenance has been established. Instead, this
increment uses QEMU's host-side QMP monitor to pause a separately booted VM,
read the current CR3 register and examine exactly one 64-bit entry at a time
via HMP `xp`, which reads guest **physical** memory.

## Bounded experiment

1. Boot the **ordinary** ISO in a dedicated QEMU instance with a local Unix
   QMP socket; wait for exactly one normal READY and the raw CR3 serial line.
2. Pause QEMU, check that it is not running and read its live CR3 with
   `info registers`. Fail if this differs from the earlier serial value.
3. Starting from that root, read four 8-byte entries for the first virtual
   byte of the **RX ELF PT_LOAD segment** (PML4, PDPT, PD, PT). Reject
   nonpresent entries, unsupported 2 MiB/1 GiB leaves, noncanonical addresses,
   invalid roots and any physical table-entry read outside the configured
   256 MiB emulator RAM.
4. Compare the observed 4 KiB leaf translation with the **candidate** derived
   from the ELF segment offset and Limine executable bases. Fail if different.
   Print observed table RW-chain and NX bits as uninterpreted evidence.
5. Terminate the separate QEMU process. The default boot, exceptions,
   memory tests and feature-only stack smoke remain unchanged.

`python3 scripts/check_active_pte_qmp.py --self-test` tests synthetic,
host-only positive/negative page walks and strict monitor-output parsing.
The two QEMU boot matrix profiles run the live observation on the exact PR
head. Nothing in the kernel uses the QMP script or has QMP dependencies.

## Safety and research limits

- One positive QEMU translation shows the inspected 4 KiB mapping exists at
  the observation point **in that emulator run**. It does not prove all
  kernel segments, transition stack, descriptor tables or exceptions survive
  a new address-space root.
- This stage intentionally rejects large pages rather than interpreting
  1 GiB / 2 MiB entries or assuming their reserved bits are valid.
- The walker does not establish CPUID MAXPHYADDR, EFER.NXE, CR0.WP, reserved
  bits, effective execute protection, physical frame exclusivity, alias
  freedom or IOMMU/DMA isolation. RW and NX fields are observations only.
- The probe uses QMP `stop`, `query-status` and HMP read commands; it
  never modifies a guest PTE, kernel RSP or CR3.
- If QMP/HMP output differs from the expected limited grammar, the test
  must fail; adapting the parser requires a separate review.

The next bounded increment is a wider **read-only** coverage and permission
inventory with physical-width/large-page rules and verified lifetime
assumptions. A CR3 write remains prohibited until independent transition
safety criteria are met.

## External mechanism references

- QEMU QMP specification: https://www.qemu.org/docs/master/interop/qmp-spec.html
- QEMU QMP reference: https://www.qemu.org/docs/master/interop/qemu-qmp-ref.html
- QEMU HMP command source (`xp` physical dump):
  https://qemu.googlesource.com/qemu/+/c8aaa24537cb87ebe5a2a6a1ea9cfff337e98bb4/hmp-commands.hx
