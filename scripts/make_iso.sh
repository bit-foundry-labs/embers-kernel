#!/usr/bin/env bash
set -euo pipefail
# External bootloader. Verify release availability and checksum before relying on this.
LIMINE_TAG="v12.9.6"
KERNEL_ELF="${KERNEL_ELF:-rust-kernel/target/x86_64-unknown-none/release/embers-rust-kernel}"
ISO_KERNEL_NAME="${ISO_KERNEL_NAME:-embers.elf}"
BOOT_CONFIG="${BOOT_CONFIG:-rust-kernel/limine.conf}"
ISO_OUT="${ISO_OUT:-build/embers-rust.iso}"
for tool in curl tar make xorriso sha256sum; do
  command -v "$tool" >/dev/null || { echo "Missing dependency: $tool" >&2; exit 1; }
done
# Independently package the Rust ELF. No C bootstrap is executed.
# Always verify the pinned archive, including on subsequent local builds.
# Never trust a preexisting repository-root "limine-binary" directory.
LIMINE_ARCHIVE="build/limine-binary-${LIMINE_TAG}.tar.gz"
LIMINE_SOURCE_DIR="build/limine-verified-source"
LIMINE_DIR="${LIMINE_SOURCE_DIR}/limine-binary"
mkdir -p build
if [ ! -f "$LIMINE_ARCHIVE" ]; then
  curl -fL --retry 2 -o "$LIMINE_ARCHIVE" "https://github.com/Limine-Bootloader/Limine/releases/download/${LIMINE_TAG}/limine-binary.tar.gz"
fi
printf "%s  %s\n" "b38aabb042b0c0f9f562b7741be3c1204838d7bbacfa78bb411c674bd5b984ee" "$LIMINE_ARCHIVE" | sha256sum --check
# Remove only generated build storage; a stale preexisting working-tree
# directory is deliberately ignored and never used as a build input.
rm -rf -- "$LIMINE_SOURCE_DIR"
mkdir -p "$LIMINE_SOURCE_DIR"
tar -xzf "$LIMINE_ARCHIVE" -C "$LIMINE_SOURCE_DIR"
if [ ! -d "$LIMINE_DIR" ]; then
  echo "Verified Limine archive lacks the expected directory" >&2
  exit 1
fi
make -C "$LIMINE_DIR"
rm -rf build/iso_root
mkdir -p build/iso_root/boot/limine build/iso_root/EFI/BOOT
cp "$KERNEL_ELF" "build/iso_root/boot/$ISO_KERNEL_NAME"
cp "$BOOT_CONFIG" build/iso_root/boot/limine/limine.conf
cp "$LIMINE_DIR/limine-bios.sys" "$LIMINE_DIR/limine-bios-cd.bin" "$LIMINE_DIR/limine-uefi-cd.bin" build/iso_root/boot/limine/
cp "$LIMINE_DIR/BOOTX64.EFI" build/iso_root/EFI/BOOT/
xorriso -as mkisofs -R -r -J -b boot/limine/limine-bios-cd.bin -no-emul-boot -boot-load-size 4 -boot-info-table --efi-boot boot/limine/limine-uefi-cd.bin -efi-boot-part --efi-boot-image --protective-msdos-label build/iso_root -o "$ISO_OUT"
"$LIMINE_DIR/limine" bios-install "$ISO_OUT"
echo "ISO ready: $ISO_OUT"
