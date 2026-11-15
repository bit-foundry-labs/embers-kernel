#!/usr/bin/env python3
"""Validate fatal QEMU paging-permission faults, never just a #PF marker.

Feature-only tests require architecturally specific CR2 and error-code bits.
Normal kernel boot and inherited page tables remain unchanged.
"""

import argparse
import re
from pathlib import Path

from check_elf_loads import parse_loads

READY = "READY: Embers Rust no_std entry reached."
EXPECTED = {
    "ro-write": ("TEST: supervisor read-only write target = ", 0x3),
    "nx-exec": ("TEST: supervisor NX fetch target = ", 0x11),
}
FAULT = re.compile(
    r"^FAULT: #PF page fault RIP=0x([0-9a-f]{16}) "
    r"CODE=0x([0-9a-f]{16}) CR2=0x([0-9a-f]{16})$"
)


def require(ok: bool, message: str) -> None:
    if not ok:
        raise ValueError(message)


def canonical_48(address: int) -> bool:
    high = address >> 48
    return high == (0xFFFF if (address >> 47) & 1 else 0)


def validate(serial: str, mode: str, loads=None) -> tuple[int, int]:
    require(mode in EXPECTED, "unsupported permission-fault mode")
    label, expected_code = EXPECTED[mode]
    lines = [line.strip() for line in serial.splitlines()]
    require(READY not in lines and not any("ERROR:" in line for line in lines),
            "fault mode reached READY or emitted ERROR")
    require(sum(line.startswith("FAULT:") for line in lines) == 1,
            "expected exactly one fault report")
    require(sum(line.startswith("TEST:") for line in lines) == 1,
            "expected exactly one test target report")

    targets = []
    for line in lines:
        if line.startswith(label):
            raw = line[len(label):]
            require(re.fullmatch(r"0x[0-9a-f]{16}", raw) is not None,
                    "malformed target diagnostic")
            targets.append(int(raw, 16))
    require(len(targets) == 1, "expected exactly one correct test target")
    target = targets[0]
    require(target != 0 and target % 4096 == 0 and canonical_48(target),
            "fault target must be a canonical, aligned private page")
    if loads is not None:
        require(len(loads) == 3, "incomplete ELF LOAD inventory")
        index = 1 if mode == "ro-write" else 2
        segment = loads[index]
        expected_flags = 4 if mode == "ro-write" else 6
        require(segment["flags"] == expected_flags,
                "test ELF segment lacks expected RX/R/RW layout")
        require(segment["vaddr"] <= target and
                target + 4096 <= segment["vaddr"] + segment["memsz"],
                "entire fault canary page must belong to expected ELF LOAD segment")

    faults = [FAULT.fullmatch(line) for line in lines]
    matches = [match for match in faults if match]
    require(len(matches) == 1, "page-fault report malformed or absent")
    rip, code, cr2 = [int(word, 16) for word in matches[0].groups()]
    require(rip != 0 and canonical_48(rip), "invalid faulting RIP")
    require(code == expected_code,
            f"wrong page-fault error code: expected 0x{expected_code:x}, got 0x{code:x}")
    require(cr2 == target,
            f"CR2 0x{cr2:016x} does not match target 0x{target:016x}")
    if mode == "nx-exec":
        require(rip == target, "NX instruction fetch RIP must equal target")
    else:
        require(rip != target, "write-fault RIP unexpectedly equals data target")
    return target, code


def self_test() -> None:
    target = 0xFFFF_FFFF_8000_3000
    for mode, (_, code) in EXPECTED.items():
        rip = target if mode == "nx-exec" else 0xFFFF_FFFF_8000_0010
        good = (
            f"{EXPECTED[mode][0]}0x{target:016x}\n"
            f"FAULT: #PF page fault RIP=0x{rip:016x} "
            f"CODE=0x{code:016x} CR2=0x{target:016x}\n"
        )
        sample_loads = [
            {"vaddr": 0xFFFF_FFFF_8000_0000, "memsz": 4096, "flags": 5},
            {"vaddr": target - 4096, "memsz": 8192, "flags": 4},
            {"vaddr": target - 4096, "memsz": 8192, "flags": 6},
        ]
        assert validate(good, mode, sample_loads) == (target, code)
        index = 1 if mode == "ro-write" else 2
        wrong_loads = [dict(item) for item in sample_loads]
        wrong_loads[index]["vaddr"] = target + 4096
        try:
            validate(good, mode, wrong_loads)
        except ValueError:
            pass
        else:
            raise AssertionError("out-of-ELF fault target was accepted")
        invalid = [
            good.replace(f"CODE=0x{code:016x}", "CODE=0x0000000000000000"),
            good.replace(f"CR2=0x{target:016x}", "CR2=0xfffffffffffeffff"),
            good.replace(f"RIP=0x{rip:016x}", "RIP=0x0000000000000000"),
            good.replace(f"0x{target:016x}", "0x0000800000000000"),
            good.replace(f"0x{target:016x}", f"0x{target+1:016x}", 1),
            good.replace("FAULT: #PF", "FAULT: #GP"),
            good.replace("TEST:", "TEST: unknown ", 1),
            good + good,
            good + READY + "\n",
            good + "ERROR: unexpected execution\n",
        ]
        for serial in invalid:
            try:
                validate(serial, mode)
            except ValueError:
                continue
            raise AssertionError(f"{mode}: invalid fault diagnostic was accepted")
    print("PASS: supervisor RO/NX fault diagnostic parser and negative cases")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("serial", nargs="?", type=Path)
    parser.add_argument("--mode", choices=tuple(EXPECTED))
    parser.add_argument("--elf", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    try:
        if args.self_test:
            require(args.serial is None and args.mode is None and args.elf is None,
                    "self-test takes no other arguments")
            self_test()
        else:
            require(args.serial is not None and args.mode is not None and
                    args.elf is not None,
                    "provide --mode, --elf and the fault serial log")
            loads = parse_loads(args.elf.read_bytes())
            target, code = validate(args.serial.read_text(), args.mode, loads)
            print(f"PASS: {args.mode} QEMU #PF CODE=0x{code:x} CR2=0x{target:016x}")
    except (ValueError, OSError) as error:
        raise SystemExit(f"FAIL: {error}") from error
