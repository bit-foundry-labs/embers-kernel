#!/usr/bin/env python3
"""Validate one-shot BSS stack switch logs against the matching feature ELF.

This is a bounded QEMU observation, not proof of page-table root replacement,
physical ownership, rollback on a fault, or DMA isolation.
"""

import argparse
import re
from pathlib import Path

from check_active_pte_qmp import (
    PAGE, READY, STACK_BYTES, canonical_48, check, feature_stack_base,
)
from check_elf_loads import parse_loads

CALLBACK_RSP = re.compile(r"^STACK-SWITCH: callback RSP = 0x([0-9a-f]{16})$", re.M)
RESTORED = "STACK-SWITCH: original BSP RSP restored"


def validate(serial: str, loads: list[dict[str, int]]) -> tuple[int, int]:
    check("ERROR:" not in serial, "unexpected kernel error marker")
    base = feature_stack_base(serial, loads)
    rsp_matches = CALLBACK_RSP.findall(serial)
    check(len(rsp_matches) == 1, "missing or repeated callback RSP")
    check(serial.splitlines().count(RESTORED) == 1,
          "missing or repeated old-RSP restoration marker")
    check(sum(line.startswith("STACK-SWITCH:") for line in serial.splitlines()) == 2,
          "unexpected stack switch metadata")
    check(serial.splitlines().count(READY) == 1,
          "missing or duplicated post-restore READY marker")
    rsp = int(rsp_matches[0], 16)
    top = base + STACK_BYTES
    check(canonical_48(rsp) and rsp % 8 == 0,
          "callback RSP is noncanonical or misaligned")
    check(top - PAGE <= rsp < top,
          "callback RSP is not inside the top page of the dedicated BSS stack")
    return base, rsp


def self_test() -> None:
    va = 0xffff_ffff_8000_0000
    rw = va + 4 * PAGE
    loads = [
        {"vaddr": va, "memsz": PAGE, "flags": 5},
        {"vaddr": va + PAGE, "memsz": PAGE, "flags": 4},
        {"vaddr": rw, "memsz": 6 * PAGE, "flags": 6},
    ]
    base = rw + PAGE
    callback = base + STACK_BYTES - 8
    serial = "\n".join([
        "PAGING: inherited CR3 raw = 0x000000000ff9c000",
        f"STACK: static BSS virtual base = 0x{base:016x}",
        "STACK: four aligned BSS pages writable under inherited mappings",
        f"STACK-SWITCH: callback RSP = 0x{callback:016x}",
        RESTORED,
        READY,
    ]) + "\n"
    assert validate(serial, loads) == (base, callback)

    def rejected(action, name: str) -> None:
        try:
            action()
        except (ValueError, KeyError):
            return
        raise AssertionError(f"invalid stack-switch report accepted: {name}")

    rejected(lambda: validate("", loads), "empty")
    rejected(lambda: validate(serial + serial, loads), "duplicate report")
    rejected(lambda: validate(serial.replace(RESTORED, ""), loads), "no restored marker")
    rejected(lambda: validate(serial.replace(RESTORED, RESTORED + "\nERROR: test"), loads),
             "error after switch")
    rejected(lambda: validate(serial.replace(READY, ""), loads), "no READY")
    rejected(lambda: validate(serial.replace(
        f"0x{callback:016x}", f"0x{(base - 8):016x}"), loads), "old stack pointer")
    rejected(lambda: validate(serial.replace(
        f"0x{callback:016x}", f"0x{(base + STACK_BYTES):016x}"), loads), "top boundary")
    rejected(lambda: validate(serial.replace(
        f"0x{callback:016x}", "0x0000800000000000"), loads), "noncanonical")
    rejected(lambda: validate(serial.replace(
        f"0x{callback:016x}", f"0x{(callback + 1):016x}"), loads), "misaligned")
    rejected(lambda: validate(serial, [*loads[:2],
        {"vaddr": rw, "memsz": 2 * PAGE, "flags": 6}]), "truncated ELF")
    rejected(lambda: validate(serial.replace(
        f"0x{base:016x}", f"0x{(base + 1):016x}"), loads), "misaligned base")
    print("PASS: stack-switch report negative fixtures (host-only, no CR3 writes)")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--serial", type=Path)
    parser.add_argument("--elf", type=Path)
    args = parser.parse_args()
    try:
        if args.self_test:
            check(args.serial is None and args.elf is None,
                  "self-test does not accept a serial or ELF argument")
            self_test()
        else:
            check(args.serial is not None and args.elf is not None,
                  "expected --serial and --elf")
            base, rsp = validate(args.serial.read_text(errors="replace"),
                                 parse_loads(args.elf.read_bytes()))
            print("PASS: stack-switch callback ran on BSS stack and restored old RSP")
            print(f"  feature ELF stack base=0x{base:016x}; observed callback RSP=0x{rsp:016x}")
            print("  QEMU observation only; no stack ownership or replacement CR3 proof")
    except (OSError, ValueError, KeyError) as error:
        raise SystemExit(f"FAIL: {error}") from error
