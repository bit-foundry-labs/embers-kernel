# Community roles and path to maintainership

Embers Kernel is an experimental, open-source systems research project stewarded by **Bit Foundry Labs**. We want independent scrutiny of boot, memory safety, capability security, and later scheduling without giving privileged repository access to everyone who helps.

**Status:** This is a documented pathway for *future* appointments. No new community reviewers or external maintainers are appointed by this document. Roles are voluntary; they do not imply employment, compensation, funding, or academic authorship.

## What each role can do

| Role | How to participate | Responsibilities | Repository authority |
| --- | --- | --- | --- |
| Contributor | Anyone may discuss issues, review public PRs, fork code, and propose changes. | Provide reproducible evidence, respectful feedback, and accurate limitations. | No special permission; no merge or repository settings access. |
| Community reviewer (future recognition) | Volunteer and demonstrate careful technical reviews across different changes. | Challenge invariants, unsafe code, threat assumptions, negative tests, QEMU evidence, and unsupported claims. | Public technical role only; **no automatic collaborator, Write, Admin, or merge access**. |
| Maintainer (future appointment) | Receive and accept an explicit invitation after sustained contributions and a stewardship decision. | Triage, coordinate reviews, enforce security gates, integrate approved changes, and mentor contributors within an agreed scope. | Explicit, least-privilege GitHub access when needed, ordinarily **Write**, not Admin; protected-branch rules still apply. |

A public GitHub user can provide technical review feedback without repository write access. A **formal GitHub review request** or a required branch-protection approval may be subject to GitHub's collaborator and permission rules; a community-reviewer title does not change those rules.

## How to become a community reviewer

1. **Choose an area.** Examples include x86-64/QEMU, Rust `no_std`, unsafe/assembly audit, capability design, IPC, concurrency, verification, and build reproducibility. A broad skill set is not required.
2. **Review something public.** Comment on an open pull request or a bounded research issue. State the exact commit or file/line being reviewed, what you checked, and one failure mode or limitation that deserves attention. The [source code review standard](CODE_REVIEW_STANDARD.md) shows what good evidence looks like.
3. **Express interest.** Open a [community-role interest issue](https://github.com/bit-foundry-labs/embers-kernel/issues/new?template=role_interest.md), choose *reviewer*, and link representative public reviews or patches. You may ask for a small first review if you do not have prior contributions.
4. **Earn recognition through work.** Maintainers look for accurate, constructive reviews across more than one change, clarity about assumptions and test scope, and responsiveness when findings are challenged. There is **no fixed PR count, tenure requirement, automatic promotion, or guarantee of acceptance**.
5. **Receive a public decision.** With the volunteer's consent, a maintainer records recognition and a focus area in a governance/role-document PR or equivalent public record. Recognition is revocable and grants **no privileged repository access**.

Reviewers should say **“not established”** when evidence is insufficient. Rejecting an unsafe change, discovering a reproducible negative path, or refining a threat model counts as useful work. They do not have to approve code to contribute.

## How someone could later become a maintainer

Maintainer responsibilities are more demanding than reviewing. When the project needs additional coverage, an existing maintainer may invite a contributor who has demonstrated sustained technical judgment, dependable communication, review of others' changes, and competence in the relevant subsystem. A volunteer can register future interest through the same issue template, but a public application does **not** initiate an automatic access grant.

Before appointment, the steward should:

1. Discuss a defined area of responsibility, expected availability, conflict-of-interest handling, and the candidate's consent. Do not require publication of private identifying information.
2. Review representative code contributions, defect reports, adversarial tests, and review decisions; seek independent input where possible.
3. Record the decision and scope through a reviewed governance change. Record who made the decision and when; do not announce an appointment that has not been accepted.
4. Grant the **minimum** GitHub access required using a separate deliberate admin action, with account 2FA and applicable organization controls. Write access is not the same as Admin access or access to CI secrets; never grant owner/admin access by default.
5. Confirm branch protection, required CI, stale-review dismissal where configured, and no routine bypass. A new maintainer should not merge their own security-critical change without independent scrutiny.

A reviewer who remains focused on reviews is valuable; promotion to maintainer is not the only measure of contribution.

## Security, accountability, and changes in role

- **Review does not authorize merging.** The project steward retains final merge and governance authority until explicitly delegated. CI and QEMU successes are specific evidence, not hardware guarantees.
- **Disclose relevant conflicts.** If reviewing your own change or a change in which you have a material interest, request an independent reviewer. Critique technical evidence, not the contributor.
- **Use the correct reporting channel.** Do not post exploits, credentials, private account information, or sensitive traces in a public role-interest issue. Follow [SECURITY.md](../SECURITY.md) for vulnerability reporting.
- **Reevaluate authority.** A maintainer may step down, become inactive, or lose privileged access for a security or conduct concern. Revoke repository access separately, document the non-sensitive governance outcome, and preserve attribution; limit disclosure of private personnel details.
- **Keep roles and access auditable.** A role title alone never changes GitHub teams, collaborator privileges, protection rules, CI permissions, secrets, billing, or ownership of Bit Foundry Labs.

For current contributor expectations, use [CONTRIBUTING.md](../CONTRIBUTING.md), [GOVERNANCE.md](GOVERNANCE.md), [CODE_OF_CONDUCT.md](../CODE_OF_CONDUCT.md), and [BRANCH_PROTECTION.md](BRANCH_PROTECTION.md).
