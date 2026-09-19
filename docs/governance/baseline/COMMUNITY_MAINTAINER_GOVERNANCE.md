# Delta Community & Maintainer Governance

**Version:** v1.0
**Status:** Governance Baseline — Final / Frozen
**Scope:** Governance of the Delta open-source community, maintainer roles, decision-making processes, and conflict resolution for `fongap/delta`.

---

## 1. Purpose

This document establishes the governance model for the Delta open-source project, defining roles, responsibilities, decision-making authority, and processes for community participation and leadership.

> Complements **Contribution Governance** (operational PR/review process) with **strategic/people governance**.

---

## 2. Governance Principles

| Principle | Description |
|-----------|-------------|
| **Meritocracy with Guardrails** | Influence earned through contribution; bounded by Architecture Governance. |
| **Transparent Decisions** | Public rationale for architectural/strategic decisions. |
| **Inclusive but Decisive** | Broad input; clear ownership for final decisions. |
| **Architecture Supremacy** | Frozen Architecture Governance (v1.0) cannot be overridden by community vote. |
| **Commercial/Community Boundary** | Suite direction set by Delta (fongap); Foundation direction collaborative. |

---

## 3. Roles & Responsibilities

### 3.1 Role Definitions

| Role | Scope | Selection | Term |
|------|-------|-----------|------|
| **Project Lead** | Overall project health; final tie-breaker; represents Delta to external orgs | Appointed by fongap (founder) | Indefinite |
| **Architecture Review Board (ARB)** | Architecture Governance enforcement; ADR decisions; security architecture | Project Lead + 2-4 Maintainers (invited) | Indefinite |
| **Maintainer** | Merge rights; code review; module ownership (CODEOWNERS); release management | ARB nomination + consensus | 2 years, renewable |
| **Security Lead** | Vulnerability response; security architecture; kill-switch authority | Project Lead appointment | Indefinite |
| **Release Manager** | Release execution; artifact signing; rollout coordination | Rotating among Maintainers | 2 releases (~4 months) |
| **Community Moderator** | Code of Conduct enforcement; forum/discord moderation | ARB appointment | 1 year, renewable |
| **Contributor** | Anyone with merged PR | Automatic (merged PR) | Ongoing |

### 3.2 Role Boundaries

| Decision | Authority | Input Required |
|----------|-----------|----------------|
| **Architecture Governance changes** | ARB (unanimous) | Community discussion (14 days) |
| **New top-level component** | ARB | ADR + implementation plan |
| **Public contract breaking change** | ARB | Deprecation period (2 minors) |
| **Maintainer appointment/removal** | ARB | Maintainer consensus |
| **Security patch release** | Security Lead + Release Manager | ARB notification |
| **Code of Conduct enforcement** | Community Moderators | Appeal to ARB |
| **Trademark license approval** | Project Lead | Legal review |
| **Suite strategy/direction** | fongap (commercial) | N/A (private) |

---

## 4. Decision-Making Processes

### 4.1 Decision Types

| Type | Examples | Process |
|------|----------|---------|
| **Type 1: Architecture** | New component, contract change, authority model | ADR → ARB unanimous → Implementation |
| **Type 2: Strategic** | Release cadence, support window, LTS policy | Proposal → Maintainer consensus → ARB ratify |
| **Type 3: Operational** | CI config, dependency updates, bug triage | Maintainer discretion |
| **Type 4: Community** | CoC enforcement, event sponsorship, swag | Moderator team → ARB if contested |

### 4.2 Consensus Model

**Modified Consensus (for Type 1 & 2):**
1. Proposal published (GitHub Discussion + `governance` label).
2. **14-day discussion period** minimum.
3. **Consensus = No sustained objection from Maintainers.**
   - Objection must be: specific, architecture-referenced, actionable.
   - "I don't like it" ≠ objection.
4. If objection sustained → ARB mediates → ARB decides (unanimous required for Type 1).
5. Decision recorded in Governance Log (`GOVERNANCE_LOG.md`).

### 4.3 Emergency Decisions

| Trigger | Process | Post-Action |
|---------|---------|-------------|
| Critical vulnerability | Security Lead authorizes → patch → release | ARB review within 48h |
| Trademark infringement | Project Lead authorizes → legal action | ARB notification |
| Infrastructure outage | Release Manager + Maintainer on-call | Postmortem within 5 days |

---

## 5. Maintainer Lifecycle

### 5.1 Becoming a Maintainer

**Criteria (all required):**
- ≥ 10 merged PRs (non-trivial) across ≥ 2 modules.
- ≥ 6 months consistent activity.
- Demonstrated architecture understanding (ADR participation or review).
- Signed CLA + good CoC standing.
- Nominated by 2+ current Maintainers.

**Process:**
1. Nomination issue (`governance` label) with evidence.
2. 14-day community comment period.
3. ARB vote (unanimous).
4. Onboarding: `CODEOWNERS` update, merge rights, signing keys, mentor assigned (3 months).

