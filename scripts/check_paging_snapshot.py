#!/usr/bin/env python3
"""Validate read-only BSP paging diagnostics; not mapping/ownership proof."""
from pathlib import Path
import re
import sys

FIELDS = (
    "inherited CR3 raw",
    "root frame candidate",
    "observed BSP RSP",
)
ROOT_MASK = 0x000F_FFFF_FFFF_F000
READY = "READY: Embers Rust no_std entry reached."


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
    raw = values["inherited CR3 raw"]
    root = values["root frame candidate"]
    rsp = values["observed BSP RSP"]
    if root != (raw & ROOT_MASK):
        raise ValueError("root candidate disagrees with masked CR3")
    upper, sign = rsp >> 48, (rsp >> 47) & 1
    if rsp == 0 or upper != (0xFFFF if sign else 0):
        raise ValueError("BSP RSP is zero or noncanonical for four-level paging")


def self_test() -> None:
    good = "\n".join((
        "PAGING: inherited CR3 raw = 0x0000000012345123",
        "PAGING: root frame candidate = 0x0000000012345000",
        "PAGING: observed BSP RSP = 0xffff800000102000",
        READY,
    ))
    check(good)
    for bad in (
        good.replace("0x0000000012345000", "0x0000000012346000"),
        good.replace("0xffff800000102000", "0x0000800000102000"),
        good.replace(READY, ""),
        good + "\nPAGING: inherited CR3 raw = 0x0000000012345123",
    ):
        try:
            check(bad)
        except ValueError:
            continue
        raise AssertionError("invalid snapshot passed")
    print("PASS: paging snapshot parser accepts valid and rejects malformed inputs")


if __name__ == "__main__":
    if len(sys.argv) == 2 and sys.argv[1] == "--self-test":
        self_test()
    elif len(sys.argv) == 2:
        check(Path(sys.argv[1]).read_text(encoding="utf-8"))
        print("PASS: observed CR3 root candidate and canonical BSP RSP (not mapping proof)")
    else:
        raise SystemExit("usage: check_paging_snapshot.py <serial-log> | --self-test")
