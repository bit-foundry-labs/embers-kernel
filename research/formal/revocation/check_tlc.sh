#!/usr/bin/env bash
set -euo pipefail

# Stable TLC 1.7.4; verify the binary before executing it.
EXPECTED_SHA256=936a262061c914694dfd669a543be24573c45d5aa0ff20a8b96b23d01e050e88
RELEASE_URL=https://github.com/tlaplus/tlaplus/releases/download/v1.7.4/tla2tools.jar
MODEL_DIR="$(cd "$(dirname "$0")" && pwd)"
JAR="$MODEL_DIR/.tools/tla2tools-1.7.4.jar"
if printenv TLA_TOOLS_JAR >/dev/null 2>&1; then JAR="$(printenv TLA_TOOLS_JAR)"; fi

if ! command -v java >/dev/null 2>&1; then
  echo "Java 11+ is required" >&2
  exit 1
fi
if [[ ! -f "$JAR" ]]; then
  if [[ "$(printenv TLA_DOWNLOAD || true)" != "1" ]]; then
    echo "No TLC JAR found at $JAR. Run: TLA_DOWNLOAD=1 bash check_tlc.sh" >&2
    exit 1
  fi
  mkdir -p "$(dirname "$JAR")"
  curl --fail --location --retry 2 --max-time 90 "$RELEASE_URL" --output "$JAR"
fi
printf '%s  %s\n' "$EXPECTED_SHA256" "$JAR" | sha256sum --check --status || {
  echo "TLC JAR SHA-256 mismatch: aborting" >&2
  exit 1
}
cd "$MODEL_DIR"
mkdir -p .check-logs
java -Xmx512m -jar "$JAR" -deadlock -workers 1 -config RevocationSafe.cfg RevocationSafe.tla | tee .check-logs/safe.log
if java -Xmx512m -jar "$JAR" -deadlock -workers 1 -config RevocationBuggy.cfg RevocationBuggy.tla >.check-logs/buggy.log 2>&1; then
  echo "Unexpectedly passed: vulnerable model produced no violation" >&2
  exit 1
fi
if ! grep -q 'Invariant NoLateCommit is violated' .check-logs/buggy.log; then
  echo "Buggy model errored without the expected NoLateCommit violation" >&2
  cat .check-logs/buggy.log
  exit 1
fi
python3 check_mutations.py --self-test
python3 check_mutations.py "$JAR"
echo "PASS: corrected TLA+ invariants; broken model and three guard mutants produced expected counterexamples"
