#!/usr/bin/env python3
"""Inspect the existing two boot frame reservations through inherited HHDM.

Only the host reads QEMU physical memory. No guest HHDM dereference, page-table
write, frame clearing, CR3 change, or physical ownership claim is made.
Supports present 4 KiB, 2 MiB, and 1 GiB x86-64 leaves with bounded checks.
"""

import argparse
import re
import subprocess
import tempfile
import time
from pathlib import Path

from check_active_pte_qmp import (
    FRAME_MASK, PAGE, PHYSICAL_RAM, Qmp, READY, canonical_48,
    check, register_cr3, serial_cr3, xp_word,
)
from check_paging_snapshot import check as validate_paging_snapshot

HHDM_LINE = re.compile(r"^ADDRESS: HHDM offset = 0x([0-9a-f]{16})$", re.M)
FRAME_LINES = [
    re.compile(rf"^MEMORY: frame claim {index} = 0x([0-9a-f]{{16}})$", re.M)
    for index in (1, 2)
]
ADDRESS_OK = "ADDRESS: Limine HHDM and executable bases validated"
MAP_OK = "MEMORY: validated Limine map for physical frame accounting"
PRESENT = 1
WRITABLE = 1 << 1
USER = 1 << 2
HUGE = 1 << 7


def reservation_samples(serial: str) -> list[tuple[int, int]]:
    """Parse exactly two disjoint, adjacent, page-aligned frame observations."""
    lines = serial.splitlines()
    check(lines.count(READY) == 1 and lines.count(ADDRESS_OK) == 1
          and lines.count(MAP_OK) == 1
          and not any(line.startswith("ERROR:") for line in lines),
          "missing, duplicated, or contradictory boot markers")
    matches = HHDM_LINE.findall(serial)
    check(len(matches) == 1, "expected exactly one HHDM offset")
    hhdm = int(matches[0], 16)
    check(hhdm != 0 and canonical_48(hhdm), "invalid HHDM offset")
    physical = []
    for pattern in FRAME_LINES:
        values = pattern.findall(serial)
        check(len(values) == 1, "missing or duplicated frame reservation")
        physical.append(int(values[0], 16))
    check(physical[0] >= 0x100000 and physical[0] % PAGE == 0
          and physical[1] == physical[0] + PAGE
          and physical[1] < PHYSICAL_RAM,
          "invalid, nonadjacent or out-of-RAM frame reservations")
    result = []
    for frame in physical:
        va = hhdm + frame
        check(va < 1 << 64 and va + PAGE <= 1 << 64
              and canonical_48(va) and canonical_48(va + PAGE - 1),
              "HHDM frame extent overflows or is noncanonical")
        result.append((frame, va))
    return result


def walk_hhdm(root: int, va: int, read_word,
              physical_bits: int) -> tuple[int, int]:
    """Walk one 48-bit VA, checking required fields of 4K/2M/1G leaves.

    This only checks selected present, RW, U/S, PS and MAXPHYADDR fields.
    NX, PAT, cache attributes, protection keys, effective rights, all
    reserved bits, frame identity and lifetime are not fully validated.
    """
    check(canonical_48(va), "noncanonical HHDM virtual address")
    check(32 <= physical_bits <= 52, "invalid MAXPHYADDR")
    check(root != 0 and root % PAGE == 0 and (root & FRAME_MASK) >> physical_bits == 0,
          "invalid inherited CR3 root candidate")
    table = root
    for shift in (39, 30, 21, 12):
        index = (va >> shift) & 511
        address = table + index * 8
        check(address + 8 <= PHYSICAL_RAM, "PTE address outside observed QEMU RAM")
        raw = read_word(address)
        check(0 <= raw < (1 << 64), "invalid 64-bit PTE value")
        check(raw & PRESENT != 0, "nonpresent HHDM PTE")
        check(raw & WRITABLE != 0, "read-only ancestor or HHDM leaf")
        check(raw & USER == 0, "user-accessible HHDM path")
        field = raw & FRAME_MASK
        check(field >> physical_bits == 0, "PTE exceeds MAXPHYADDR")
        huge = raw & HUGE != 0
        check(not (huge and shift == 39), "unsupported PML4 large-page bit")
        if huge and shift in (30, 21):
            size = 1 << shift
            # Lower address-field bits in a huge leaf must be zero, except
            # bit 12 which has the architectural PAT role at these levels.
            check(field & (size - 1) & ~(1 << 12) == 0,
                  "reserved low physical-address bits in huge PTE")
            base = field & ~(size - 1)
            physical = base + (va & (size - 1))
            check(physical < PHYSICAL_RAM, "HHDM leaf outside configured RAM")
            return physical, size
        if shift == 12:
            physical = field + (va & (PAGE - 1))
            check(physical < PHYSICAL_RAM, "HHDM leaf outside configured RAM")
            return physical, PAGE
        check(not huge, "unsupported huge-table structure")
        table = field
        check(table < PHYSICAL_RAM, "next page table outside observed QEMU RAM")
    raise ValueError("incomplete HHDM page-table walk")