### 5.2 Maintainer Expectations

| Expectation | Minimum |
|-------------|---------|
| **Review load** | 2 PRs/week (average) |
| **Architecture participation** | Attend ARB meetings (bi-weekly) or async review |
| **Release rotation** | Serve as Release Manager once per 2-year term |
| **Security awareness** | Complete annual secure code review training |
| **Community health** | Mentor 1+ new contributor per year |

### 5.3 Maintainer Emeritus / Removal

| Scenario | Process |
|----------|---------|
| **Voluntary step-down** | Notify ARB → Emeritus status (honorary, no merge rights) |
| **Inactivity (6 months)** | ARB notice → 30-day grace → Emeritus if no response |
| **CoC violation** | Moderator investigation → ARB vote (supermajority 2/3) → Removal |
| **Architecture violation** | ARB investigation → ARB vote (unanimous) → Removal |

> **Removal is rare.** Focus on coaching first. Emeritus retain community recognition.

---

## 6. Community Participation

### 6.1 Channels

| Channel | Purpose | Governance |
|---------|---------|------------|
| **GitHub Discussions** | Design proposals, Q&A, announcements | ARB monitors; `governance` label for decisions |
| **GitHub Issues** | Bug reports, feature requests | Triage by Maintainers |
| **Discord / Matrix** | Real-time chat, support, social | Moderators enforce CoC |
| **Mailing List** (`community@delta.example.com`) | Formal announcements, security advisories | Read-only for most; ARB posts |
| **Bi-weekly Community Call** | Sync, demos, Q&A | Open; notes published |

### 6.2 Contributor Recognition

| Tier | Criteria | Recognition |
|------|----------|-------------|
| **Contributor** | 1+ merged PR | `CONTRIBUTORS.md`; release notes mention |
| **Active Contributor** | 5+ PRs/year; helps review | Discord role; swag eligibility |
| **Core Contributor** | 10+ PRs/year; module expertise | Maintainer nomination fast-track; direct ARB access |
| **Hall of Fame** | Sustained impact (years) | Annual blog post; permanent credits |

---

## 7. Conflict Resolution

### 7.1 Technical Disputes

1. **Discuss in PR/Issue** → reference Architecture Governance.
2. **Escalate to ARB** if unresolved in 5 business days.
3. **ARB Decision** → recorded in ADR or Governance Log.
4. **Appeal** → Project Lead (final).

### 7.2 Governance Disputes

1. **Mediation** by neutral Maintainer.
2. **ARB Hearing** (both parties present).
3. **ARB Decision** (unanimous) → binding.
4. **Final Appeal** → Project Lead.

### 7.3 Commercial vs. Community Tension

| Situation | Resolution |
|-----------|------------|
| Community wants Suite feature in Foundation | ARB evaluates: if architectural boundary violated → stays in Suite |
| Suite change breaks Foundation compatibility | ARB blocks Suite release until compatible |
| Contributor builds Suite-parity feature in Foundation | ARB evaluates: if blurs commercial boundary → redirect to Suite or reject |

> **Architecture Governance is the tie-breaker.** Commercial differentiation is a valid architectural concern.

---

## 8. Funding & Sustainability

### 8.1 Project Funding Sources

| Source | Use | Governance |
|--------|-----|------------|
| **fongap Commercial Revenue** | Core team salaries, infrastructure, security audits | fongap controls; transparency report annually |
| **GitHub Sponsors / Open Collective** | Community events, bounties, swag | Community Moderators allocate; ARB oversight |
| **Enterprise Support Contracts** | Dedicated engineering, SLA | fongap sales; no community governance |

### 8.2 Financial Transparency

- **Annual Transparency Report:** Revenue sources, expense categories, reserve status.
- **No individual salaries disclosed.**
- **Community funds** tracked in Open Collective (public ledger).

---

## 9. Governance Evolution

### 9.1 Amending This Document

- **Type 1 (Architecture-linked):** ARB unanimous + 30-day community notice.
- **Type 2 (Process):** Maintainer consensus + ARB ratify.
- **Type 3 (Administrative):** Project Lead + 1 Maintainer.

### 9.2 Governance Log

All decisions recorded in `GOVERNANCE_LOG.md`:
```markdown
## YYYY-MM-DD: Decision Title
**Type:** 1/2/3/4
**Proposal:** #discussion-number
**Decision:** Accepted / Rejected / Modified
**Rationale:** <link to Architecture Governance section>
**Votes:** Maintainer1 ✅, Maintainer2 ✅, Maintainer3 ❌ (objection: ...)
**ARB:** Lead ✅, Member1 ✅, Member2 ✅
```

---

## 10. Version History

| Version | Date | Author | Change |
|---------|------|--------|--------|
| v1.0 | 2026-09-17 | Project Lead + ARB | Initial baseline |

---

**End of Document**