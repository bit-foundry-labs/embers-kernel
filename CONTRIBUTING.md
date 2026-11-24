# Contributing to Embers Kernel

The best first contribution is small enough to explain, test, and review.

New to kernel engineering? The [Learning Lab](docs/learning/README.md) offers three source-linked exercises you can start in a browser. Report precisely what you reviewed or tested; the exercises do not grant GitHub permissions.

## Before starting

1. Read [the README](README.md), [project roadmap](docs/CONTRIBUTOR_ROADMAP.md), and [Code of Conduct](CODE_OF_CONDUCT.md).
2. Check whether someone is already working on the issue. Comment on an open issue with your intended approach before beginning a larger change.
3. Prefer one observable behavior per pull request. For security-critical APIs, start with tests and a written threat assumption.
4. Do not claim mathematical guarantees without a precisely scoped proof, benchmark, or test.

The default branch contains experimental Rust `no_std` x86-64 boot, GDT/TSS/IDT exception handling, Limine memory-map validation and symbolic frame-owner accounting (M02–M05). Follow the headless QEMU and host-test commands in [README](README.md). Those are **not** ring-3 isolation or capability enforcement. **PR #2** preserves the historical C bootstrap; **PR #3** proposes architecture, and the host-tested policy experiment from **PR #6** is now on `main` in `experiments/rust-policy-guard` (not privileged kernel code).

## Choose a contribution path

- **Documentation and research:** Fix reproducibility gaps, clarify hardware assumptions, check technical references, improve the contributor experience.
- **Kernel engineering:** Small boot/exception/paging milestones with QEMU output and negative tests.
- **Rust experiments:** Host-testable, no_std-compatible libraries for capabilities, policy validation, resource budgets, simulation and property tests.

If changing APIs, provide the invariant being enforced, the trust boundary, failure behavior and how it is tested.

## Grow into a reviewer or maintainer

You do not need to be a code author to contribute. Start by reviewing an open PR: identify an invariant, reproduce a test, inspect a failure path, or explain which hardware assumptions remain unverified.

