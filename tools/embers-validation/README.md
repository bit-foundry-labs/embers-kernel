# Embers host validation tools

Rust-first migration pilot. The initial dependency-free crate checks the current ELF64 PT_LOAD linker contract and uses checked offsets, bounded program-header counts and widened arithmetic for exclusive address endpoints. It is a host-only library, not part of the kernel binary.

    cargo test --offline --manifest-path tools/embers-validation/Cargo.toml
    cargo fmt --manifest-path tools/embers-validation/Cargo.toml -- --check
    cargo clippy --offline --manifest-path tools/embers-validation/Cargo.toml --all-targets -- -D warnings

The authored fixtures cover the accepted layout, truncated header/table, writable-executable segments, wrong flags, overlaps, invalid extents/alignment, entry bounds and the exclusive address endpoint at 2^64. Numeric physical candidates are calculated from the exact Limine serial base lines using checked arithmetic. The exclusive endpoint can equal 2^64. These candidates do not establish physical ownership, mapping, or lifetime.

The Rust validator also rejects byte-disjoint load segments that share a 4 KiB virtual page, because the expected RX, R and RW segments have distinct permissions. ELF generally permits congruent unaligned segment addresses and offsets, so the validator still accepts those when their pages are separate. The existing Python reference does not perform the page-sharing check; Rust is intentionally stricter for this input. Checking the ELF layout does not verify the permissions of actual bootloader or kernel page tables.

The differential tests invoke the existing Python load validator on selected synthetic ELF and serial inputs, and compare acceptance and physical candidate ranges. They require Python 3 and the repository's `scripts/check_elf_loads.py`; the Rust library itself has no dependencies. To compare load fields on a real artifact, first run `make rust-check`, then set `EMBERS_TEST_ELF` to the absolute path of the resulting ELF when running `cargo test`. Without that variable, the real-artifact test reports that it did not run.

When `EMBERS_TEST_SERIAL` and `EMBERS_TEST_ELF` point to a matching QEMU serial log and kernel ELF, the test compares the physical candidates produced by Rust and Python. The Rust boot workflow supplies both paths for each CPU profile and sets `EMBERS_REQUIRE_REAL_ARTIFACT=1`, which fails the real-artifact tests if a required path is missing. These are numerical comparisons, not mapping proofs. The exception workflow also validates each normal and fault-injection ELF with the Rust CLI.

Run the host CLI with `cargo run --offline --manifest-path tools/embers-validation/Cargo.toml -- path/to/kernel.elf --serial path/to/serial.log`. The serial argument is optional. Inputs are limited to 64 MiB for the ELF and 4 MiB for the serial log. Missing, oversized, malformed, or invalid UTF-8 serial inputs cause failure. The CLI is an additional check; it does not replace the existing Python gates.

The library also checks the byte markers and a global function `kernel_entry` in a bounded ELF64 static symbol table. The symbol must equal the ELF entry point and lie in an allocated executable section contained within the executable load segment. This is stricter than matching the output of `nm`. Extended section numbering is not supported. The shared-page rejection is an explicitly tested difference from the Python reference, not full parity: broader malformed-input and marker/symbol differential coverage remain outstanding. Existing Python validators and every existing CI gate remain unchanged. Do not retire either Python validator on the basis of these fixtures.
