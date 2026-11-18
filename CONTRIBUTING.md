# Contributing to Embers Kernel

The best first contribution is small enough to explain, test, and review.

## Before starting

1. Read [the README](README.md), [project roadmap](docs/CONTRIBUTOR_ROADMAP.md), and [Code of Conduct](CODE_OF_CONDUCT.md).
2. Check whether someone is already working on the issue. Comment on an open issue with your intended approach before beginning a larger change.
3. Prefer one observable behavior per pull request. For security-critical APIs, start with tests and a written threat assumption.
4. Do not claim mathematical guarantees without a precisely scoped proof, benchmark, or test.

The default branch contains experimental Rust `no_std` x86-64 boot, GDT/TSS/IDT exception handling, Limine memory-map validation and symbolic frame-owner accounting (M02–M05). Follow the headless QEMU and host-test commands in [README](README.md). Those are **not** ring-3 isolation or capability enforcement. **PR #2** preserves the historical C bootstrap; **PR #3** proposes architecture, and **PR #6** is an independent host-tested policy simulation.

## Your first hour with Embers

A useful starting point is [#89: document the normal Rust boot path](https://github.com/bit-foundry-labs/embers-kernel/issues/89). Read the current baseline and `rust-kernel/src/main.rs`, trace how Limine hands off to `kernel_entry()` and how the normal path reaches `READY`, then propose a short, source-linked map. You can do this without running QEMU or editing kernel code. The issue explains the acceptance criteria and how to claim it.

If you prefer testing, [#8: Ubuntu/WSL reproduction](https://github.com/bit-foundry-labs/embers-kernel/issues/8) already has [PR #87](https://github.com/bit-foundry-labs/embers-kernel/pull/87) under review; coordinate before duplicating it. Experienced reviewers can tackle [#84: M06 paging observations](https://github.com/bit-foundry-labs/embers-kernel/issues/84). Check that an issue is still open and unclaimed before beginning.

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
2. Reproduce the Rust kernel baseline from `main` using `make rust-iso`, headless QEMU and the applicable `make rust-memory-test` / `make rust-ledger-test` commands. For fault work, see `rust-kernel/M03_EXCEPTIONS.md`. The C bootstrap in PR #2 is historical reference code; the policy model in PR #6 remains a separate branch.
3. Implement one scoped change. Add or update tests and concise documentation.
4. Open a pull request with: problem, approach, limitations, tests run, QEMU log if relevant, and any security impact.
5. Keep communication respectful. Maintainers may request revisions, defer work, or reject incompatible designs.

Do not change unrelated code, disable failing tests, commit model credentials, include proprietary model weights, or add network/API spending as an undocumented dependency.

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