def inspect_reservations(samples: list[tuple[int, int]], root: int,
                         read_word, physical_bits: int) -> list[tuple[int, int]]:
    """Check first/last byte of both reserved frames against physical identity."""
    check(len(samples) == 2 and samples[1][0] == samples[0][0] + PAGE,
          "missing or nonadjacent reserved frames")
    results = []
    for physical, va in samples:
        sizes = set()
        for offset in (0, PAGE - 1):
            observed, leaf_size = walk_hhdm(root, va + offset, read_word, physical_bits)
            check(observed == physical + offset,
                  "HHDM translation differs from reported reserved frame")
            sizes.add(leaf_size)
        check(len(sizes) == 1, "inconsistent leaf size across one frame")
        results.append((physical, next(iter(sizes))))
    return results


def run(cpu: str, iso: Path) -> None:
    check(cpu in ("max", "qemu64"), "unsupported QEMU CPU model")
    with tempfile.TemporaryDirectory(prefix="embers-hhdm-") as temp:
        directory = Path(temp)
        serial_path, qmp_path = directory / "serial.log", directory / "qmp.sock"
        process = subprocess.Popen([
            "qemu-system-x86_64", "-cpu", cpu, "-m", "256M",
            "-cdrom", str(iso.resolve()), "-display", "none",
            "-serial", f"file:{serial_path}", "-monitor", "none",
            "-qmp", f"unix:{qmp_path},server=on,wait=off",
            "-no-reboot", "-no-shutdown",
        ], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        monitor = None
        try:
            deadline = time.monotonic() + 15
            serial = ""
            while time.monotonic() < deadline:
                check(process.poll() is None, "QEMU exited before READY")
                if serial_path.exists():
                    serial = serial_path.read_text(errors="replace")
                    if READY in serial:
                        break
                time.sleep(0.05)
            else:
                raise ValueError("QEMU did not report READY within 15 seconds")
            samples = reservation_samples(serial)
            serial_root = serial_cr3(serial)
            physical_bits = validate_paging_snapshot(serial)
            monitor = Qmp(qmp_path)
            monitor.execute("stop")
            check(monitor.execute("query-status").get("running") is False,
                  "QEMU not paused")
            current = register_cr3(monitor.hmp("info registers"))
            check(current == serial_root, "inherited CR3 changed before QMP read")
            words = {}

            def read_word(address: int) -> int:
                if address not in words:
                    words[address] = xp_word(monitor.hmp(f"xp /1gx 0x{address:x}"))
                return words[address]

            result = inspect_reservations(
                samples, current & FRAME_MASK, read_word, physical_bits)
            print(f"PASS: {cpu} reserved-frame HHDM address translations observed")
            print(f"  inherited CR3=0x{current:016x}; MAXPHYADDR={physical_bits}")
            for frame, size in result:
                print(f"  frame=0x{frame:016x}; leaf={size // 1024} KiB; RW, supervisor PTE path")
            print("  QEMU read-only observation, NOT physical ownership or zeroing")
        finally:
            if monitor is not None:
                monitor.close()
            process.terminate()
            try:
                process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=3)


