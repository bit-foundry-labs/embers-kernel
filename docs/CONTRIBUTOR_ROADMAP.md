# Embers Kernel · Contributor research roadmap

This is a research sequence, not a release schedule. Work is tracked in issues and tested pull requests. Design decisions are proposals until accepted.

## Foundation: reproducible execution

**Implemented and CI-tested on `main`:**
- [M02](https://github.com/bit-foundry-labs/embers-kernel/pull/26): Rust `no_std` Limine/QEMU boot, pinned compiler, UART READY.
- [M03](https://github.com/bit-foundry-labs/embers-kernel/pull/27): GDT/TSS/IDT, double-fault IST, deliberate exception tests.
- [M04](https://github.com/bit-foundry-labs/embers-kernel/pull/28): Limine map validation and monotonic physical-frame claims.
- [M05](https://github.com/bit-foundry-labs/embers-kernel/pull/29): bounded, single-writer owner ledger; quarantine cannot be recycled.

**Still needed:** fresh Ubuntu/WSL reproduction by an independent contributor; tested minimum CPU feature requirements beyond `-cpu max`; separate UEFI boot coverage; full fault-state and memory-mapping safety review. See [launch-readiness issue #24](https://github.com/bit-foundry-labs/embers-kernel/issues/24). The historical C reference lives in PR #2, not the Rust build.

## Isolation: the real microkernel
- Persistent physical frame ownership, kernel-owned page tables, supervisor/user permission boundaries, and safe zeroing/reclamation. The M05 accounting demo does not enforce these.
- First separate user-mode task and validated syscall/IPC boundary.
- [IPC v1 proposed contract](IPC_V1_CONTRACT.md): authorization, bounded message copying, ordering, failure outcomes, revocation, and negative-path tests. This is not yet an implemented IPC facility.
- Capability handle table; revocation, attenuation, stale-handle rejection.
- Deterministic scheduling, CPU time budgets and documented admission assumptions.
- Crash isolation and restartable user-space services, with state-recovery tests.

## Adaptive policy, only after isolation
- Host-tested Rust policy validation experiment ([PR #6](https://github.com/bit-foundry-labs/embers-kernel/pull/6)).
- Model Invocation and Execution Budget interface research ([Issue #5](https://github.com/bit-foundry-labs/embers-kernel/issues/5)).
- Shadow policy vs. deterministic baseline, with p50/p99, starvation, overhead and fault injection.
- Optional accelerator broker, after device isolation and DMA ownership semantics are implemented.

## Read before proposing a large subsystem
- [Closed architecture research PR #3](https://github.com/bit-foundry-labs/embers-kernel/pull/3) preserves earlier proposals as historical context, **not implemented guarantees**.
- [Research issue #4](https://github.com/bit-foundry-labs/embers-kernel/issues/4) defines the initial benchmark questions.

## Available first contributions

- [#8: independent Ubuntu/WSL QEMU reproduction](https://github.com/bit-foundry-labs/embers-kernel/issues/8), documentation-only.
- [#89: map the normal Rust boot path from Limine to READY](https://github.com/bit-foundry-labs/embers-kernel/issues/89), documentation-only.

For more experienced reviewers, [#84: independently review M06 QMP integration evidence](https://github.com/bit-foundry-labs/embers-kernel/issues/84).

Claim an issue by commenting with your intended approach and environment. Each issue has testable acceptance criteria. Do not boot on physical hardware.

## Definition of a good first issue
An expected result, a bounded number of files, concrete tests, and an explicit limitation. Missing performance data is an invitation to measure, not invent numbers.
