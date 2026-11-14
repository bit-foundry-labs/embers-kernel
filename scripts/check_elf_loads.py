#!/usr/bin/env python3
"""Audit ELF64 PT_LOAD layout and derived Limine load-address candidates.

This is a structural/arithmetical check. It does not inspect the CPU page
hierarchy, authenticate physical memory ownership, or prove any mapping rights.
"""

import argparse
import re
import struct
from pathlib import Path

LINK_VIRTUAL_BASE = 0xFFFF_FFFF_8000_0000
PAGE_BYTES = 4096
UINT64_LIMIT = 1 << 64
SEGMENT_FLAGS = (5, 4, 6)  # RX, R, RW, as explicitly defined by linker.ld
PHDR = struct.Struct("<IIQQQQQQ")


def require(condition: bool, reason: str) -> None:
    if not condition:
        raise ValueError(reason)


def canonical_48(address: int) -> bool:
    if not 0 <= address < UINT64_LIMIT:
        return False
    upper = address >> 48
    return upper == (0xFFFF if (address >> 47) & 1 else 0)


def parse_loads(data: bytes) -> list[dict[str, int]]:
    require(len(data) >= 64, "truncated ELF64 header")
    require(data[:6] == b"\x7fELF\x02\x01", "expected little-endian ELF64")
    etype, machine, version = struct.unpack_from("<HHI", data, 16)
    entry, phoff = struct.unpack_from("<QQ", data, 24)
    ehsize, phentsize, phnum = struct.unpack_from("<HHH", data, 52)
    require((etype, machine, version) == (2, 62, 1), "expected x86-64 ET_EXEC")
    require(ehsize == 64 and phentsize == PHDR.size and 0 < phnum <= 64,
            "unexpected ELF64 program-header dimensions")
    require(phoff >= 64 and phoff + phnum * phentsize <= len(data),
            "truncated program-header table")
    loads = []
    for i in range(phnum):
        typ, flags, offset, vaddr, _paddr, filesz, memsz, align = PHDR.unpack_from(
            data, phoff + i * phentsize
        )
        if typ != 1:
            continue
        require(filesz <= memsz and memsz != 0, "invalid PT_LOAD lengths")
        require(offset + filesz <= len(data), "PT_LOAD file extent out of bounds")
        require(align >= PAGE_BYTES and (align & (align - 1)) == 0,
                "PT_LOAD alignment must be page-aligned power of two")
        require(vaddr % align == offset % align, "ELF PT_LOAD offset/VA congruence failed")
        require(vaddr >= LINK_VIRTUAL_BASE and vaddr + memsz <= UINT64_LIMIT,
                "PT_LOAD lies outside linked higher-half address range")
        require(canonical_48(vaddr) and canonical_48(vaddr + memsz - 1),
                "noncanonical PT_LOAD virtual range")
        require(not (flags & 2 and flags & 1), "ELF segment is writable and executable")
        loads.append(dict(flags=flags, offset=offset, vaddr=vaddr,
                          filesz=filesz, memsz=memsz, align=align))
    require(len(loads) == len(SEGMENT_FLAGS), "expected exactly three PT_LOAD segments")
    require(tuple(x["flags"] for x in loads) == SEGMENT_FLAGS,
            "expected RX/R/RW PT_LOAD flags in linker order")
    for earlier, later in zip(loads, loads[1:]):
        require(earlier["vaddr"] + earlier["memsz"] <= later["vaddr"],
                "PT_LOAD virtual ranges overlap or are out of order")
    require(loads[0]["vaddr"] <= entry < loads[0]["vaddr"] + loads[0]["memsz"],
            "ELF entry point not inside executable segment")
    return loads


def serial_base(serial: str, kind: str) -> int:
    pattern = re.compile(rf"ADDRESS: executable {kind} base = 0x([0-9a-f]{{16}})")
    matches = [pattern.fullmatch(line.strip()) for line in serial.splitlines()]
    values = [int(m.group(1), 16) for m in matches if m is not None]
    require(len(values) == 1, f"expected exactly one executable {kind} base")
    return values[0]


