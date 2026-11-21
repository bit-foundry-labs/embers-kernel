# Embers Kernel

**An open-source, Rust-first microkernel research project for AI-oriented operating systems.**

We're investigating what the kernel should control when applications and AI agents compete for CPU time, memory, and eventually accelerator resources. The aim is a small, predictable enforcement layer: capability-based authority, explicit resource budgets, isolation, and measurable performance.

**This is a research effort, not a working general-purpose operating system or a benchmark-proven AI-optimized kernel.**

[Start learning](docs/learning/README.md) · [Validation Lab](https://embers-kernel-validation-lab.brittneyleighb.chatgpt.site) · [Contribute](CONTRIBUTING.md) · [Research roadmap](docs/CONTRIBUTOR_ROADMAP.md) · [Open issues](https://github.com/bit-foundry-labs/embers-kernel/issues) · [Contributor launch milestone](https://github.com/bit-foundry-labs/embers-kernel/issues/24)

## Validation Lab

[![Preview of the Embers Kernel Validation Lab](docs/assets/validation-lab-preview.svg)](https://embers-kernel-validation-lab.brittneyleighb.chatgpt.site)

[Open the Embers Kernel Validation Lab](https://embers-kernel-validation-lab.brittneyleighb.chatgpt.site) to connect research questions, architectural decisions, protocol assumptions, tasks, code, CI evidence, QEMU results, risks, and the next smallest safe implementation step. The preview is a navigation aid, not a claim that pending hardware or protocol decisions are settled.

## The engineering idea

**Keep enforcement deterministic; keep intelligence outside the trusted kernel.**

- **Original microkernel:** target Rust `no_std`, x86-64 and reproducible QEMU experiments. This is not a Linux fork.
- **Capabilities, not ambient privilege:** explicit authority for memory, processes, IPC, and eventual compute resources; test revocation and stale-handle behavior.
- **AI-aware without AI in the kernel:** untrusted user-space services may propose scheduling or resource policies, but cannot grant themselves authority.
- **Bounded resources and fair fallback:** investigate CPU budgets, admission, latency, starvation, and safe recovery before attempting accelerator integration.
- **Evidence before claims:** compare proposed adaptations against deterministic baselines; measure IPC costs, tail latency, overhead, and failure behavior.

Embers Kernel is **separate from Emberglass OS**, a Linux-based Rust operating-system environment. They explore related questions but do not share a working kernel.

## Where the code actually stands

The default branch **boots a freestanding Rust `no_std` kernel in headless QEMU** and includes early exception handling and physical-frame bookkeeping. It is not yet a protected microkernel.

| Milestone | On `main` | Evidence |
| --- | --- | --- |
| M02: Rust boot | Limine loads the ELF, bounded COM1 diagnostics, `READY` | [#26](https://github.com/bit-foundry-labs/embers-kernel/pull/26) |
| M03: CPU exceptions | Kernel GDT/TSS/IDT; deliberate #UD, #PF and #DF QEMU tests | [#27](https://github.com/bit-foundry-labs/embers-kernel/pull/27) |
| M04: memory map | Validate Limine descriptors; bounded, unique physical-frame accounting | [#28](https://github.com/bit-foundry-labs/embers-kernel/pull/28) |
| M05: frame ledger | Fixed-capacity symbolic owners, cross-owner rejection and **permanent quarantine** | [#29](https://github.com/bit-foundry-labs/embers-kernel/pull/29) |
| Proposed research | Capability and AI-aware resource policy architecture | [ADR #3](https://github.com/bit-foundry-labs/embers-kernel/pull/3), [policy model #6](https://github.com/bit-foundry-labs/embers-kernel/pull/6), not kernel features |

**Not implemented:** kernel-owned page tables, hardware-enforced user/kernel isolation, ring-3 processes, syscalls, IPC, authenticated capability enforcement, DMA/IOMMU quiescence or a deterministic multitasking scheduler. Frame ledger owner IDs are internal demonstration labels, not secure process identities.

A passing QEMU or host-test check proves only the tested milestone. It does **not** establish hardware isolation, kernel-level capability enforcement, production security, scheduler superiority, or a functioning AI operating system.

**Next proposed engineering boundary:** [M06 page-table transition contract](docs/PAGING_TRANSITION_V1.md) specifies the required memory mappings, frame reservations, failure behavior, and QEMU evidence before Embers can safely replace Limine's page-table root. It is a review proposal, not a completed milestone.

## Boot Embers in headless QEMU

**Development emulator only. Do not boot on physical hardware.** On Ubuntu or WSL2 Ubuntu, install Rust via [rustup](https://rustup.rs) and then:

```bash
git clone https://github.com/bit-foundry-labs/embers-kernel.git
cd embers-kernel
sudo apt-get update
sudo apt-get install -y make binutils gcc libc6-dev python3 qemu-system-x86 xorriso curl tar
cd rust-kernel
rustup toolchain install 1.90.0 --component clippy,rustfmt --target x86_64-unknown-none
cd ..
make rust-check
make rust-iso
qemu-system-x86_64 -cpu max -m 256M -cdrom build/embers-rust.iso \
  -display none -serial stdio -monitor none -no-reboot
```

The serial log should contain `READY: Embers Rust no_std entry reached.`. The intentionally halted kernel keeps QEMU running; stop the emulator with Ctrl+C. The measured emulator baseline is documented in [the Rust boot notes](rust-kernel/README.md): `-cpu max` is retained as a control profile and `qemu64` is the current minimum **tested emulator** model. This is not a minimum hardware requirement.

Run the pure-Rust memory and ownership tests independently of QEMU:

```bash
make rust-memory-test
make rust-ledger-test
```

For the four-mode exception smoke test, see [M03 exception instructions](rust-kernel/M03_EXCEPTIONS.md); for allocation and ownership semantics see [M04](rust-kernel/M04_PHYSICAL_MEMORY.md) and [M05](rust-kernel/M05_FRAME_LEDGER.md). The [CI workflows](.github/workflows/) run these against supported development-emulator configurations.

The earlier C prototype remains accessible as [historical PR #2](https://github.com/bit-foundry-labs/embers-kernel/pull/2). It is **not** required to build or execute the Rust kernel.

For the host-only policy experiment now on `main`, run `cargo test` in `experiments/rust-policy-guard`. [PR #6](https://github.com/bit-foundry-labs/embers-kernel/pull/6) records its development. This experiment does not execute a privileged Rust kernel.

## Learn by contributing

New to kernels or Rust systems programming? Start with the [Embers Kernel Learning Lab](docs/learning/README.md): three short, evidence-based exercises on the boot path, memory ownership, and reviewing `unsafe` Rust. Browser-only source review is welcome; no privileged code changes or physical hardware are needed. See the [contributor test selection guide](CONTRIBUTING.md#run-the-right-checks-before-opening-a-pr) if you choose to run tests.

## Help build it

We're looking for contributors interested in **Rust systems programming, kernel bring-up, x86-64/QEMU, capability security, IPC, concurrency, scheduling, model checking, and reproducible benchmarks**.

Start with a narrow, verifiable contribution, whether you are learning the codebase or reviewing advanced work:

- **Documentation learning and review:** [Trace the Rust boot path (#89)](https://github.com/bit-foundry-labs/embers-kernel/issues/89). Contributor `archweave` has volunteered to lead the requested map; offer a source-linked review or coordinate on the issue instead of opening a duplicate PR.
- **Completed reproducibility example:** [Ubuntu/QEMU reproduction (#8)](https://github.com/bit-foundry-labs/embers-kernel/issues/8) was completed by merged [PR #87](https://github.com/bit-foundry-labs/embers-kernel/pull/87). The [Ubuntu 26.04 evidence](rust-kernel/README.md#ubuntu-2604-reproduction-record-issue-8) is available to study or independently reproduce, but #8 is no longer an open contribution task.
- **Independent review:** The [M06 pre-write integration queue (#110)](https://github.com/bit-foundry-labs/embers-kernel/issues/110) tracks remaining security-review questions. Current examples are [#104: Limine memory-map overlap validation](https://github.com/bit-foundry-labs/embers-kernel/pull/104) and [#107: copied boot-map classifications](https://github.com/bit-foundry-labs/embers-kernel/pull/107). Both are draft PRs; confirm their current status and coordinate on the PR before reviewing.

For deeper research, see the [capability threat-model task](https://github.com/bit-foundry-labs/embers-kernel/issues/12) and the [contributor-launch milestone](https://github.com/bit-foundry-labs/embers-kernel/issues/24).

Read [the contribution guide](CONTRIBUTING.md), [research roadmap](docs/CONTRIBUTOR_ROADMAP.md), [review standard](docs/CODE_REVIEW_STANDARD.md), and [Code of Conduct](CODE_OF_CONDUCT.md). Comment on an issue before undertaking substantial work.

## Reviewers and future maintainers

Want to help strengthen Embers without writing kernel code? **Independent technical reviews are contributions**, especially reviews of paging, unsafe Rust, capability protocols, and reproducible QEMU tests. Public review comments do not require repository write access.

**Start with a concrete review:** Find an active question in the [M06 integration queue](https://github.com/bit-foundry-labs/embers-kernel/issues/110) or browse [open pull requests](https://github.com/bit-foundry-labs/embers-kernel/pulls?q=is%3Apr+is%3Aopen). Check the latest commit, identify an invariant or negative test to inspect, and comment with the exact evidence and remaining limitations. Coordinate before beginning an extensive review, particularly on draft PRs.

**Grow through demonstrated work:** Anyone can contribute; reviewers who demonstrate careful judgment across changes may request **community-reviewer recognition** using the [public role-interest template](https://github.com/bit-foundry-labs/embers-kernel/issues/new?template=role_interest.md), linking their work. Recognition requires a consented, recorded maintainer decision and grants no automatic GitHub privileges. A future maintainer appointment separately requires sustained trust, explicit invitation, defined responsibilities, and a deliberate least-privilege access decision.

Read the [community roles and selection process](docs/CONTRIBUTOR_ROLES.md) and the [review standard](docs/CODE_REVIEW_STANDARD.md). No open role promises repository access, employment, or compensation.

## License and stewardship

An experimental open-source project stewarded by [Bit Foundry Labs](https://github.com/bit-foundry-labs), licensed under [Apache-2.0](LICENSE). See [governance](docs/GOVERNANCE.md) and [security policy](SECURITY.md). Contributions are welcome; participation does not imply funding, employment, or publication credit.
