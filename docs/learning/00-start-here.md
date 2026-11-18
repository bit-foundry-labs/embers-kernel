# 00 — Your first 15 minutes with Embers Kernel

**No installation, terminal, account signup, or pull request required.** Use a browser to read a few lines of Rust and explain what they demonstrate. You can stop after the self-check; sharing your notes is optional.

## 0–3 minutes: What are we looking at?

An **operating-system kernel** is the privileged software layer that eventually controls access to hardware resources and isolates programs. Embers is **research toward** such a kernel, not a complete operating system.

Right now, Embers can boot a small Rust `no_std` program under the **QEMU emulator**. The **Limine bootloader** loads the kernel and hands it an initial execution environment. The kernel can print status messages, check parts of that environment, and deliberately halt. That is useful evidence of early boot, but it is **not** evidence that it can run user applications, enforce user/kernel isolation, or securely own all page tables.

If you meet an unfamiliar word, use the [plain-language glossary](glossary.md); you do not need to memorize it first.

## 3–8 minutes: Follow just three functions

Open [`rust-kernel/src/main.rs`](../../rust-kernel/src/main.rs) in a new browser tab. Use GitHub's file search or your browser's Find command to locate each function **by name**, not line number.

1. **`kernel_entry()`**: This is the Rust entry point after Limine hands off. Find the first two calls, `serial_init()` and `serial_write(...)`. In ordinary words, what can the kernel attempt to tell us at this stage?
2. **`check_boot_memory()`**: Called later from `kernel_entry()`. Notice that it handles errors while establishing a bounded reservation of physical-frame **address numbers**. Why might the code stop instead of continuing after invalid boot data?
3. **`run_kernel()`**: Find the normal-path `READY: Embers Rust no_std entry reached.` message and the subsequent `halt_forever()`. The READY message is conditional: feature-gated fault experiments may stop earlier instead.

**Your evidence:** link to the function or quote a short identifier you located. Do not claim to have run the kernel; this is a source trace.

## 8–12 minutes: Make one testable claim

Complete these three sentences, in your own words:

- **I found:** `____()` in [`main.rs`](../../rust-kernel/src/main.rs).
- **The code does:** ____. I know because the source calls or checks ____.
- **This does not prove:** ____.

**Worked example:** “In `kernel_entry()`, `serial_init()` runs before the first `EMBERS:` message is written. That shows the intended initialization order in source; it does not prove that serial output works on every physical computer.”

## 12–15 minutes: Check your reasoning

Try answering before opening the explanations.

<details>
<summary>1. Does seeing READY prove there are working user processes?</summary>

No. The normal path prints READY and halts. Embers does not yet have user-mode processes or a general-purpose scheduler. See [`run_kernel()`](../../rust-kernel/src/main.rs) and the [project's implementation status](../../README.md#where-the-code-actually-stands).
</details>

<details>
<summary>2. Does a valid frame reservation prove hardware-enforced memory isolation?</summary>

No. The early bookkeeping tracks physical address numbers; it does not replace Limine's inherited page tables or enforce access rights. See [`check_boot_memory()`](../../rust-kernel/src/main.rs) and [the unsafe-boundary inventory](../../rust-kernel/UNSAFE_BOUNDARIES.md).
</details>

<details>
<summary>3. What did you actually verify?</summary>

You inspected source code in a browser. Write **“Source review only; tests not run.”** You have not personally executed QEMU, the host tests, or the privileged kernel.
</details>

**You're done when** you can identify those three functions, explain the READY marker without overstating its meaning, and name one limit of your evidence. This is a complete learning activity without a contribution.

## Where next?

- Want more detail on boot? Continue to [01 — Trace the boot path](01-boot-path.md).
- Curious about memory? Try [02 — Who owns a frame?](02-memory-ownership.md).
- Want to evaluate Rust safety? Try [03 — Review an unsafe boundary](03-unsafe-review.md).
- Want to contribute? Read [CONTRIBUTING](../../CONTRIBUTING.md) and check live issue discussions before volunteering. [Issue #89](https://github.com/bit-foundry-labs/embers-kernel/issues/89) already has a volunteer leading the boot-path documentation; offer review rather than duplicating it.

**Safety boundary:** This introduction is read-only. Do not boot on physical hardware. Running QEMU is optional and covered separately in the [existing setup instructions](../../README.md#boot-embers-in-headless-qemu).