def derive_candidates(loads: list[dict[str, int]], serial: str) -> list[tuple[int, int]]:
    physical_base = serial_base(serial, "physical")
    virtual_base = serial_base(serial, "virtual")
    require(virtual_base == LINK_VIRTUAL_BASE, "Limine virtual base disagrees with linker")
    require(physical_base != 0 and physical_base % PAGE_BYTES == 0,
            "Limine physical base is null or unaligned")
    candidates = []
    for segment in loads:
        offset = segment["vaddr"] - virtual_base
        start = physical_base + offset
        end = start + segment["memsz"]
        require(end <= UINT64_LIMIT, "derived physical-address range overflows u64")
        candidates.append((start, end))
    return candidates


def self_test() -> None:
    def make_elf(flags: tuple[int, int, int] = SEGMENT_FLAGS, overlap=False) -> bytes:
        data = bytearray(0x4000)
        data[:6] = b"\x7fELF\x02\x01"
        struct.pack_into("<HHI", data, 16, 2, 62, 1)
        struct.pack_into("<QQ", data, 24, LINK_VIRTUAL_BASE + 0x10, 64)
        struct.pack_into("<HHH", data, 52, 64, PHDR.size, 3)
        for i, flag in enumerate(flags):
            offset = (i + 1) * PAGE_BYTES
            vaddr = LINK_VIRTUAL_BASE + (i * PAGE_BYTES)
            if overlap and i == 1:
                vaddr = LINK_VIRTUAL_BASE
            PHDR.pack_into(data, 64 + i * PHDR.size, 1, flag, offset, vaddr,
                           0, 32, 0x100, PAGE_BYTES)
        return bytes(data)

    serial = ("ADDRESS: executable physical base = 0x000000000ff44000\n"
              "ADDRESS: executable virtual base = 0xffffffff80000000\n")
    good = make_elf()
    loads = parse_loads(good)
    assert [x["flags"] for x in loads] == [5, 4, 6]
    assert derive_candidates(loads, serial) == [
        (0x0ff44000, 0x0ff44100),
        (0x0ff45000, 0x0ff45100),
        (0x0ff46000, 0x0ff46100),
    ]

    def rejected(callback, label: str) -> None:
        try:
            callback()
        except ValueError:
            return
        raise AssertionError("expected rejection: " + label)

    rejected(lambda: parse_loads(good[:90]), "truncated headers")
    rejected(lambda: parse_loads(make_elf((7, 4, 6))), "W+X")
    rejected(lambda: parse_loads(make_elf((5, 6, 4))), "wrong flags")
    rejected(lambda: parse_loads(make_elf(overlap=True)), "overlapping VA")
    rejected(lambda: derive_candidates(loads, serial + serial), "duplicate metadata")
    rejected(lambda: derive_candidates(loads, serial.replace("0xffffffff80000000", "0xffff800000000000")), "wrong virtual base")
    rejected(lambda: derive_candidates(loads, serial.replace("0x000000000ff44000", "0xfffffffffffff000")), "physical overflow")
    rejected(lambda: derive_candidates(loads, serial.replace("0x000000000ff44000", "0x000000000ff44001")), "unaligned physical base")
    print("PASS: ELF PT_LOAD validator and synthetic Limine provenance rejection cases")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("elf", nargs="?", type=Path)
    parser.add_argument("--serial", type=Path, help="serial log with Limine executable bases")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        require(args.elf is None and args.serial is None, "self-test takes no files")
        self_test()
        return
    require(args.elf is not None, "ELF file is required")
    loads = parse_loads(args.elf.read_bytes())
    candidates = derive_candidates(loads, args.serial.read_text()) if args.serial else None
    for i, seg in enumerate(loads):
        print(f"ELF LOAD {i}: flags={SEGMENT_FLAGS[i]} VA=0x{seg['vaddr']:016x} "
              f"mem_bytes=0x{seg['memsz']:x} file_bytes=0x{seg['filesz']:x}")
        if candidates:
            start, end = candidates[i]
            print(f"  Limine-derived PHYSICAL CANDIDATE: 0x{start:016x}..0x{end:016x}")
    print("PASS: ELF load structure and numeric provenance checked (NO PTE mapping proof)")


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError) as error:
        raise SystemExit(f"FAIL: {error}") from error
