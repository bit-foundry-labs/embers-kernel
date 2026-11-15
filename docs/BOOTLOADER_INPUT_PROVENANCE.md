# Bootloader input provenance for M02/M06 QEMU builds

**Scope:** ISO packaging only. Limine remains an external bootloader and the
kernel trusts the handoff at this stage. A verified archive does not verify
the running bootloader, boot firmware, hardware configuration, or all inputs
to a reproducible binary.

The ISO builder pins Limine binary release `v12.9.6` and the SHA-256 of
its release archive. A previous build only checked the archive digest when
`limine-binary/` was **absent**. If a local directory already existed,
the script could build and package those unchecked files.

The hardened flow instead:

1. Downloads the pinned archive when the generated cache is missing.
2. Verifies the archive's pinned SHA-256 on **every invocation**. A corrupted
   cached archive stops the build before extraction, without falling back
   to unchecked local files.
3. Recreates `build/limine-verified-source/` from the verified archive.
   All Limine binaries copied into the ISO, and the `bios-install` tool,
   originate from that generated extraction.
4. Ignores any repository-root `limine-binary/` directory, leaving user
   files in that directory untouched.

The normal Rust boot QEMU matrix installs an untrusted marker in a local
`limine-binary/` directory before running `make rust-iso`. A passing build
demonstrates that those local files were ignored. The other kernel and
exception workflows must still boot from the packaged ISO.

**Limits:** A pinned digest is a content-integrity check, not a signature or
proof that the release publisher is uncompromised. `make`, `curl`,
`tar`, `xorriso` and the host toolchain remain trusted. We have not
established fully reproducible byte-for-byte ISO outputs, UEFI Secure Boot,
firmware measurements, or a verified Limine binary.