Our [community role guide](docs/CONTRIBUTOR_ROLES.md) explains the distinction between public contributors, recognized reviewers, and future maintainers; the evidence we look for; how nominations work; and why review recognition does not grant GitHub write access. To express interest, [open a community-role issue](https://github.com/bit-foundry-labs/embers-kernel/issues/new?template=role_interest.md) and link previous contributions. You may also simply comment on a PR.

Maintainer appointments are **not open grants**. They require sustained technical contributions, an explicit invitation and agreement, appropriate security safeguards, and a recorded governance decision. We are not currently appointing outside maintainers by default.

## Practical workflow

1. Start with an [open issue](https://github.com/bit-foundry-labs/embers-kernel/issues), ideally one with a reproducible expected result. Fork the repository, create a feature branch, and reference the issue.
2. Reproduce the Rust kernel baseline from `main` using `make rust-iso`, headless QEMU and the applicable `make rust-memory-test` / `make rust-ledger-test` commands. For fault work, see `rust-kernel/M03_EXCEPTIONS.md`. The C bootstrap in PR #2 is historical reference code; the host-only policy experiment from PR #6 is now on `main`.
3. Implement one scoped change. Add or update tests and concise documentation.
4. Open a pull request with: problem, approach, limitations, tests run, QEMU log if relevant, and any security impact.
5. Keep communication respectful. Maintainers may request revisions, defer work, or reject incompatible designs.

Do not change unrelated code, disable failing tests, commit model credentials, include proprietary model weights, or add network/API spending as an undocumented dependency.

## Your first pull request: a short walkthrough

1. **Pick an open, bounded task.** Try the [Learning Lab boot-path exercise](docs/learning/01-boot-path.md) and coordinate on [#89](https://github.com/bit-foundry-labs/embers-kernel/issues/89) if you want to contribute: `archweave` has already volunteered to lead its requested documentation PR. Offer source-linked review help rather than duplicating that work. [Issue #8](https://github.com/bit-foundry-labs/embers-kernel/issues/8) is complete after merged [PR #87](https://github.com/bit-foundry-labs/embers-kernel/pull/87); its [Ubuntu/QEMU reproduction evidence](rust-kernel/README.md#ubuntu-2604-reproduction-record-issue-8) is a reference, not an open task. Check the [current issues](https://github.com/bit-foundry-labs/embers-kernel/issues) for unclaimed work. Comment on your selected issue before starting.
2. **Fork and branch from current `main`.** Follow the [README setup and headless QEMU instructions](README.md#boot-embers-in-headless-qemu) if your task requires execution. Do not run the kernel on physical hardware.
3. **Make one reviewable change.** For boot-path documentation or review, link the actual functions and distinguish observed QEMU behavior from unimplemented isolation. Coordinate with the contributor leading #89 before proposing edits to that map; avoid unrelated refactoring.
4. **Report evidence precisely.** Run relevant commands if your environment supports them, recording commands and results. Otherwise write **“Documentation-only; tests not run.”** Never imply that CI, QEMU, or hardware tests ran locally when they did not.
5. **Open the PR and respond to review.** Link the issue, summarize the change and limitations, complete the PR template, and address reviewer feedback on the same branch. Maintainers verify required CI and decide whether to merge.

## Run the right checks before opening a PR

From the repository root, on Ubuntu or WSL with the pinned Rust toolchain
installed (see [Rust kernel setup](rust-kernel/README.md)). Host-only tests do not require QEMU or an ISO; boot/exception tests also require QEMU, `xorriso`, and the toolchain prerequisites documented in the setup guide:

| Change type | Start with | Additional evidence |
| --- | --- | --- |
| Documentation only | Review Markdown links and run `git diff --check` | GitHub Source hygiene check |
| Rust source, formatting, or build | `make rust-check` | `cd rust-kernel && cargo fmt --all -- --check && cargo clippy --release --offline -- -D warnings` |
| Boot memory selection | `make rust-memory-test` | [M04/M05 memory workflow](.github/workflows/rust-memory.yml) |
| Owner ledger or reservation registry | `make rust-ledger-test` and `make rust-frame-registry-test` | [M04/M05 memory workflow](.github/workflows/rust-memory.yml); QEMU ownership smoke for feature-gated changes |
| Limine address metadata or boot-pool gate | `make rust-provenance-test` and `make rust-boot-pool-gate-test` | [Rust boot](.github/workflows/rust-boot.yml) and [M04 memory](.github/workflows/rust-memory.yml) workflows |
| Paging research models | `make rust-paging-research-test` and `make rust-pte-fields-test` | [M04 memory host fixtures](.github/workflows/rust-memory.yml) and [Rust boot QEMU preflight](.github/workflows/rust-boot.yml) when privileged code changes |
| Serial diagnostics | `make rust-serial-test` | QEMU READY/fault serial logs if boot output changes |
| Exception, privileged CPU, or fault-injection code | `make rust-iso` | [Rust boot](.github/workflows/rust-boot.yml) and [M03 exceptions](.github/workflows/rust-exceptions.yml) workflows |

These are **starting points**, not a substitute for the full CI suite. Host
tests run model or bookkeeping logic; QEMU exercises selected emulator paths.
Neither establishes production hardware safety or memory isolation.

**Example:** If you change `rust-kernel/src/frame_registry.rs`, start with `make rust-frame-registry-test`; the M04 workflow also exercises frame-registry host tests and QEMU memory diagnostics. If you did not run QEMU locally, report that explicitly rather than presenting CI results as your own.

**When a check fails:** include the exact command, first relevant error,
toolchain/host environment, and whether the failure reproduces on unmodified
`main`. Distinguish a build/test assertion failure from dependency download
or runner setup failures. Never disable a failing test to make a PR green.

**When a check cannot run:** say so in the PR. Mark it *not run*, explain the
blocker, and link any relevant CI job. Do not report CI results as local tests.

## Professional communication and technical writing

Write all contributor-facing prose in **clear, complete, grammatically correct sentences**. This standard applies to README files, architecture and research notes, issue descriptions, pull-request descriptions, review discussions, commit messages where prose is used, Rust documentation comments, and explanatory inline code comments.

- Use a professional, respectful, evidence-based tone. Avoid slang, sarcasm, insults, dismissive language, and exaggerated claims about correctness or performance.
- Explain technical decisions precisely. State the assumption, invariant, reason for a non-obvious operation, and known limitations when they affect correctness or safety.
- Prefer concise sentences and established Rust and operating-systems terminology. Define project-specific abbreviations at first use; proofread grammar, punctuation, spelling, and links before requesting review.
- Write explanatory code comments as complete sentences with appropriate capitalization and terminal punctuation. Headings, identifiers, API names, register names, diagnostic strings, table entries, and code examples are not required to be sentences.
- Comment **why** a mechanism or boundary exists, rather than narrating self-evident code. Keep comments accurate when behavior changes; do not introduce decorative or redundant commentary.
- Document every `unsafe` block with a specific `SAFETY:` rationale. Document the caller's obligations for `unsafe fn` interfaces; do not replace actual proof obligations with confident language.
- Report tests exactly as executed. Distinguish host-model behavior, QEMU evidence, and unverified hardware assumptions. Do not invent test results, security guarantees, or citations.

Reviewers should request revisions for unclear or unprofessional writing just as they would for ambiguous APIs. This is a quality standard, not a requirement for contributors to be native English speakers; constructive language assistance and editorial corrections are welcome.

## Merge readiness

Before a change to `main` is merged, check the `whitespace`,
`rust-boot`, `exceptions`, and `memory` GitHub Actions jobs.
All four run on every pull request, including documentation-only changes.
A passing emulator check does not imply hardware isolation or formal proof.

The [main branch protection guide](docs/BRANCH_PROTECTION.md) documents
the repository administrator's required rule settings and review policy.
CI passing and an active protection ruleset are separate verification steps.

## Review checklist

- [ ] I linked the relevant issue or ADR.
- [ ] I described the behavior and relevant threat assumptions.
- [ ] I included a reproducible test or explain why testing is blocked.
- [ ] I differentiated simulation from real kernel behavior and hardware-specific guarantees.
- [ ] No secrets, third-party licensing violations, or unexpected downloads.
- [ ] I verified build/test behavior on the specific branch I changed.
- [ ] I proofread prose and explanatory code comments for complete sentences, correct grammar, professional tone, and technical accuracy.

## Compatibility, contribution rights, and credit

Apache-2.0 applies to original code contributed to this repository under its license terms. Ensure you have the right to contribute your work; identify third-party material and license requirements. We currently do not require a separate contributor license agreement. Attribution and academic authorship are based on actual contributions and applicable norms, not guaranteed by opening a pull request.

For substantial architecture changes, propose an ADR or discuss the issue before implementation.
