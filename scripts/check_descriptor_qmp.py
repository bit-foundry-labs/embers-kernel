#!/usr/bin/env python3
"""Observe installed x86-64 descriptor and IST1 ranges under inherited QEMU CR3.

Opt-in feature only. Host QMP physical reads are read-only; this is not
kernel-owned page-table, memory-lifetime, or DMA isolation evidence.
"""

import argparse
import re
import subprocess
import tempfile
import time
from pathlib import Path

from check_active_pte_qmp import (
    FRAME_MASK,
    PAGE,
    Qmp,
    READY,
    canonical_48,
    check,
    register_cr3,
    segment_probe_points,
    serial_cr3,
    walk_4k_only,
    xp_word,
)
from check_elf_loads import derive_candidates, parse_loads
from check_paging_snapshot import check as validate_paging_snapshot

SIZES = {"GDT": 40, "IDT": 4096, "TSS": 104, "IST1": 16 * 1024}
ACCEPTED = "DESCRIPTOR: register state validated"
HEX = r"0x([0-9a-f]{16})"


def descriptor_ranges(serial: str, loads: list[dict[str, int]]) -> dict[str, tuple[int, int]]:
    """Require a complete, unambiguous report confined to this ELF's RW LOAD."""
    check(len(loads) == 3 and loads[2]["flags"] == 6, "expected RX/R/RW ELF")
    lines = [line.strip() for line in serial.splitlines()]
    check(lines.count(ACCEPTED) == 1 and lines.count(READY) == 1
          and not any("ERROR:" in line for line in lines),
          "descriptor acceptance or READY missing, duplicated or contradicted")
    check(sum(line.startswith("DESCRIPTOR:") for line in lines) == 10,
          "incomplete, extra or duplicated descriptor metadata")
    tr = re.findall(r"^DESCRIPTOR: TR selector = " + HEX + r"$", serial, re.M)
    check(tr == ["0000000000000018"], "TR selector is not the installed TSS selector")
    rw = loads[2]
    result = {}
    for name, expected_length in SIZES.items():
        bases = re.findall(r"^DESCRIPTOR: " + name + r" base = " + HEX + r"$",
                           serial, re.M)
        lengths = re.findall(r"^DESCRIPTOR: " + name + r" bytes = " + HEX + r"$",
                             serial, re.M)
        check(len(bases) == len(lengths) == 1,
              f"missing or duplicated {name} base/length")
        base, length = int(bases[0], 16), int(lengths[0], 16)
        end = base + length
        check(base != 0 and length == expected_length and end <= 1 << 64
              and canonical_48(base) and canonical_48(end - 1),
              f"invalid or noncanonical {name} range")
        check(rw["vaddr"] <= base and end <= rw["vaddr"] + rw["memsz"],
              f"{name} range not completely in ELF RW LOAD")
        result[name] = (base, length)
    return result