def self_test() -> None:
    hhdm = 0xffff_8000_0000_0000
    frame = 0x100000
    va = hhdm + frame
    sample_log = "\n".join((
        "ADDRESS: Limine HHDM and executable bases validated",
        f"ADDRESS: HHDM offset = 0x{hhdm:016x}",
        MAP_OK,
        f"MEMORY: frame claim 1 = 0x{frame:016x}",
        f"MEMORY: frame claim 2 = 0x{frame + PAGE:016x}",
        READY,
    )) + "\n"
    samples = reservation_samples(sample_log)
    assert samples == [(frame, va), (frame + PAGE, va + PAGE)]

    root = 0x1000
    i4, i3, i2, i1 = [(va >> shift) & 511 for shift in (39, 30, 21, 12)]
    pml4 = root + i4 * 8
    pdpt = 0x2000 + i3 * 8
    pd = 0x3000 + i2 * 8
    pt = 0x4000 + i1 * 8
    cells = {pml4: 0x2003, pdpt: 0x3003, pd: 0x4003,
             pt: frame | 3, pt + 8: (frame + PAGE) | 3}
    assert inspect_reservations(samples, root, cells.__getitem__, 40) == [
        (frame, PAGE), (frame + PAGE, PAGE)
    ]
    two_meg = dict(cells)
    two_meg[pd] = 0x83  # 2 MiB superpage, base zero
    assert inspect_reservations(samples, root, two_meg.__getitem__, 40) == [
        (frame, 1 << 21), (frame + PAGE, 1 << 21)
    ]
    one_gig = dict(cells)
    one_gig[pdpt] = 0x83  # 1 GiB superpage, base zero
    assert inspect_reservations(samples, root, one_gig.__getitem__, 40) == [
        (frame, 1 << 30), (frame + PAGE, 1 << 30)
    ]

    def rejected(action, why):
        try:
            action()
        except (ValueError, KeyError):
            return
        raise AssertionError(f"accepted bad HHDM evidence: {why}")

    rejected(lambda: reservation_samples(""), "empty metadata")
    rejected(lambda: reservation_samples(sample_log * 2), "duplicate metadata")
    rejected(lambda: reservation_samples(
        sample_log.replace(f"0x{frame + PAGE:016x}", f"0x{frame + 2 * PAGE:016x}")),
        "nonadjacent reservations")
    rejected(lambda: reservation_samples(
        sample_log.replace(f"0x{frame:016x}", f"0x{frame + 1:016x}")),
        "unaligned frame")
    rejected(lambda: reservation_samples(
        sample_log.replace(f"0x{hhdm:016x}", "0x0000800000000000")),
        "noncanonical HHDM offset")
    rejected(lambda: reservation_samples(
        sample_log.replace(f"0x{hhdm:016x}", "0xfffffffffffff000")),
        "HHDM offset overflow")
    rejected(lambda: reservation_samples(
        sample_log.replace(MAP_OK, "ERROR: corrupt handoff")), "error marker")
    rejected(lambda: inspect_reservations(
        [(frame, va)], root, cells.__getitem__, 40), "incomplete reservation set")
    bad = dict(cells)
    bad[pt] = (frame + 2 * PAGE) | 3
    rejected(lambda: inspect_reservations(
        samples, root, bad.__getitem__, 40), "incorrect physical backing")
    for bad_cells, label in [
        ({**cells, pt: cells[pt] & ~1}, "nonpresent leaf"),
        ({**cells, pd: cells[pd] & ~2}, "read-only ancestor"),
        ({**cells, pt: cells[pt] | 4}, "user-accessible leaf"),
        ({**cells, pml4: cells[pml4] | 0x80}, "large PML4"),
        ({**cells, pd: 0x83 | (1 << 13)}, "2 MiB reserved field"),
        ({**cells, pdpt: 0x83 | (1 << 20)}, "1 GiB reserved field"),
        ({**cells, pt: cells[pt] | (1 << 40)}, "MAXPHYADDR"),
    ]:
        rejected(lambda c=bad_cells: inspect_reservations(
            samples, root, c.__getitem__, 40), label)
    for width in (0, 31, 53):
        rejected(lambda width=width: inspect_reservations(
            samples, root, cells.__getitem__, width), "invalid physical width")
    print("PASS: 4 KiB, 2 MiB and 1 GiB HHDM walks and negative fixtures (host-only)")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--cpu", choices=("qemu64", "max"))
    parser.add_argument("--iso", type=Path)
    args = parser.parse_args()
    try:
        if args.self_test:
            check(args.cpu is None and args.iso is None, "self-test takes no QEMU arguments")
            self_test()
        else:
            check(args.cpu is not None and args.iso is not None,
                  "live mode requires --cpu and --iso")
            run(args.cpu, args.iso)
    except (OSError, ValueError, KeyError, TimeoutError) as error:
        raise SystemExit(f"FAIL: {error}") from error
