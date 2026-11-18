# A small glossary for the Embers Learning Lab

This glossary describes terms **as they appear in the current Embers research kernel**. It is a reading aid, not a claim that planned OS features are already implemented.

| Term | In everyday language | In Embers today |
| --- | --- | --- |
| **Kernel** | The privileged part of an operating system that can manage hardware and enforce access rules. | A small experimental Rust program booting in QEMU; not yet a general-purpose OS. |
| **Bootloader (Limine)** | Software that prepares the machine and hands control to the kernel. | Provides the initial kernel image, memory information, stack and page tables that Embers initially trusts. |
| **QEMU** | A program that emulates a computer so low-level code can be tested without booting physical hardware. | The supported environment for the project's boot and intentional fault experiments. |
| **Rust `no_std`** | Rust code that does not rely on the normal operating-system standard library. | Used for freestanding kernel code; this alone does not make code safe. |
| **Entry point** | The function where execution begins after another component hands off control. | `kernel_entry()` in [`main.rs`](../../rust-kernel/src/main.rs). |
| **BSP** | The first processor responsible for starting the kernel. | Embers currently assumes one starting CPU; coordinated multicore execution is not implemented. |
| **Serial / COM1** | A simple channel for sending diagnostic text. | Used to print boot, error and READY markers under QEMU. |
| **READY marker** | A message signaling a specific observed milestone. | A normal boot path reports `READY: Embers Rust no_std entry reached.` before deliberately halting. It does not mean a full OS is ready. |
| **Physical frame** | A small, fixed-size piece of physical memory. | Embers tracks the addresses of these pieces but does not independently enforce who may access them. |
| **Frame ledger** | A record of who is allowed to claim or use a frame within a software model. | Symbolic owner tokens and quarantine rules, not enforced user-process memory isolation. |
| **Page table / CR3** | A page table helps the CPU find where a program's memory addresses point. CR3 is a CPU register identifying the active page-table root on x86-64. | Embers reads information about the bootloader's page tables but has not replaced them with its own. |
| **CPL0 / ring 0** | A processor privilege level reserved for kernel-mode operations. | Early boot executes privileged instructions; ring-3 user processes are not implemented. |
| **GDT / TSS / IDT** | CPU data structures involved in segmentation, task state and exception delivery. | Initialized by `arch::install()` for the single-BSP boot path. |
| **`unsafe` in Rust** | Code in which the programmer must uphold rules the compiler cannot fully check. | Those rules need careful review; a `SAFETY:` comment describes an assumption, not proof. |
| **Evidence boundary** | What an observation actually shows, and what it does not show. | Passing one QEMU test does not prove that the kernel is safe on every computer. |

Back to [your first 15 minutes](00-start-here.md) or the [Learning Lab](README.md). For deeper technical details, see [the Rust kernel notes](../../rust-kernel/README.md) and [unsafe-boundary inventory](../../rust-kernel/UNSAFE_BOUNDARIES.md).
