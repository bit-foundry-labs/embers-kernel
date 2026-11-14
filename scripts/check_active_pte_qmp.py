#!/usr/bin/env python3
"""Read one live x86-64 ELF text translation via paused QEMU QMP memory reads.

No kernel code, HHDM pointer or guest page-table write is involved. Only a
single present, 4 KiB leaf chain is supported; unsupported formats fail closed.
QEMU observation is not a formal hardware proof or DMA isolation evidence.
"""

import argparse
import json
import re
import socket
import subprocess
import tempfile
import time
from pathlib import Path

from check_elf_loads import derive_candidates, parse_loads

PAGE = 4096
PHYSICAL_RAM = 256 * 1024 * 1024
FRAME_MASK = 0x000F_FFFF_FFFF_F000
READY = "READY: Embers Rust no_std entry reached."
CR3_LINE = re.compile(r"^PAGING: inherited CR3 raw = 0x([0-9a-f]{16})$", re.M)
REG_CR3 = re.compile(r"\bCR3\s*=\s*(?:0x)?([0-9a-fA-F]{8,16})\b")
XP_VALUE = re.compile(r"(?im)^\s*0x[0-9a-f]+:\s*0x([0-9a-f]{1,16})\s*$")


def check(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def canonical_48(va: int) -> bool:
    check(0 <= va < (1 << 64), "virtual address outside u64")
    upper = va >> 48
    return upper == (0xFFFF if (va >> 47) & 1 else 0)


def serial_cr3(text: str) -> int:
    hits = CR3_LINE.findall(text)
    check(len(hits) == 1 and text.count(READY) == 1,
          "expected one inherited CR3 observation and READY")
    return int(hits[0], 16)


def register_cr3(text: str) -> int:
    hits = REG_CR3.findall(text)
    check(len(hits) == 1, "expected one live CPU CR3 register")
    return int(hits[0], 16)


def xp_word(text: str) -> int:
    hits = XP_VALUE.findall(text)
    check(len(hits) == 1, f"expected one readable physical 64-bit word; QEMU said {text[:240]!r}")
    return int(hits[0], 16)


def walk_4k_only(root: int, va: int, read_word) -> tuple[int, list[int]]:
    """Walk one 48-bit VA; reject absent or huge-page/unsupported entries.

    read_word(phys_addr) is injectable, so negative cases need no QEMU.
    We do not claim hardware physical-width, reserved-bit or NXE validation.
    """
    check(canonical_48(va), "noncanonical target VA")
    check(root != 0 and root % PAGE == 0, "invalid CR3 root frame")
    table = root
    entries = []
    for shift in (39, 30, 21, 12):
        index = (va >> shift) & 0x1FF
        location = table + index * 8
        check(location < PHYSICAL_RAM - 7, "table-entry read outside QEMU RAM")
        value = read_word(location)
        check(0 <= value < (1 << 64), "invalid PTE width")
        check(value & 1 != 0, "nonpresent page-table entry")
        if shift > 12:
            check(value & (1 << 7) == 0, "unsupported large-page entry")
        entries.append(value)
        table = value & FRAME_MASK
        check(table < PHYSICAL_RAM, "next frame outside observed QEMU RAM")
    return table + (va & 0xFFF), entries


class Qmp:
    def __init__(self, location: Path):
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.settimeout(5.0)
        self.sock.connect(str(location))
        self.file = self.sock.makefile("rwb", buffering=0)
        greeting = self._message()
        check("QMP" in greeting, "missing QMP greeting")
        self.sequence = 0
        self.execute("qmp_capabilities")

    def _message(self) -> dict:
        for _ in range(32):
            line = self.file.readline()
            check(bool(line), "QMP socket closed")
            response = json.loads(line)
            if "event" not in response:
                return response
        raise ValueError("too many unsolicited QMP messages")

    def execute(self, command: str, arguments=None):
        self.sequence += 1
        data = {"execute": command, "id": self.sequence}
        if arguments is not None:
            data["arguments"] = arguments
        self.file.write(json.dumps(data).encode("utf-8") + b"\r\n")
        response = self._message()
        check(response.get("id") == self.sequence, "QMP response ID mismatch")
        check("error" not in response and "return" in response,
              f"QMP {command} failed: {response.get('error')}")
        return response["return"]

    def hmp(self, command: str) -> str:
        value = self.execute("human-monitor-command", {"command-line": command})
        check(isinstance(value, str), "QMP HMP result not text")
        return value

    def close(self):
        self.file.close()
        self.sock.close()


def run(cpu: str, iso: Path, elf: Path) -> None:
    check(cpu in ("qemu64", "max"), "unsupported QEMU CPU profile")
    loads = parse_loads(elf.read_bytes())
    # The first RX LOAD segment starts at the kernel's linked entry page.
    va = loads[0]["vaddr"]
    with tempfile.TemporaryDirectory(prefix="embers-pte-") as directory:
        location = Path(directory)
        serial = location / "serial.log"
        qmp_path = location / "qmp.sock"
        process = subprocess.Popen([
            "qemu-system-x86_64", "-cpu", cpu, "-m", "256M",
            "-cdrom", str(iso.resolve()), "-display", "none",
            "-serial", f"file:{serial}", "-monitor", "none",
            "-qmp", f"unix:{qmp_path},server=on,wait=off",
            "-no-reboot", "-no-shutdown",
        ], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        monitor = None
        try:
            deadline = time.monotonic() + 15
            text = ""
            while time.monotonic() < deadline:
                check(process.poll() is None, "QEMU exited before READY")
                if serial.exists():
                    text = serial.read_text(errors="replace")
                    if READY in text:
                        break
                time.sleep(0.05)
            else:
                raise ValueError("QEMU did not report READY in 15 seconds")
            observed = serial_cr3(text)
            candidates = derive_candidates(loads, text)
            monitor = Qmp(qmp_path)
            monitor.execute("stop")
            state = monitor.execute("query-status")
            check(state.get("running") is False, "QEMU not paused")
            current = register_cr3(monitor.hmp("info registers"))
            check(current == observed, "CR3 changed between boot serial and paused CPU")
            root = current & FRAME_MASK
            physical, chain = walk_4k_only(
                root, va,
                lambda address: xp_word(monitor.hmp(f"xp /1gx 0x{address:x}")),
            )
            expected_start, _ = candidates[0]
            check(physical == expected_start,
                  "QEMU PTE physical translation differs from Limine candidate")
            writable = all(value & 2 != 0 for value in chain)
            nx_observed = any(value & (1 << 63) != 0 for value in chain)
            print(f"PASS: paused {cpu} active 4-level PTE path matches RX ELF start")
            print(f"  VA=0x{va:016x} PHYSICAL=0x{physical:016x}")
            print(f"  CR3=0x{current:016x}; RW-chain={writable}; NX-bit-seen={nx_observed}")
            print("  QEMU host observation, NOT verified permissions or ownership")
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
    root = 0x1000
    indices = [(va >> shift) & 511 for shift in (39, 30, 21, 12)]
    cells = {
        root + indices[0] * 8: 0x2003,
        0x2000 + indices[1] * 8: 0x3003,
        0x3000 + indices[2] * 8: 0x4003,
        0x4000 + indices[3] * 8: 0x0FF44003,
    }
    assert walk_4k_only(root, va, lambda address: cells[address])[0] == 0x0FF44000
    assert xp_word("0x000000000ff9c800: 0x000000000ff44003\r\n") == 0x0FF44003
    assert register_cr3("CR0=80000011 CR2=0000 CR3=000000000ff9c000") == 0x0FF9C000
    assert serial_cr3("PAGING: inherited CR3 raw = 0x000000000ff9c000\n" + READY) == 0x0FF9C000

    def rejected(action, reason):
        try:
            action()
        except (ValueError, KeyError):
            return
        raise AssertionError("invalid input was accepted: " + reason)

    rejected(lambda: walk_4k_only(root, 0x0000800000000000, cells.__getitem__), "canonicality")
    rejected(lambda: walk_4k_only(0, va, cells.__getitem__), "zero root")
    rejected(lambda: walk_4k_only(root, va, lambda address: cells[address] & ~1), "nonpresent")
    rejected(lambda: walk_4k_only(root, va, lambda address: cells[address] | (1 << 7)), "huge")
    rejected(lambda: xp_word("Cannot access memory"), "physical memory read failed")
    rejected(lambda: register_cr3("CR3=000000000ff9c000 CR3=000000000ff9c000"), "duplicate CR3")
    print("PASS: bounded QMP PTE walker and rejection fixtures (no live QEMU)")


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
