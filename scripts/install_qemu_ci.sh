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
    -o Acquire::https::Timeout=25
  timeout 420s sudo apt-get install -y --no-install-recommends \
    -o Acquire::Retries=2 \
    -o Acquire::http::Timeout=25 \
    -o Acquire::https::Timeout=25 \
    qemu-system-x86 xorriso
}

if ! install_packages; then
  echo "::warning::Default Ubuntu package mirror failed; retrying using archive.ubuntu.com."
  # GitHub-hosted Ubuntu 24.04 commonly uses a deb822 sources file with
  # mirror+file: URIs. Only change the main Ubuntu archive, not security.
  source_file=/etc/apt/sources.list.d/ubuntu.sources
  if [[ ! -f "$source_file" ]]; then
    echo "::error::Cannot locate Ubuntu deb822 sources for mirror fallback."
    exit 1
  fi
  sudo sed -i \
    -e 's|mirror+file:/etc/apt/apt-mirrors.txt|http://archive.ubuntu.com/ubuntu|g' \
    -e 's|http://[[:alnum:].-]*\.archive\.ubuntu\.com/ubuntu|http://archive.ubuntu.com/ubuntu|g' \
    "$source_file"
  install_packages
fi

command -v qemu-system-x86_64
command -v xorriso
qemu-system-x86_64 --version | head -n 1
