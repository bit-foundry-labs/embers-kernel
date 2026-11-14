# Embers Rust-first x86-64 QEMU kernel. The legacy C prototype is not a build dependency.
RUST_KERNEL := rust-kernel/target/x86_64-unknown-none/release/embers-rust-kernel
RUST_FEATURES ?=

.PHONY: all check rust-check rust-iso rust-run clean
all: rust-check
check: rust-check

rust-check:
	cd rust-kernel && cargo build --release --offline $(RUST_FEATURES)
	python3 scripts/check_rust_elf.py $(RUST_KERNEL)

rust-iso: rust-check
	KERNEL_ELF="$(RUST_KERNEL)" ISO_KERNEL_NAME="embers.elf" BOOT_CONFIG="rust-kernel/limine.conf" ISO_OUT="build/embers-rust.iso" bash scripts/make_iso.sh

rust-run: rust-iso
	qemu-system-x86_64 -cpu max -m 256M -cdrom build/embers-rust.iso -serial stdio -no-reboot -no-shutdown

clean:
	rm -rf build
	cd rust-kernel && cargo clean

# Host-only tests for the pure Rust boot memory-map selection and accounting.
.PHONY: rust-memory-test
rust-memory-test:
	mkdir -p build
	cd rust-kernel && rustc --edition=2021 --test src/memory.rs -o ../build/embers-memory-host-tests
	./build/embers-memory-host-tests

# Host-only ownership-ledger tests using the pure no_std Rust modules.
.PHONY: rust-ledger-test
rust-ledger-test:
	mkdir -p build
	cd rust-kernel && rustc --edition=2021 --test tests/frame_ledger_host.rs -o ../build/embers-frame-ledger-tests
	./build/embers-frame-ledger-tests

# Host-only tests of validated Limine address arithmetic, not real mappings.
.PHONY: rust-provenance-test
rust-provenance-test:
	mkdir -p build
	cd rust-kernel && rustc --edition=2021 --test tests/address_provenance_host.rs -o ../build/embers-address-provenance-tests
	./build/embers-address-provenance-tests

# Host-only tests of bounded serial diagnostic error propagation.
.PHONY: rust-serial-test
rust-serial-test:
	mkdir -p build
	rustc --edition=2021 --test rust-kernel/src/serial.rs -o build/embers-serial-host-tests
	./build/embers-serial-host-tests

# Pure host validation of CR0/CR4 bits; privileged sampling is QEMU-only.
.PHONY: rust-paging-mode-test
rust-paging-mode-test:
	mkdir -p build
	cd rust-kernel && rustc --edition=2021 --test tests/paging_mode_host.rs -o ../build/embers-paging-mode-tests
	./build/embers-paging-mode-tests

# Host-only fail-closed transition ordering model; no privileged CR3 write.
.PHONY: rust-transition-gate-test
rust-transition-gate-test:
	mkdir -p build
	cd rust-kernel && rustc --edition=2021 --test tests/cr3_transition_gate_host.rs -o ../build/embers-transition-gate-tests
	./build/embers-transition-gate-tests

# One-shot BSP initial-pool issuance tests; no hardware ownership proof.
.PHONY: rust-boot-pool-gate-test
rust-boot-pool-gate-test:
	mkdir -p build
	cd rust-kernel && rustc --edition=2021 --test tests/boot_pool_gate_host.rs -o ../build/embers-boot-pool-gate-tests
	./build/embers-boot-pool-gate-tests

# Host-only M06d range-registry invariants; does not claim hardware ownership.
.PHONY: rust-frame-registry-test
rust-frame-registry-test:
	mkdir -p build
	cd rust-kernel && rustc --edition=2021 --test tests/frame_registry_host.rs -o ../build/embers-frame-registry-tests
	./build/embers-frame-registry-tests
