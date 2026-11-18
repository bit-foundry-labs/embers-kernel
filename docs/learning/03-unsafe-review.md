# 03 — Review an `unsafe` boundary

**Question:** What must be true for an `unsafe` operation to be sound, and where is that assumption validated?

**Prerequisites:** Basic Rust references, raw pointers, and error handling. **Mode:** read-only source review.

## Observe

Read [the code review standard](../CODE_REVIEW_STANDARD.md) and the [unsafe boundary inventory](../../rust-kernel/UNSAFE_BOUNDARIES.md). Locate one small `unsafe` block in the current Rust source. For a manageable starting point, inspect [`boot_addresses.rs::load`](../../rust-kernel/src/boot_addresses.rs): it reads Limine response pointers via `read_volatile`. The surrounding checks reject null responses, but a nonnull pointer is not thereby proven valid. Avoid changing privileged code as part of this module.

## Explain

Write a compact review record:

| Question | Evidence to capture |
| --- | --- |
| What operation requires `unsafe`? | Link to the precise source function or block |
| What is the safety precondition? | Paraphrase its `SAFETY:` rationale, if present |
| Who establishes the precondition? | Link to validation or caller obligations |
| What could go wrong? | One concrete invalid pointer, aliasing, alignment, lifetime, or CPU-state scenario |
| How is failure handled? | Link to an actual guard or test; otherwise say **not established** |

**Example review observation:** `boot_addresses.rs::load` relies on valid mapped Limine response pointers; checking for null cannot prove that an arbitrary nonnull address is dereferenceable. Distinguish that handoff assumption from a runtime validation that does not exist.

A `SAFETY:` comment is an explanation, **not a proof**. Distinguish an assumption from a runtime check and from an invariant tested under QEMU.

## Critique

Try to falsify the safety argument. Ask whether malformed bootloader input, unexpected CPU state, integer overflow, or an invalid address could reach the operation. Choose only scenarios relevant to the code you actually inspected.

## Contribute

If you find a missing explanation or a specific unchecked precondition, first search open issues and PRs for an existing discussion. Post a respectful, source-linked review with expected and actual behavior. For sensitive vulnerability details, follow [SECURITY](../../SECURITY.md) rather than a public issue.

**Completion evidence:** one evidence-linked review, one plausible failure mode, and a clear account of what was verified versus not verified. A review need not result in an approval.
