#!/usr/bin/env python3
"""Check that Kani rejects a deliberately broken transaction in an isolated copy.

Mutates only a TemporaryDirectory; no repository source, compiled artifact or
production module is modified. Intended for CI after the positive Kani proofs.
"""

from __future__ import annotations

import shutil
import subprocess
import tempfile
from pathlib import Path


REPO = Path(__file__).resolve().parents[2]
HARNESS = "rejection_preserves_pool_and_every_exposed_slot"

# Exact source-match guard: fail loudly if the production transaction changes.
# Move pool consumption *before* registry validation in a temporary copy,
# creating a realistic failure-atomicity bug when owner/capacity validation fails.
CORRECT = """        let reservation = self
            .registry
            .validate(start, pages, owner)
            .map_err(TransactionError::Registry)?;
        self.pool
            .reserve_contiguous(pages)
            .map_err(TransactionError::Pool)?;
"""

BROKEN = """        // NEGATIVE CONTROL ONLY: consume frames before validating registry.
        self.pool
            .reserve_contiguous(pages)
            .map_err(TransactionError::Pool)?;
        let reservation = self
            .registry
            .validate(start, pages, owner)
            .map_err(TransactionError::Registry)?;
"""


def run_negative_control() -> None:
    with tempfile.TemporaryDirectory(prefix="embers-kani-mutant-") as dirname:
        root = Path(dirname)
        (root / "src").mkdir()
        (root / "research").mkdir()
        for name in ("memory.rs", "frame_registry.rs"):
            shutil.copy2(REPO / "rust-kernel" / "src" / name, root / "src" / name)
        proof = "kani_transactional_allocator.rs"
        shutil.copy2(REPO / "rust-kernel" / "research" / proof, root / "research" / proof)

        registry = root / "src" / "frame_registry.rs"
        original = registry.read_text(encoding="utf-8")
        if original.count(CORRECT) != 1:
            raise RuntimeError("Expected exact transactional commit ordering not found")
        registry.write_text(original.replace(CORRECT, BROKEN), encoding="utf-8")

        result = subprocess.run(
            [
                "kani",
                str(root / "research" / proof),
                "--harness",
                HARNESS,
            ],
            cwd=REPO,
            check=False,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            timeout=180,
        )

        report = result.stdout
        for line in report.splitlines():
            if (
                "Checking harness" in line
                or "Status: FAILURE" in line
                or "VERIFICATION:-" in line
                or "Failed Checks:" in line
                or "SUMMARY:" in line
                or "failed (" in line
            ):
                print(line)

        if (
            result.returncode == 0
            or "VERIFICATION:- FAILED" not in report
            or "- Status: FAILURE" not in report
            or "assertion" not in report
            or f"Checking harness verification::{HARNESS}" not in report
        ):
            print(report[-7000:])
            raise RuntimeError(
                "Negative control did not produce an expected Kani assertion failure; "
                "a compilation failure, timeout, or green proof is not acceptable"
            )
        print(
            "PASS: Kani rejected temporary pre-validation pool consumption; "
            "checked-in source remains unchanged"
        )


if __name__ == "__main__":
    run_negative_control()
