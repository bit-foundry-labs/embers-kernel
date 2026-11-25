# Embers host validation tools

Rust-first migration pilot. The initial dependency-free crate checks the current ELF64 PT_LOAD linker contract and uses checked offsets, bounded program-header counts and widened arithmetic for exclusive address endpoints. It is a host-only library, not part of the kernel binary.

    cargo test --offline --manifest-path tools/embers-validation/Cargo.toml
    cargo fmt --manifest-path tools/embers-validation/Cargo.toml -- --check
    cargo clippy --offline --manifest-path tools/embers-validation/Cargo.toml --all-targets -- -D warnings

The authored fixtures cover the accepted layout, truncated header/table, writable-executable segments, wrong flags, overlaps, invalid extents/alignment, entry bounds and the exclusive address endpoint at 2^64. Rust is unavailable in the authoring environment; no local compilation or passing tests are claimed.

Existing Python validators and every existing CI gate remain unchanged. This is not full parity: serial-derived physical candidates, marker/symbol checks, CLI behavior, real ELF comparisons, and direct differential fixtures are still outstanding. The next increment must compare both implementations before changing Makefile or retiring either Python validator. No hardware mapping or ownership proof is implied.
