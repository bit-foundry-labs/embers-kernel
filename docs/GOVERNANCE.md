# Project governance (initial)

**Steward:** Bit Foundry Labs. Embers Kernel is currently an early-stage open-source systems research project; it has no formal steering committee or guaranteed release schedule.

- Maintainers review and merge PRs. Passing CI is necessary where applicable but not sufficient for merging kernel/security changes.
- Architectural decisions are discussed in issues or pull requests and documented with an ADR; proposals remain proposals until a maintainer explicitly adopts them.
- Security-critical changes require failure-path testing and written assumptions. Claims of mathematical guarantees require a proof with a stated scope.
- Contributors retain attribution according to license and platform conventions. Participation does not create a promise of employment, compensation, publications, or future governance rights.
- The Apache-2.0 license permits commercial reuse subject to its terms; the **Embers Kernel** name/visual identity is not automatically licensed as a trademark by the code license.
- Decisions about code ownership or licensing changes should be transparent and discussed before any policy is changed.

This lightweight governance document should evolve as the actual maintainer/contributor community grows.

## Community roles and appointment

The current project steward is **Bit Foundry Labs**. Community reviewer and additional maintainer roles are **future appointments**, not memberships automatically conveyed by opening an issue, making a PR, receiving a compliment, or performing a single review.

The [community roles guide](CONTRIBUTOR_ROLES.md) defines each role's responsibilities, how to express interest, evidence of sustained competence, nomination and appointment steps, scope and duration of authority, recusal, and orderly removal. Maintainers decide appointments transparently in the repository where feasible, with candidate consent. Public technical feedback remains welcome without a role designation.

**Access must follow responsibility:** A recognized community reviewer receives no automatic collaborator, Write, Admin, secret, deployment, or branch-rule bypass permissions. Any future maintainer access is a separate, explicitly approved repository administration action, ordinarily limited to the least GitHub role needed. It does not grant authority over Bit Foundry Labs accounts or services. Use the [main-branch protection guide](BRANCH_PROTECTION.md) for review and CI gates; publishing these docs does not activate GitHub settings.
