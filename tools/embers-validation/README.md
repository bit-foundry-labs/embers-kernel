# Embers host validation tools

Rust-first migration pilot. The initial dependency-free crate checks the current ELF64 PT_LOAD linker contract and uses checked offsets, bounded program-header counts and widened arithmetic for exclusive address endpoints. It is a host-only library, not part of the kernel binary.

    cargo test --offline --manifest-path tools/embers-validation/Cargo.toml
    cargo fmt --manifest-path tools/embers-validation/Cargo.toml -- --check
    cargo clippy --offline --manifest-path tools/embers-validation/Cargo.toml --all-targets -- -D warnings

The authored fixtures cover the accepted layout, truncated header/table, writable-executable segments, wrong flags, overlaps, invalid extents/alignment, entry bounds and the exclusive address endpoint at 2^64. Numeric physical candidates are calculated from the exact Limine serial base lines using checked arithmetic. The exclusive endpoint can equal 2^64. These candidates do not establish physical ownership, mapping, or lifetime.

The differential tests invoke the existing Python load validator on selected synthetic ELF and serial inputs, and compare acceptance and physical candidate ranges. They require Python 3 and the repository's `scripts/check_elf_loads.py`; the Rust library itself has no dependencies. To compare load fields on a real artifact, first run `make rust-check`, then set `EMBERS_TEST_ELF` to the absolute path of the resulting ELF when running `cargo test`. Without that variable, the real-artifact test reports that it did not run.

When `EMBERS_TEST_SERIAL` and `EMBERS_TEST_ELF` point to a matching QEMU serial log and kernel ELF, the test compares the physical candidates produced by Rust and Python. The Rust boot workflow supplies both paths for each CPU profile. These are numerical comparisons, not mapping proofs.

Run the host CLI with `cargo run --offline --manifest-path tools/embers-validation/Cargo.toml -- path/to/kernel.elf --serial path/to/serial.log`. The serial argument is optional. Inputs are limited to 64 MiB for the ELF and 4 MiB for the serial log. Missing, oversized, malformed, or invalid UTF-8 serial inputs cause failure. The CLI is an additional check; it does not replace the existing Python gates.

The library also checks the byte markers and a defined `kernel_entry` in a bounded ELF64 static symbol table. The latter is stricter than matching the output of `nm`; it does not establish that the symbol identifies the ELF entry point. Extended section numbering is not supported. This is not full parity: broader malformed-input and marker/symbol differential coverage remain outstanding. Existing Python validators and every existing CI gate remain unchanged. Do not retire either Python validator on the basis of these fixtures.
