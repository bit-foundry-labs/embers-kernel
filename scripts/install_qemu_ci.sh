#!/usr/bin/env bash
# CI-only QEMU dependency provisioning. Never silently skip emulator tests.
set -euo pipefail

if command -v qemu-system-x86_64 >/dev/null && command -v xorriso >/dev/null; then
  echo "QEMU and xorriso are already installed."
  exit 0
fi

install_packages() {
  timeout 120s sudo apt-get update \
    -o Acquire::Retries=2 \
    -o Acquire::http::Timeout=25 \
    -o Acquire::https::Timeout=25 || return 1
  timeout 420s sudo apt-get install -y --no-install-recommends \
    -o Acquire::Retries=2 \
    -o Acquire::http::Timeout=25 \
    -o Acquire::https::Timeout=25 \
    qemu-system-x86 xorriso || return 1
}

if ! install_packages; then
  echo "::warning::Default Ubuntu package mirror failed; retrying using archive.ubuntu.com."
  source_file=/etc/apt/sources.list.d/ubuntu.sources
  if [[ ! -f "$source_file" ]]; then
    echo "::error::Cannot locate Ubuntu deb822 sources for mirror fallback."
    exit 1
  fi
  # Edit a temporary copy first; preserve the original sources for restoration.
  original=$(mktemp)
  replacement=$(mktemp)
  sudo cp "$source_file" "$original"
  cp "$original" "$replacement"
  trap 'rm -f "$original" "$replacement"' EXIT
  if ! python3 scripts/qemu_mirror_fallback.py "$replacement"; then
    echo "::error::Unsupported Ubuntu sources layout; refusing broad mirror rewrite."
    exit 1
  fi
  sudo cp "$replacement" "$source_file"
  if ! install_packages; then
    sudo cp "$original" "$source_file"
    echo "::error::QEMU package installation failed after bounded mirror fallback."
    exit 1
  fi
fi

command -v qemu-system-x86_64
command -v xorriso
qemu-system-x86_64 --version | head -n 1
