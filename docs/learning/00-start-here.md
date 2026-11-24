# 00 — Your first 15 minutes with Embers Kernel

**No installation, terminal, account signup, or pull request required.** This is a suggested 15-minute route, not a measured completion time. One source observation is enough to finish the beginner checkpoint; you can explore the other functions if you have more time. Sharing notes is optional.

## 0–3 minutes: What are we looking at?

An **operating-system kernel** is the privileged software layer that eventually controls access to hardware resources and isolates programs. Embers is **research toward** such a kernel, not a complete operating system.

Right now, Embers can boot a small Rust `no_std` program under the **QEMU emulator**. The **Limine bootloader** loads the kernel and hands it an initial execution environment. The kernel can print status messages, check parts of that environment, and deliberately halt. That is useful evidence of early boot, but it is **not** evidence that it can run user applications, enforce user/kernel isolation, or securely own all page tables.

If you meet an unfamiliar word, use the [plain-language glossary](glossary.md); you do not need to memorize it first.

## 3–8 minutes: Find your first function

Open [`rust-kernel/src/main.rs`](../../rust-kernel/src/main.rs) in another browser tab. This is a long file; you only need a few lines. On a computer, press **Ctrl+F** (or **Cmd+F** on a Mac). On a phone, use your browser's **Find in page** option. Search for `fn kernel_entry`; `fn` means “function” in Rust. GitHub's file search may also work, but searching within this page is the most direct option.

1. Find **`kernel_entry()`**, the Rust entry point after Limine hands off.
2. Look at its first two calls: `serial_init()` and `serial_write(...)`. What message is the kernel *trying* to send?
3. **First success:** Write one sentence: “Before checking the bootloader revision, the kernel initializes serial output and attempts to print an `EMBERS:` message.” Compare it with the source; that's already a complete observation.

You can stop here and move to the self-check. **You do not need to understand the rest of the file to succeed.**

### Optional: Follow two more functions

- Search for `fn check_boot_memory`. It is called later from `kernel_entry()`. Find the branch for `BootMapError::NoResponse`: **What does the kernel do if the bootloader's memory-map response is missing?** (It prints an error and halts.) The [glossary](glossary.md) can explain physical frames and reservations afterward.
- Search for `fn run_kernel`. On the normal path it prints `READY: Embers Rust no_std entry reached.` and calls `halt_forever()`. Feature-gated fault experiments may stop earlier.

**Your evidence:** link to a function or quote the short identifier you found. You inspected source, not a running kernel. Do not claim to have executed tests.

## 8–12 minutes: Explain what you found (optional deeper step)

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

**Beginner checkpoint:** You're done when you find `kernel_entry()`, describe what its first two calls attempt to do, and explain that reading source is not the same as running QEMU. **Deeper checkpoint (optional):** Locate `check_boot_memory()` and `run_kernel()`, explain READY without overstating its meaning, and identify one limitation of the evidence. Neither requires a contribution.

## Where next?

- Want more detail on boot? Continue to [01 — Trace the boot path](01-boot-path.md).
- Curious about memory? Try [02 — Who owns a frame?](02-memory-ownership.md).
- Want to evaluate Rust safety? Try [03 — Review an unsafe boundary](03-unsafe-review.md).
- Want to contribute? Read [CONTRIBUTING](../../CONTRIBUTING.md) and check live issue discussions before volunteering. [Issue #89](https://github.com/bit-foundry-labs/embers-kernel/issues/89) already has a volunteer leading the boot-path documentation; offer review rather than duplicating it.

**Safety boundary:** This introduction is read-only. Do not boot on physical hardware. Running QEMU is optional and covered separately in the [existing setup instructions](../../README.md#boot-embers-in-headless-qemu).
