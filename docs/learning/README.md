# Embers Kernel Learning Lab

Learn systems engineering by investigating **the real Embers source**, reproducing evidence, and contributing small, reviewable findings. This is a guided entry point, not a separate toy kernel or a certification.

## Start here if you're new

**[Your first 15 minutes](00-start-here.md)** is a complete browser-only exercise. Open three real Rust functions, make one evidence-backed claim, and check your explanation. No tools to install, no pull request to submit. Keep the [plain-language glossary](glossary.md) nearby when terminology gets unfamiliar.

Completing an exercise is enough; contributing is a separate, optional next step.

## Choose a starting point

| Module | Prerequisites | You will produce | First contribution |
| --- | --- | --- | --- |
| [00 — First 15 minutes](00-start-here.md) | Browser only; no Rust required | A three-function source trace and one correctly bounded claim | None required; learning is a complete outcome |
| [01 — Trace a boot](01-boot-path.md) | Read Rust functions and GitHub diffs; QEMU optional | A source-linked boot sequence with an explicit evidence boundary | [Issue #89](https://github.com/bit-foundry-labs/embers-kernel/issues/89): `archweave` has volunteered; coordinate or offer review |
| [02 — Who owns a frame?](02-memory-ownership.md) | Basic Rust enums and unit tests | A claim/deny/quarantine trace and one test question | A small issue or review proposal; do not assume an open coding task |
| [03 — Review an unsafe boundary](03-unsafe-review.md) | Rust borrowing and pointer basics | A concise, evidence-linked review of one `unsafe` contract | Review a PR or propose a documentation correction |

Each module uses **observe → explain → test or inspect → critique → contribute**. You can complete the read-only portions in a browser. Running the kernel is optional for the first pass, and **physical-hardware boot is not supported**.

## Before contributing

1. Read [CONTRIBUTING](../../CONTRIBUTING.md), the [Code of Conduct](../../CODE_OF_CONDUCT.md), and the [research roadmap](../CONTRIBUTOR_ROADMAP.md).
2. Choose one module and follow its evidence exercise. Use the current `main` branch, since symbols and implementation details change.
3. Read the issue discussion before starting. An open issue may already have a volunteer; for example, `archweave` is leading #89. Coordinate or offer review rather than duplicating a contribution. For an untracked finding, discuss it before opening a large PR.
4. State exactly what you ran. If you only inspected code, say **“source review only; tests not run.”** A CI badge is not evidence that you ran tests locally.
5. Use the [review standard](../CODE_REVIEW_STANDARD.md) to make your finding actionable. Technical review is a valid contribution even without writing kernel code.

## Check your evidence

Before sharing an exercise, ask: **Did I link the specific current source or test?** **Did I separate what I observed from what I assumed?** **Did I list exactly which commands I ran, or clearly say tests were not run?** A useful learning artifact answers all three.

## What Embers currently proves, and what it does not

Embers has a Rust `no_std` QEMU boot and experiments in exception handling, memory-map validation, and frame accounting. Some paging and reservation work may be present or proposed; verify status on `main` and in the linked tests. **Do not infer** production hardware safety, ring-3 isolation, working IPC, or enforced capability security from a boot marker or host test.

## Further reading

- [Operating Systems: Three Easy Pieces](https://pages.cs.wisc.edu/~remzi/OSTEP/), Remzi and Andrea Arpaci-Dusseau. Use address spaces, virtual memory, and concurrency chapters for concepts; this is not an Embers implementation specification.
- [Writing an OS in Rust](https://os.phil-opp.com/), Philipp Oppermann. Read the freestanding Rust, exceptions, and paging chapters for parallel examples. Its bootloader and code structure differ from Embers; **do not paste its implementation into this repository**.

## Growing with the project

A first contribution can be a reproducible defect report, a corrected explanation, a negative test, or an independent review. People interested in ongoing review can read [community roles](../CONTRIBUTOR_ROLES.md) and use the [community-role interest form](https://github.com/bit-foundry-labs/embers-kernel/issues/new?template=role_interest.md). Learning modules do not grant GitHub permissions or guarantee a maintainer role.
