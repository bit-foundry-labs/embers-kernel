#!/usr/bin/env python3
"""TLC negative controls: deliberately remove one RevocationSafe admission/lifetime guard.

The mutants are generated from the actual safe TLA+ source, not reimplemented
state machines. An expected invariant violation is evidence that TLC exercises
the corresponding failure path in this bounded abstract model only.
"""
import argparse
import shutil
import subprocess
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
MODEL = HERE / "RevocationSafe.tla"
CONFIG = HERE / "RevocationSafe.cfg"
LOGS = HERE / ".check-logs"

# Exact anchored replacements prevent silently testing an unrelated model.
MUTATIONS = (
    (
        "post-revoke-admission",
        "NoPostRevokeAdmission",
        'Admit(c) ==\n    /\\ phase = "Active"\n',
        'Admit(c) ==\n    /\\ TRUE\n',
    ),
    (
        "stale-generation-admission",
        "NoStaleHandleAdmission",
        'Admit(c) ==\n    /\\ phase = "Active"\n'
        '    /\\ c \\notin holds\n    /\\ handles[c] = generation\n',
        'Admit(c) ==\n    /\\ phase = "Active"\n'
        '    /\\ c \\notin holds\n    /\\ TRUE\n',
    ),
    (
        "unquiesced-device-reclaim",
        "NoEarlyReclaim",
        'Quiesce ==\n    /\\ phase \\in {"Draining", "Quarantined"}\n'
        '    /\\ holds = {}\n    /\\ device = "Idle"\n',
        'Quiesce ==\n    /\\ phase \\in {"Draining", "Quarantined"}\n'
        '    /\\ holds = {}\n    /\\ TRUE\n',
    ),
)


def generate(source: str):
    for label, invariant, original, replacement in MUTATIONS:
        occurrences = source.count(original)
        if occurrences != 1:
            raise ValueError(
                f"{label}: expected one exact TLA+ guard, found {occurrences}"
            )
        mutated = source.replace(original, replacement, 1)
        if mutated == source:
            raise AssertionError(f"{label}: no source mutation occurred")
        yield label, invariant, mutated


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("jar", nargs="?", type=Path, help="verified TLC JAR path")
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="verify exact guard anchors without executing TLC",
    )
    args = parser.parse_args()

    source = MODEL.read_text(encoding="utf-8")
    variants = tuple(generate(source))
    if args.self_test:
        for label, invariant, _ in variants:
            print(f"PASS: {label} has exact source anchor for {invariant}")
        return

    if args.jar is None:
        parser.error("provide the verified TLC JAR path, or use --self-test")
    jar = args.jar.resolve(strict=True)
    LOGS.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="embers-revocation-mutants-") as tmp:
        for label, invariant, mutated in variants:
            work = Path(tmp) / label
            work.mkdir()
            (work / MODEL.name).write_text(mutated, encoding="utf-8")
            shutil.copyfile(CONFIG, work / CONFIG.name)
            try:
                run = subprocess.run(
                    [
                        "java",
                        "-Xmx512m",
                        "-jar",
                        str(jar),
                        "-deadlock",
                        "-workers",
                        "1",
                        "-config",
                        CONFIG.name,
                        MODEL.name,
                    ],
                    cwd=work,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.STDOUT,
                    text=True,
                    timeout=120,
                    check=False,
                )
            except subprocess.TimeoutExpired as exc:
                raise RuntimeError(f"{label}: TLC timed out") from exc
            log = LOGS / f"mutant-{label}.log"
            log.write_text(run.stdout, encoding="utf-8")
            expected = f"Invariant {invariant} is violated"
            if run.returncode == 0 or expected not in run.stdout:
                raise RuntimeError(
                    f"{label}: TLC did not report the expected {invariant} "
                    f"counterexample (exit {run.returncode}); inspect {log}"
                )
            print(f"PASS: TLC rejects {label} via {invariant}; log: {log}")


if __name__ == "__main__":
    main()
