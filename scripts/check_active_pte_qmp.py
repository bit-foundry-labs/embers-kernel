#!/usr/bin/env python3
"""Inspect inherited ELF and opt-in stack mappings via paused QEMU QMP reads.

The monitor never dereferences guessed guest pointers or modifies page tables.
Only present 4 KiB leaf chains are supported; unsupported formats fail closed.
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
from check_paging_snapshot import check as validate_paging_snapshot

PAGE = 4096
MAX_PAGES_PER_SEGMENT = 64  # Any larger image needs a reviewed coverage budget.
PHYSICAL_RAM = 256 * 1024 * 1024
FRAME_MASK = 0x000F_FFFF_FFFF_F000
READY = "READY: Embers Rust no_std entry reached."
STACK_BYTES = 4 * PAGE
STACK_BASE_LINE = re.compile(r"^STACK: static BSS virtual base = 0x([0-9a-f]{16})$", re.M)
STACK_SMOKE = "STACK: four aligned BSS pages writable under inherited mappings"
CR3_LINE = re.compile(r"^PAGING: inherited CR3 raw = 0x([0-9a-f]{16})$", re.M)
BSP_RSP_LINE = re.compile(r"^PAGING: observed BSP RSP = 0x([0-9a-f]{16})$", re.M)
REG_CR3 = re.compile(r"\bCR3\s*=\s*(?:0x)?([0-9a-fA-F]{8,16})\b")
REG_RSP = re.compile(r"\bRSP\s*=\s*(?:0x)?([0-9a-fA-F]{8,16})\b")
XP_VALUE = re.compile(r"(?im)^\s*(?:0x)?[0-9a-f]{8,16}:\s*0x([0-9a-f]{1,16})\s*$")


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



def serial_bsp_rsp(text: str) -> int:
    hits = BSP_RSP_LINE.findall(text)
    check(len(hits) == 1, "expected exactly one early BSP RSP snapshot")
    return int(hits[0], 16)


def register_rsp(text: str) -> int:
    hits = REG_RSP.findall(text)
    check(len(hits) == 1, "expected exactly one paused live CPU RSP")
    return int(hits[0], 16)


def inspect_bsp_stack_samples(early_rsp: int, live_rsp: int,
                              root: int, read_word,
                              physical_bits: int) -> dict[str, int]:
    """Inspect eight bytes *below* each RSP; never infer a whole stack range.

    RSP is a point-in-time numeric observation. The tested eight-byte span
    does not establish the stack allocation boundaries or its reclamation
    lifetime, nor does a writable PTE chain authenticate frame ownership.
    """
    observed = {}
    for name, rsp in (("early", early_rsp), ("paused", live_rsp)):
        check(isinstance(rsp, int) and 8 <= rsp < (1 << 64)
              and rsp % 8 == 0 and canonical_48(rsp),
              f"{name} RSP is invalid, noncanonical or misaligned")
        check(canonical_48(rsp - 8), f"{name} RSP underflowed into canonical hole")
        frames = set()
        for va in (rsp - 8, rsp - 1):
            phys, chain = walk_4k_only(root, va, read_word, physical_bits)
            check(phys < PHYSICAL_RAM,
                  f"{name} RSP translation exceeds observed QEMU RAM")
            check(all(bool(entry & 2) for entry in chain),
                  f"{name} RSP has a read-only PTE chain")
            frames.add(phys & ~(PAGE - 1))
        # The eight-byte window cannot span two 4 KiB pages for aligned RSP.
        check(len(frames) == 1, f"{name} RSP window has unexpected frame alias")
        observed[name] = next(iter(frames))
    return observed


def xp_word(text: str) -> int:
    hits = XP_VALUE.findall(text)
    check(len(hits) == 1, f"expected one readable physical 64-bit word; QEMU said {text[:240]!r}")
    return int(hits[0], 16)


def walk_4k_only(root: int, va: int, read_word,
                 physical_bits: int) -> tuple[int, list[int]]:
    """Walk one 48-bit VA; reject out-of-width physical fields at each level.

    Width comes from the checked CPU preflight. This is selected-field
    validation, not complete reserved-bit, PCID or permission validation.
    read_word(physical_addr) is injectable for negative-path tests.
    """
    check(canonical_48(va), "noncanonical target VA")
    check(32 <= physical_bits <= 52, "invalid reported MAXPHYADDR")
    check(root != 0 and root % PAGE == 0, "invalid CR3 root frame")
    check((root & FRAME_MASK) >> physical_bits == 0,
          "reserved physical-address bits in CR3 root")
    table = root
    entries = []
    for shift in (39, 30, 21, 12):
        index = (va >> shift) & 0x1FF
        location = table + index * 8
        check(location < PHYSICAL_RAM - 7, "table-entry read outside QEMU RAM")
        value = read_word(location)
        check(0 <= value < (1 << 64), "invalid PTE width")
        check(value & 1 != 0, "nonpresent page-table entry")
        frame_field = value & FRAME_MASK
        check(frame_field >> physical_bits == 0,
              "reserved physical-address bits in present page-table entry")
        if shift > 12:
            check(value & (1 << 7) == 0, "unsupported large-page entry")
        entries.append(value)
        table = frame_field
        check(table < PHYSICAL_RAM, "next frame outside observed QEMU RAM")
    return table + (va & 0xFFF), entries


def test_mutated_captured_entries(root: int, va: int, read_word,
                                  physical_bits: int) -> int:
    """Inject invalid bits into Python copies of paused QEMU entries only.

    All guest physical-memory reads remain read-only. Each injected entry
    must fail the same MAXPHYADDR check used by the positive live walk.
    """
    check(32 <= physical_bits <= 52, "invalid mutation-test physical width")
    if physical_bits == 52:
        return 0  # No physical address-field bit above MAXPHYADDR remains.
    locations = []

    def record_read(location: int) -> int:
        locations.append(location)
        return read_word(location)

    walk_4k_only(root, va, record_read, physical_bits)
    check(len(locations) == 4, "expected four captured PTE locations")
    reserved = 1 << physical_bits
    for modified_location in locations:
        def modified_read(location: int) -> int:
            raw = read_word(location)
            return raw | reserved if location == modified_location else raw

        try:
            walk_4k_only(root, va, modified_read, physical_bits)
        except ValueError as error:
            check(str(error) == "reserved physical-address bits in present page-table entry",
                  "injected PTE rejected for the wrong reason")
        else:
            raise ValueError("injected PTE was unexpectedly accepted")

    try:
        walk_4k_only(root | reserved, va, read_word, physical_bits)
    except ValueError as error:
        check(str(error) == "reserved physical-address bits in CR3 root",
              "injected CR3 candidate rejected for the wrong reason")
    else:
        raise ValueError("injected CR3 root was unexpectedly accepted")
    return len(locations)


def segment_probe_points(segment: dict[str, int]) -> list[int]:
    """Sample each intersected 4 KiB page and the final loaded byte."""
    start = segment["vaddr"]
    size = segment["memsz"]
    check(size > 0, "empty ELF load segment")
    last = start + size - 1
    first_page = start & ~(PAGE - 1)
    last_page = last & ~(PAGE - 1)
    pages = 1 + (last_page - first_page) // PAGE
    check(0 < pages <= MAX_PAGES_PER_SEGMENT, "segment exceeds bounded QMP page budget")
    points = [start]
    for boundary in range(first_page + PAGE, last_page + 1, PAGE):
        points.append(boundary)
    if points[-1] != last:
        points.append(last)
    return points


def inspect_load_segments(loads: list[dict[str, int]],
                          candidates: list[tuple[int, int]],
                          root: int, read_word, physical_bits: int) -> list[dict]:
    """Compare every LOAD page and its last byte against Limine candidates."""
    check(len(loads) == len(candidates) == 3, "incomplete kernel LOAD inventory")
    observed = []
    for index, (segment, candidate) in enumerate(zip(loads, candidates)):
        start, end = candidate
        check(end - start == segment["memsz"], "candidate span length mismatch")
        permissions = set()
        points = segment_probe_points(segment)
        for va in points:
            physical, chain = walk_4k_only(root, va, read_word, physical_bits)
            expected = start + va - segment["vaddr"]
            check(physical == expected,
                  f"LOAD {index} VA 0x{va:016x} differs from Limine physical candidate")
            permissions.add((all(bool(value & 2) for value in chain),
                             any(bool(value & (1 << 63)) for value in chain)))
        observed.append({"load": index, "flags": segment["flags"],
                         "pages": len({va // PAGE for va in points}),
                         "checks": len(points), "permissions": permissions})
    return observed


def feature_stack_base(serial: str, loads: list[dict[str, int]]) -> int:
    """Validate feature-only stack address against its own ELF RW segment."""
    check(len(loads) == 3 and loads[2]["flags"] == 6,
          "feature-stack ELF must have RX/R/RW load inventory")
    matches = STACK_BASE_LINE.findall(serial)
    check(len(matches) == 1 and serial.count(STACK_SMOKE) == 1,
          "feature-stack log missing or duplicated")
    base = int(matches[0], 16)
    check(base != 0 and base % PAGE == 0, "feature stack base is null or unaligned")
    end = base + STACK_BYTES
    check(end <= 1 << 64 and canonical_48(base) and canonical_48(end - 1),
          "feature stack range is noncanonical or overflows")
    rw = loads[2]
    check(rw["vaddr"] <= base and end <= rw["vaddr"] + rw["memsz"],
          "feature stack range is outside feature ELF RW LOAD")
    return base


def inspect_feature_stack(base: int, loads: list[dict[str, int]],
                          candidates: list[tuple[int, int]],
                          root: int, read_word,
                          physical_bits: int) -> set[tuple[bool, bool]]:
    """Verify first and last byte of each static stack page under old CR3."""
    check(len(loads) == len(candidates) == 3, "incomplete feature ELF inventory")
    rw = loads[2]
    physical_start, physical_end = candidates[2]
    check(physical_end - physical_start == rw["memsz"],
          "feature RW candidate span length mismatch")
    check(rw["vaddr"] <= base and base + STACK_BYTES <= rw["vaddr"] + rw["memsz"],
          "feature stack outside RW LOAD")
    rights = set()
    frames = set()
    for page in range(4):
        addresses = (base + page * PAGE, base + (page + 1) * PAGE - 1)
        for va in addresses:
            phys, chain = walk_4k_only(root, va, read_word, physical_bits)
            expected = physical_start + va - rw["vaddr"]
            check(phys == expected,
                  f"feature stack VA 0x{va:016x} differs from expected backing")
            writable = all(bool(entry & 2) for entry in chain)
            check(writable, "feature stack page lacks writable PTE chain")
            rights.add((writable, any(bool(entry & (1 << 63)) for entry in chain)))
            frames.add(phys & ~(PAGE - 1))
    check(len(frames) == 4, "feature stack pages unexpectedly share frames")
    return rights


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


def run(cpu: str, iso: Path, elf: Path, feature_stack: bool = False) -> None:
    check(cpu in ("qemu64", "max"), "unsupported QEMU CPU profile")
    loads = parse_loads(elf.read_bytes())
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
            early_rsp = serial_bsp_rsp(text) if not feature_stack else None
            physical_bits = validate_paging_snapshot(text)
            candidates = derive_candidates(loads, text)
            stack_base = feature_stack_base(text, loads) if feature_stack else None
            if not feature_stack:
                check(STACK_BASE_LINE.search(text) is None,
                      "default kernel unexpectedly logged feature stack")
            monitor = Qmp(qmp_path)
            monitor.execute("stop")
            state = monitor.execute("query-status")
            check(state.get("running") is False, "QEMU not paused")
            registers = monitor.hmp("info registers")
            current = register_cr3(registers)
            live_rsp = register_rsp(registers) if not feature_stack else None
            check(current == observed, "CR3 changed between boot serial and paused CPU")
            root = current & FRAME_MASK
            # The guest is paused; repeated ancestor reads are immutable.
            # Cache physical PTE words to keep the QMP query count bounded.
            entries = {}

            def read_word(address: int) -> int:
                if address not in entries:
                    entries[address] = xp_word(monitor.hmp(f"xp /1gx 0x{address:x}"))
                return entries[address]

            results = inspect_load_segments(loads, candidates, root, read_word, physical_bits)
            mutation_checks = test_mutated_captured_entries(
                root, loads[0]["vaddr"], read_word, physical_bits)
            print(f"PASS: paused {cpu} QEMU PT_LOAD page coverage matches Limine candidates")
            print(f"  MAXPHYADDR={physical_bits}; {mutation_checks} observed PTE mutations rejected (host-only)")
            print(f"  inherited CR3=0x{current:016x}; 4 KiB leaves only")
            for result in results:
                raw_rights = ", ".join(
                    f"RW-chain={rw},NX-bit-seen={nx}"
                    for rw, nx in sorted(result["permissions"])
                )
                print(f'  LOAD {result["load"]} ELF-flags={result["flags"]} '
                      f'pages={result["pages"]} checked-VA={result["checks"]}; '
                      f'observed PTE bits: {raw_rights}')
            if early_rsp is not None and live_rsp is not None:
                stack_observed = inspect_bsp_stack_samples(
                    early_rsp, live_rsp, root, read_word, physical_bits)
                print(f"PASS: {cpu} sampled early/paused BSP stack locations mapped writable")
                for label, frame in stack_observed.items():
                    print(f"  {label} RSP-8..RSP-1 physical frame=0x{frame:016x}")
                print("  Point samples ONLY: no stack extent, provenance, or lifetime proof")
            if stack_base is not None:
                stack_rights = inspect_feature_stack(
                    stack_base, loads, candidates, root, read_word, physical_bits)
                rights = ", ".join(
                    f"RW-chain={rw},NX-bit-seen={nx}"
                    for rw, nx in sorted(stack_rights)
                )
                print(f"PASS: feature-only stack 4 pages/8 endpoints mapped in {cpu}")
                print(f"  BSS stack VA=0x{stack_base:016x}; observed PTE bits: {rights}")
            print("  QEMU host observation, NOT verified effective permissions or ownership")
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
    assert walk_4k_only(root, va, lambda address: cells[address], 40)[0] == 0x0FF44000
    assert xp_word("0x000000000ff9c800: 0x000000000ff44003\r\n") == 0x0FF44003
    assert xp_word("000000000ffacff8: 0x000000000ffab027\r\n") == 0x0FFAB027
    assert register_cr3("CR0=80000011 CR2=0000 CR3=000000000ff9c000") == 0x0FF9C000
    assert serial_cr3("PAGING: inherited CR3 raw = 0x000000000ff9c000\n" + READY) == 0x0FF9C000

    def rejected(action, expected):
        """Require the intended guard, not an unrelated failure or missing fixture."""
        try:
            action()
        except ValueError as error:
            check(expected in str(error),
                  f"wrong rejection: expected {expected!r}, got {error!s}")
            return
        raise AssertionError("invalid input was accepted: " + expected)

    # Guard against false positives from unrelated failures or broken fixtures.
    try:
        rejected(lambda: check(False, "unrelated failure"), "intended rejection")
    except ValueError as error:
        check("wrong rejection:" in str(error), "unexpected rejection mismatch")
    else:
        raise AssertionError("unrelated ValueError counted as intended rejection")
    try:
        rejected(lambda: {}["missing"], "nonpresent page-table entry")
    except KeyError:
        pass
    else:
        raise AssertionError("missing fixture cell counted as intended rejection")

    rejected(lambda: walk_4k_only(root, 0x0000800000000000, cells.__getitem__, 40), "noncanonical target VA")
    rejected(lambda: walk_4k_only(0, va, cells.__getitem__, 40), "invalid CR3 root frame")
    rejected(lambda: walk_4k_only(root, va, lambda address: cells[address] & ~1, 40), "nonpresent page-table entry")
    rejected(lambda: walk_4k_only(root, va, lambda address: cells[address] | (1 << 7), 40), "unsupported large-page entry")
    rejected(lambda: xp_word("Cannot access memory"), "expected one readable physical 64-bit word")
    rejected(lambda: register_cr3("CR3=000000000ff9c000 CR3=000000000ff9c000"), "expected one live CPU CR3 register")
    # Pure three-page test: cover interior page boundaries and the last byte.
    three_page_segment = {
        "vaddr": va, "memsz": 2 * PAGE + 17, "flags": 5,
    }
    points = segment_probe_points(three_page_segment)
    assert points == [va, va + PAGE, va + 2 * PAGE, va + 2 * PAGE + 16]
    leaves = dict(cells)
    leaves[0x4000 + indices[3] * 8 + 8] = 0x0FF45003
    leaves[0x4000 + indices[3] * 8 + 16] = 0x0FF46003
    # One-segment verification is intentionally rejected: the inventory must
    # contain the full kernel RX/R/RW set.
    rejected(lambda: inspect_load_segments(
        [three_page_segment], [(0x0FF44000, 0x0FF46011)], root,
        leaves.__getitem__, 40), "incomplete kernel LOAD inventory")
    # A complete synthetic inventory exercises all page boundaries and the
    # last byte, plus mismatch/nonpresent rejection in an interior page.
    second = {"vaddr": va + 3 * PAGE, "memsz": 30, "flags": 4}
    third = {"vaddr": va + 4 * PAGE, "memsz": 80, "flags": 6}
    leaves[0x4000 + indices[3] * 8 + 24] = 0x0FF47003
    leaves[0x4000 + indices[3] * 8 + 32] = 0x0FF48003
    segments = [three_page_segment, second, third]
    candidates = [(0x0FF44000, 0x0FF46011),
                  (0x0FF47000, 0x0FF4701E),
                  (0x0FF48000, 0x0FF48050)]
    results = inspect_load_segments(segments, candidates, root, leaves.__getitem__, 40)
    assert [row["checks"] for row in results] == [4, 2, 2]
    rejected(lambda: segment_probe_points({
        "vaddr": va, "memsz": (MAX_PAGES_PER_SEGMENT + 1) * PAGE,
    }), "segment exceeds bounded QMP page budget")
    mismatched = dict(leaves)
    mismatched[0x4000 + indices[3] * 8 + 8] = 0x0FF4F003
    rejected(lambda: inspect_load_segments(
        segments, candidates, root, mismatched.__getitem__, 40), "differs from Limine physical candidate")
    absent = dict(leaves)
    absent[0x4000 + indices[3] * 8 + 16] &= ~1
    rejected(lambda: inspect_load_segments(
        segments, candidates, root, absent.__getitem__, 40), "nonpresent page-table entry")
    # Feature-only stack: validate the logged virtual extent belongs to this
    # particular ELF's RW LOAD and inspect eight addresses on four pages.
    rw_start = va + 4 * PAGE
    stack_start = rw_start + PAGE
    feature_loads = [three_page_segment, second,
                     {"vaddr": rw_start, "memsz": 5 * PAGE, "flags": 6}]
    feature_candidates = [candidates[0], candidates[1],
                          (0x0FF48000, 0x0FF4D000)]
    feature_log = (f"STACK: static BSS virtual base = 0x{stack_start:016x}\n"
                   + STACK_SMOKE + "\n")
    assert feature_stack_base(feature_log, feature_loads) == stack_start
    feature_leaves = dict(leaves)
    for page in range(4, 9):
        feature_leaves[0x4000 + indices[3] * 8 + page * 8] = (
            (0x0FF44000 + page * PAGE) | 0x3
        )
    assert inspect_feature_stack(
        stack_start, feature_loads, feature_candidates, root,
        feature_leaves.__getitem__, 40) == {(True, False)}
    rejected(lambda: feature_stack_base("", feature_loads), "feature-stack log missing or duplicated")
    rejected(lambda: feature_stack_base(feature_log + feature_log,
                                        feature_loads), "feature-stack log missing or duplicated")
    rejected(lambda: feature_stack_base(
        feature_log.replace(f"0x{stack_start:016x}",
                            f"0x{stack_start+1:016x}"),
        feature_loads), "feature stack base is null or unaligned")
    rejected(lambda: feature_stack_base(
        feature_log.replace(f"0x{stack_start:016x}",
                            f"0x{rw_start:016x}"),
        [*feature_loads[:2], {"vaddr": rw_start, "memsz": 2 * PAGE,
                              "flags": 6}]), "feature stack range is outside feature ELF RW LOAD")
    wrong = dict(feature_leaves)
    wrong[0x4000 + indices[3] * 8 + 6 * 8] = 0x0FF6A003
    rejected(lambda: inspect_feature_stack(
        stack_start, feature_loads, feature_candidates, root,
        wrong.__getitem__, 40), "differs from expected backing")
    absent_stack = dict(feature_leaves)
    absent_stack[0x4000 + indices[3] * 8 + 7 * 8] &= ~1
    rejected(lambda: inspect_feature_stack(
        stack_start, feature_loads, feature_candidates, root,
        absent_stack.__getitem__, 40), "nonpresent page-table entry")
    readonly_stack = dict(feature_leaves)
    readonly_stack[0x4000 + indices[3] * 8 + 5 * 8] &= ~2
    rejected(lambda: inspect_feature_stack(
        stack_start, feature_loads, feature_candidates, root,
        readonly_stack.__getitem__, 40), "feature stack page lacks writable PTE chain")


    # Snapshot checks report only two isolated RSP-adjacent 8-byte windows.
    assert serial_bsp_rsp(
        "PAGING: observed BSP RSP = 0xffffffff80000010\n") == va + 16
    assert register_rsp("RAX=00000000 RSP=ffffffff80001010 CR3=00001000") == va + PAGE + 16
    early_rsp = va + 16
    paused_rsp = va + PAGE + 16
    sample_cells = dict(cells)
    sample_cells[0x4000 + indices[3] * 8 + 8] = 0x0FF45003
    snapshot = inspect_bsp_stack_samples(
        early_rsp, paused_rsp, root, sample_cells.__getitem__, 40)
    assert snapshot == {"early": 0x0FF44000, "paused": 0x0FF45000}
    rejected(lambda: serial_bsp_rsp(""), "expected exactly one early BSP RSP snapshot")
    rejected(lambda: serial_bsp_rsp(
        "PAGING: observed BSP RSP = 0xffffffff80000010\n" * 2),
        "expected exactly one early BSP RSP snapshot")
    rejected(lambda: register_rsp("RSP=ffffffff80001010 RSP=ffffffff80001010"),
             "expected exactly one paused live CPU RSP")
    rejected(lambda: register_rsp(""), "expected exactly one paused live CPU RSP")
    for bad in (0, 7, va + 1, 0x0000800000000000, 0xffff000000000000):
        rejected(lambda bad=bad: inspect_bsp_stack_samples(
            bad, paused_rsp, root, sample_cells.__getitem__, 40), "early RSP is invalid, noncanonical or misaligned")
    absent_stack = dict(sample_cells)
    absent_stack[0x4000 + indices[3] * 8 + 8] &= ~1
    rejected(lambda: inspect_bsp_stack_samples(
        early_rsp, paused_rsp, root, absent_stack.__getitem__, 40),
        "nonpresent page-table entry")
    readonly_stack_sample = dict(sample_cells)
    readonly_stack_sample[0x4000 + indices[3] * 8 + 8] &= ~2
    rejected(lambda: inspect_bsp_stack_samples(
        early_rsp, paused_rsp, root, readonly_stack_sample.__getitem__, 40),
        "paused RSP has a read-only PTE chain")
    wrong_leaf = dict(sample_cells)
    wrong_leaf[0x4000 + indices[3] * 8 + 8] = (PHYSICAL_RAM - PAGE) | 3
    # The leaf frame itself may be inside RAM, so this numeric check cannot
    # prove bootloader ownership or absence of aliases to a different frame.
    assert inspect_bsp_stack_samples(
        early_rsp, paused_rsp, root, wrong_leaf.__getitem__, 40)["paused"] == (
            PHYSICAL_RAM - PAGE)
    large_page = dict(sample_cells)
    large_page[0x3000 + indices[2] * 8] |= 1 << 7
    rejected(lambda: inspect_bsp_stack_samples(
        early_rsp, paused_rsp, root, large_page.__getitem__, 40),
        "unsupported large-page entry")

    # Out-of-width address bits must fail at every level *before* masking.
    assert test_mutated_captured_entries(root, va, cells.__getitem__, 40) == 4
    for width in [0, 31, 53, 64]:
        rejected(lambda width=width: walk_4k_only(
            root, va, cells.__getitem__, width), "invalid reported MAXPHYADDR")
    for location in cells:
        corrupt = dict(cells)
        corrupt[location] |= 1 << 40
        rejected(lambda corrupt=corrupt: walk_4k_only(
            root, va, corrupt.__getitem__, 40), "reserved physical-address bits in present page-table entry")
    rejected(lambda: walk_4k_only(
        root | (1 << 40), va, cells.__getitem__, 40), "reserved physical-address bits in CR3 root")
    # At MAXPHYADDR 52 there is no out-of-width bit within PTE bits 51:12.
    assert test_mutated_captured_entries(root, va, cells.__getitem__, 52) == 0

    print("PASS: ELF, MAXPHYADDR, feature-stack, and BSP RSP QMP rejection fixtures (no live QEMU)")


if __name__ == "__main__":
    args = argparse.ArgumentParser(description=__doc__)
    args.add_argument("--self-test", action="store_true")
    args.add_argument("--feature-stack", action="store_true",
                      help="inspect separately built transition-stack feature ELF")
    args.add_argument("--cpu", choices=("qemu64", "max"))
    args.add_argument("--iso", type=Path)
    args.add_argument("--elf", type=Path)
    parsed = args.parse_args()
    try:
        if parsed.self_test:
            check(parsed.cpu is None and parsed.iso is None and parsed.elf is None
                  and not parsed.feature_stack,
                  "self-test takes no QEMU arguments")
            self_test()
        else:
            check(parsed.cpu and parsed.iso and parsed.elf,
                  "live mode needs --cpu, --iso and --elf")
            run(parsed.cpu, parsed.iso, parsed.elf, parsed.feature_stack)
    except (OSError, ValueError, KeyError, TimeoutError) as error:
        raise SystemExit(f"FAIL: {error}") from error
