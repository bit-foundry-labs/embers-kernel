#!/usr/bin/env python3
"""Validate read-only BSP paging diagnostics; not mapping/ownership proof."""
from pathlib import Path
import re
import sys

FIELDS = (
    "inherited CR3 raw",
    "root frame candidate",
    "observed BSP RSP",
    "MAXPHYADDR bits",
    "maximum linear bits",
)
ROOT_MASK = 0x000F_FFFF_FFFF_F000
READY = "READY: Embers Rust no_std entry reached."
PROTECTION_MARKER = "PAGING: CPU protection controls active (CR0.WP, EFER.NXE, EFER.LMA)"


def check(serial: str) -> None:
    lines = [line.strip() for line in serial.splitlines()]
    values = {}
    positions = []
    for field in FIELDS:
        pattern = re.compile(rf"PAGING: {re.escape(field)} = 0x([0-9a-f]{{16}})")
        matches = [(i, pattern.fullmatch(line)) for i, line in enumerate(lines)]
        matches = [(i, match) for i, match in matches if match is not None]
        if len(matches) != 1:
            raise ValueError(f"expected exactly one well-formed {field} line")
        index, match = matches[0]
        values[field] = int(match.group(1), 16)
        positions.append(index)
    ready = [i for i, line in enumerate(lines) if line == READY]
    if len(ready) != 1 or not (positions == sorted(positions) and positions[-1] < ready[0]):
        raise ValueError("paging snapshot must precede one normal READY checkpoint")
    markers = [i for i, line in enumerate(lines) if line == PROTECTION_MARKER]
    if len(markers) != 1 or not (positions[-1] < markers[0] < ready[0]):
        raise ValueError("missing, duplicated or misplaced protection-controls marker")
    raw = values["inherited CR3 raw"]
    root = values["root frame candidate"]
    rsp = values["observed BSP RSP"]
    if root != (raw & ROOT_MASK):
        raise ValueError("root candidate disagrees with masked CR3")
    physical_width = values["MAXPHYADDR bits"]
    linear_width = values["maximum linear bits"]
    if not (32 <= physical_width <= 52):
        raise ValueError("invalid CPU-reported MAXPHYADDR width")
    if not (48 <= linear_width <= 57):
        raise ValueError("invalid CPU-reported maximum linear width")
    if root == 0 or root % 4096 or root >> physical_width:
        raise ValueError("root candidate exceeds the reported physical width")
    upper, sign = rsp >> 48, (rsp >> 47) & 1
    if rsp == 0 or upper != (0xFFFF if sign else 0):
        raise ValueError("BSP RSP is zero or noncanonical for four-level paging")


def self_test() -> None:
    good = "\n".join((
        "PAGING: inherited CR3 raw = 0x0000000012345123",
        "PAGING: root frame candidate = 0x0000000012345000",
        "PAGING: observed BSP RSP = 0xffff800000102000",
        "PAGING: MAXPHYADDR bits = 0x0000000000000028",
        "PAGING: maximum linear bits = 0x0000000000000030",
        PROTECTION_MARKER,
        READY,
    ))
    check(good)
    for bad in (
        good.replace("0x0000000012345000", "0x0000000012346000"),
        good.replace("0xffff800000102000", "0x0000800000102000"),
        good.replace(READY, ""),
        good + "\nPAGING: inherited CR3 raw = 0x0000000012345123",
        good.replace("MAXPHYADDR bits = 0x0000000000000028",
                     "MAXPHYADDR bits = 0x000000000000001f"),
        good.replace("maximum linear bits = 0x0000000000000030",
                     "maximum linear bits = 0x000000000000002f"),
        good.replace(PROTECTION_MARKER, ""),
        good + "\n" + PROTECTION_MARKER,
        good.replace("0x0000000012345123", "0x0000010012345123")
            .replace("0x0000000012345000", "0x0000010012345000"),
    ):
        try:
            check(bad)
        except ValueError:
            continue
        raise AssertionError("invalid snapshot passed")
    print("PASS: paging and CPU protection snapshot parser rejects invalid observations")


if __name__ == "__main__":
    if len(sys.argv) == 2 and sys.argv[1] == "--self-test":
        self_test()
    elif len(sys.argv) == 2:
        check(Path(sys.argv[1]).read_text(encoding="utf-8"))
        print("PASS: CPU protection and CR3/RSP observations (not frame ownership or permission proof)")
    else:
        raise SystemExit("usage: check_paging_snapshot.py <serial-log> | --self-test")
