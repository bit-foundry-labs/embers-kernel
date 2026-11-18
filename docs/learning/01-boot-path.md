# 01 — Trace the path from Limine to READY

**Question:** What does the kernel actually do between the bootloader handoff and its observable `READY` marker?

**Prerequisites:** Ability to read basic Rust functions and navigate GitHub. **Mode:** browser-only source trace, with optional QEMU reproduction.

## Observe

Read [rust-kernel/README.md](../../rust-kernel/README.md) and [rust-kernel/src/main.rs](../../rust-kernel/src/main.rs). Locate `kernel_entry()` and `run_kernel()`. Identify the bootloader as **Limine**, not the historical C prototype. Search the source for `READY: Embers Rust no_std entry reached.`.

## Explain

Create a six-to-ten-row table in your notes:

| Stage or function | Source link | Input or assumption | Observable output or failure |
| --- | --- | --- | --- |
| Limine handoff | Link the actual entry source | Boot protocol requirements | Kernel entry begins |
| Entry validation | Find the current function | State the checked preconditions | Log or stop |
| Architecture setup | Find the current call site | State what is initialized | Evidence or limitation |
| Memory bookkeeping | Find the current call site | Distinguish accounting from isolation | Evidence or limitation |
| READY and halt | Find the current call site | Normal boot path | Serial marker and intentional halt |

**Worked example:** For the serial initialization stage, link [`kernel_entry()` in `main.rs`](../../rust-kernel/src/main.rs), note the early BSP's COM1 assumption, and record that `serial_init()` runs before the first `EMBERS:` banner. This observation does not prove physical UART compatibility.

The rows above are prompts, **not a claim about exact execution order**. Trace the order in the current source, and distinguish normal boot from feature-gated fault and ownership smoke tests.

## Reproduce, if you have an Ubuntu/WSL environment

Follow the [repository's current boot instructions](../../README.md#boot-embers-in-headless-qemu). Record tool versions, commands, and the serial output you actually observed. Do not boot on physical hardware. If you do not have QEMU, perform the source trace only and explicitly mark tests as not run.

## Critique

Answer in a few sentences: Does reaching `READY` prove a user-mode process exists? Does it establish secure page-table ownership? What evidence would be needed to support either claim?

## Contribute

[Issue #89](https://github.com/bit-foundry-labs/embers-kernel/issues/89) proposes a bounded `rust-kernel/BOOT_PATH_MAP.md` documentation PR. `archweave` has already volunteered and been invited to lead that work. You can do this exercise independently, then coordinate on the issue to offer review or feedback rather than opening a duplicate PR. Keep all function names and claims anchored to the then-current `main`.

**Completion evidence:** a source-linked sequence, one explicit unproven property, and a clear statement of whether any tests were run.