def inspect_ranges(ranges: dict[str, tuple[int, int]],
                   loads: list[dict[str, int]], candidates: list[tuple[int, int]],
                   root: int, read_word, physical_bits: int) -> dict[str, int]:
    """Check every page intersecting every descriptor/IST1 extent."""
    check(set(ranges) == set(SIZES) and len(loads) == len(candidates) == 3,
          "partial descriptor inventory")
    rw = loads[2]
    physical_base, physical_end = candidates[2]
    check(rw["flags"] == 6 and physical_end - physical_base == rw["memsz"],
          "invalid RW physical candidate")
    counts = {}
    for name, (base, length) in ranges.items():
        check(length == SIZES[name] and rw["vaddr"] <= base
              and base + length <= rw["vaddr"] + rw["memsz"],
              f"invalid {name} extent")
        points = segment_probe_points({"vaddr": base, "memsz": length})
        for va in points:
            physical, chain = walk_4k_only(root, va, read_word, physical_bits)
            check(physical == physical_base + va - rw["vaddr"],
                  f"{name} VA 0x{va:016x} differs from reported ELF backing")
            check(all(bool(entry & 2) for entry in chain),
                  f"{name} has a read-only ancestor or leaf PTE")
        counts[name] = len({va // PAGE for va in points})
    return counts


def run(cpu: str, iso: Path, elf: Path) -> None:
    check(cpu in ("max", "qemu64"), "unexpected QEMU CPU profile")
    loads = parse_loads(elf.read_bytes())
    with tempfile.TemporaryDirectory(prefix="embers-descriptors-") as temp:
        root_path = Path(temp)
        serial_path = root_path / "serial.log"
        qmp_path = root_path / "qmp.sock"
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
                raise ValueError("feature guest did not report READY within 15 seconds")
            observed_cr3 = serial_cr3(serial)
            physical_bits = validate_paging_snapshot(serial)
            ranges = descriptor_ranges(serial, loads)
            candidates = derive_candidates(loads, serial)

            monitor = Qmp(qmp_path)
            monitor.execute("stop")
            check(monitor.execute("query-status").get("running") is False,
                  "QEMU did not stop before inspecting memory")
            live_cr3 = register_cr3(monitor.hmp("info registers"))
            check(live_cr3 == observed_cr3,
                  "live CR3 differs from feature guest serial evidence")
            entries = {}

            def read_word(address: int) -> int:
                if address not in entries:
                    entries[address] = xp_word(monitor.hmp(f"xp /1gx 0x{address:x}"))
                return entries[address]

            checked = inspect_ranges(
                ranges, loads, candidates, live_cr3 & FRAME_MASK,
                read_word, physical_bits)
            print(f"PASS: {cpu} installed descriptors and IST1 under inherited CR3")
            for name, pages in checked.items():
                print(f"  {name}: {pages} intersected pages have expected ELF backing and RW PTEs")
            print("  QMP observation only; not a kernel-owned CR3 or frame authority")
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
    va = 0xFFFF_FFFF_8000_0000
    rw = va + 4 * PAGE
    loads = [
        {"vaddr": va, "memsz": PAGE, "flags": 5},
        {"vaddr": va + PAGE, "memsz": PAGE, "flags": 4},
        {"vaddr": rw, "memsz": 10 * PAGE, "flags": 6},
    ]
    physical_rw = 0x100000
    candidates = [(0x200000, 0x201000), (0x201000, 0x202000),
                  (physical_rw, physical_rw + 10 * PAGE)]
    bases = {
        "GDT": rw + 0x40,
        "IDT": rw + PAGE + 0x80,
        "TSS": rw + 0x140,
        "IST1": rw + 3 * PAGE + 0x10,
    }
    lines = []
    for name, length in SIZES.items():
        lines.append(f"DESCRIPTOR: {name} base = 0x{bases[name]:016x}")
        lines.append(f"DESCRIPTOR: {name} bytes = 0x{length:016x}")
    lines += ["DESCRIPTOR: TR selector = 0x0000000000000018", ACCEPTED, READY]
    valid = "\n".join(lines) + "\n"
    ranges = descriptor_ranges(valid, loads)

    root = 0x1000
    indices = [(rw >> shift) & 511 for shift in (39, 30, 21, 12)]
    cells = {
        root + indices[0] * 8: 0x2003,
        0x2000 + indices[1] * 8: 0x3003,
        0x3000 + indices[2] * 8: 0x4003,
    }
    for page in range(10):
        leaf = 0x4000 + ((rw + page * PAGE) >> 12 & 511) * 8
        cells[leaf] = (physical_rw + page * PAGE) | 3
    assert inspect_ranges(ranges, loads, candidates, root, cells.__getitem__, 40) == {
        "GDT": 1, "IDT": 2, "TSS": 1, "IST1": 5,
    }

    def rejected(action, label: str) -> None:
        try:
            action()
        except (ValueError, KeyError):
            return
        raise AssertionError(f"invalid descriptor evidence accepted: {label}")

    rejected(lambda: descriptor_ranges("", loads), "missing data")
    rejected(lambda: descriptor_ranges(valid + valid, loads), "duplicate report")
    rejected(lambda: descriptor_ranges(valid.replace(
        "0x0000000000000018", "0x0000000000000000"), loads), "wrong TR selector")
    rejected(lambda: descriptor_ranges(valid.replace(
        f"0x{bases['GDT']:016x}", "0x0000800000000000"), loads), "noncanonical GDT")
    rejected(lambda: descriptor_ranges(valid.replace(
        "DESCRIPTOR: TSS bytes = 0x0000000000000068",
        "DESCRIPTOR: TSS bytes = 0x0000000000000067"), loads), "incorrect TSS size")
    rejected(lambda: descriptor_ranges(valid, [*loads[:2],
        {"vaddr": rw, "memsz": 2 * PAGE, "flags": 6}]), "truncated RW ELF")

    corrupted = dict(cells)
    corrupted[0x4000 + ((rw + 4 * PAGE) >> 12 & 511) * 8] = 0x300003
    rejected(lambda: inspect_ranges(
        ranges, loads, candidates, root, corrupted.__getitem__, 40), "physical mismatch")
    absent = dict(cells)
    absent[0x4000 + ((rw + 4 * PAGE) >> 12 & 511) * 8] &= ~1
    rejected(lambda: inspect_ranges(
        ranges, loads, candidates, root, absent.__getitem__, 40), "nonpresent page")
    readonly = dict(cells)
    readonly[0x4000 + ((rw + 4 * PAGE) >> 12 & 511) * 8] &= ~2
    rejected(lambda: inspect_ranges(
        ranges, loads, candidates, root, readonly.__getitem__, 40), "read-only page")
    print("PASS: descriptor range and PTE negative fixtures (host-only)")


if __name__ == "__main__":
    args = argparse.ArgumentParser(description=__doc__)
    args.add_argument("--self-test", action="store_true")
    args.add_argument("--cpu", choices=("qemu64", "max"))
    args.add_argument("--iso", type=Path)
    args.add_argument("--elf", type=Path)
    parsed = args.parse_args()
    try:
        if parsed.self_test:
            check(parsed.cpu is None and parsed.iso is None and parsed.elf is None,
                  "self-test takes no QEMU arguments")
            self_test()
        else:
            check(parsed.cpu and parsed.iso and parsed.elf,
                  "live mode needs --cpu, --iso and --elf")
            run(parsed.cpu, parsed.iso, parsed.elf)
    except (OSError, ValueError, KeyError, TimeoutError) as error:
        raise SystemExit(f"FAIL: {error}") from error
