# Delta Contribution & CLA Governance

**Version:** v1.0
**Status:** Governance Baseline — Final / Frozen
**Scope:** All contributions to the public `fongap/delta` Foundation repository.

---

## 1. Purpose

This document defines the contribution lifecycle, reviewer responsibilities, merge criteria, and Contributor License Agreement (CLA) requirements for the Delta Foundation repository.

> Referenced by **Delta Dual-Repo Design v1.0 Section 21** and **Delta Source & Licensing Governance v1.0 Section 5**.

---

## 2. Contribution Principles

| Principle | Description |
|-----------|-------------|
| **Open to All** | Anyone may contribute; no invitation required. |
| **Merit-Based** | Decisions based on technical merit, not affiliation. |
| **Architecture-First** | All changes must respect frozen architecture boundaries (Architecture Governance v1.0). |
| **Contract-Stable** | Public contracts (SDK, schemas) evolve slowly; breaking changes require formal review. |
| **Security-Conscious** | Every change evaluated for trust boundary impact. |
| **Documented** | Architecture-relevant changes require ADR or design doc. |

---

## 3. Contribution Types & Requirements

### 3.1 Contribution Categories

| Category | Examples | Review Bar | CLA Required |
|----------|----------|------------|--------------|
| **Trivial** | Typos, formatting, comment fixes, doc links | Single maintainer approval | No (but GitHub ToS covers) |
| **Code Fix** | Bug fixes, test additions, refactoring within module | 1+ maintainer; CI pass | Yes |
| **Feature** | New capability, worker, connector, skill | 2+ maintainers; ADR if architectural | Yes |
| **Architecture** | Core module changes, contract changes, new top-level component | Architecture Review Board; ADR mandatory | Yes |
| **Security** | Vulnerability fixes, trust boundary changes | Security team + 1 maintainer; expedited | Yes |
| **Dependency** | Adding/upgrading third-party crates/packages | License check + 1 maintainer | Yes |

### 3.2 CLA Enforcement Matrix

| Contributor Type | CLA Mechanism | Enforcement |
|------------------|---------------|-------------|
| Individual (first-time) | Auto-prompt via `cla-assistant` on PR | Merge blocked until signed |
| Individual (repeat) | CLA recorded in GitHub org | Auto-verified |
| Corporate Employee | Corporate CLA signed by authorized officer | Verified via email domain + CLA database |
| Bot/Automation | Exempt (no IP contribution) | Labeled `bot`; no CLA check |

> **No CLA = No Merge.** Exceptions only for trivial non-code changes (typo/docs) where GitHub ToS §D.6 applies.

---

## 4. Contribution Lifecycle

### 4.1 Standard Flow

```text
Issue/Discussion
      ↓
Design Doc / ADR (if architectural)
      ↓
Fork → Branch → Commits (signed-off)
      ↓
Pull Request → CI (lint, test, contract, arch-guard, license)
      ↓
Review (maintainers + arch review if needed)
      ↓
Approval → Squash Merge to main
      ↓
Release Train (see Release Governance)
```

### 4.2 Branch & Commit Conventions

- **Branch naming:** `type/scope-short-desc` (e.g., `feat/connector-gmail`, `fix/core-trust-timeout`, `arch/adr-004-capability-registry`)
- **Commit messages:** Conventional Commits (e.g., `feat(core): add execution grant validation`)
- **Signed commits:** `git commit -s` (Developer Certificate of Origin) **in addition to CLA** — DCO provides provenance; CLA provides license grant.

### 4.3 Pull Request Requirements

| Check | Required For | Tool |
|-------|--------------|------|
| CI Pass (lint, unit, contract, arch-guard) | All PRs | GitHub Actions |
| CLA Signed | All code PRs | `cla-assistant` |
| Architecture Review | Architectural PRs | Architecture Review Board |
| Security Review | Security-relevant PRs | Security Team |
| License Check | New dependencies | `cargo-deny` / `license-checker` |
| Changelog Entry | User-visible changes | Manual (PR template) |
| Documentation Updated | Public API/contract changes | Manual (PR template) |

---

## 5. Reviewer & Maintainer Model

### 5.1 Roles

| Role | Scope | Appointment |
|------|-------|-------------|
| **Code Owner** | Specific crate/module (per `CODEOWNERS`) | Architecture Review Board |
| **Maintainer** | Cross-module review authority; merge rights | Architecture Review Board |
| **Architecture Review Board (ARB)** | Architectural changes, contract changes, top-level components | Delta Core Team (founding members) |
| **Security Team** | Security-relevant changes, vulnerability handling | Delta Core Team + external auditors |
| **Release Manager** | Release branching, tagging, artifact signing | Rotating among Maintainers |

### 5.2 Review SLAs

| PR Type | Initial Response | Review Completion |
|---------|------------------|-------------------|
| Trivial | 1 business day | 3 business days |
| Code Fix / Dependency | 2 business days | 5 business days |
| Feature | 3 business days | 10 business days |
| Architecture | 5 business days | 20 business days (may require ADR iteration) |
| Security (Critical) | 4 hours | 24 hours |

> SLAs are targets, not guarantees. Community contributors are not bound by SLAs.

### 5.3 Maintainer Onboarding / Offboarding

- **Onboarding:** Nomination by 2+ Maintainers → ARB approval → `CODEOWNERS` update → onboarding checklist.
- **Offboarding:** Voluntary resignation or 6 months inactivity → ARB acknowledgment → `CODEOWNERS` removal.
- **Emergency Removal:** Security/conduct violation → ARB unanimous decision → immediate.

---

## 6. Architecture Change Process

### 6.1 What Requires Architecture Review

Any change affecting:
- Top-level component list (Architecture Governance §3)
- Core internal security domains (§12)
- Authority model (§13)
- Public contracts (Contract Governance)
- Capability model (Capability Model)
- Runtime/Orchestrator structure (§16)
- Trust/Sandbox boundaries (§34-36)

### 6.2 Architecture Decision Record (ADR) Process

1. **Propose:** Open GitHub Discussion with `adr-proposal` label.
2. **Draft:** Author writes ADR in `docs/adr/NNNN-title.md` (next number).
3. **Review:** ARB + Maintainers comment; 14-day minimum discussion.
4. **Decision:** ARB records decision (Accepted / Rejected / Deferred) in ADR header.
5. **Implement:** Accepted ADRs tracked as implementation issues.
6. **Archive:** Superseded ADRs marked `Superseded by ADR-NNNN`.

### 6.3 ADR Template

```markdown
# ADR-NNNN: <Title>

**Status:** Proposed / Accepted / Rejected / Superseded
**Date:** YYYY-MM-DD
**Authors:** @github-handles
**Reviewers:** @github-handles

## Context
<Why this decision is needed; link to issues>

## Decision
<What we will do>

## Consequences
### Positive
- ...
### Negative
- ...
### Risks
- ...

## Alternatives Considered
- <Alternative 1>: <Why rejected>
- <Alternative 2>: <Why rejected>

## References
- Architecture Governance §X.Y
- Related ADRs: NNNN, MMMM
```

---

## 7. Security Contribution Process

### 7.1 Vulnerability Reporting

- **Public Issues:** Non-security bugs only.
- **Security Issues:** Email `security@delta.example.com` (or GitHub Security Advisories).
- **No Public Disclosure** until fix released + 7-day grace period.

### 7.2 Security Fix Workflow

```text
Security Report
      ↓
Security Team Triage (≤ 24h)
      ↓
Private Fork / Branch (maintainers only)
      ↓
Fix + Regression Tests
      ↓
Security Review (Security Team)
      ↓
Coordinated Release (patch version)
      ↓
Public Advisory (GitHub Security Advisory + CVE)
```

### 7.3 Security Reviewer Requirements

- Security Team members must complete secure code review training.
- All Core/Trust/Runtime changes require Security Team sign-off.
- Security fixes in optional extensions must not delay Foundation fixes.

---

## 8. Community Conduct

### 8.1 Code of Conduct

Delta adopts the **Contributor Covenant v2.1** (https://www.contributor-covenant.org/version/2/1/code_of_conduct/).

### 8.2 Enforcement

| Channel | Scope |
|---------|-------|
| GitHub Discussions / Issues / PRs | Public community |
| Delta Discord / Matrix | Real-time chat |
| Private reports | `conduct@delta.example.com` |

**Enforcement Body:** Community Moderation Team (3+ Maintainers, rotated quarterly).
**Appeals:** Architecture Review Board.

---

## 9. Recognition & Attribution

### 9.1 Contributor Recognition

- **CONTRIBUTORS.md:** Auto-generated from git history (all merged PR authors).
- **Release Notes:** Notable contributors highlighted per release.
- **Hall of Fame:** Annual recognition for sustained contribution.

### 9.2 Attribution in Artifacts

- Binary `About` dialog includes "Contributors" link to `CONTRIBUTORS.md`.
- No individual names in binary metadata (privacy).

---

## 10. Corporate Contribution Policy

### 10.1 Corporate CLA

Organizations employing 5+ contributors **must** sign a Corporate CLA covering all current/future employees.

### 10.2 Employee Contributions

- Employees of CLA-signed orgs: Covered by Corporate CLA.
- Employees of non-CLA-signed orgs: Individual CLA required.
- Work-time contributions: Employer must confirm IP ownership via Corporate CLA.

### 10.3 Competitive Contributions

Contributions from direct competitors (as defined by ARB) are **welcome** but:
- Must not introduce Suite-parity features that undermine commercial differentiation.
- Architecture review may reject features that blur Foundation/Suite boundary.
- This is a **governance boundary**, not a license restriction (Apache-2.0 permits all use).

---

## 11. AI-Assisted Contributions

### 11.1 Policy

AI-generated code is **permitted** if:
- Human author reviews, tests, and understands the code.
- Human author signs the commit (`-s`) and CLA.
- No copyrighted/non-permissive code is inadvertently included.

### 11.2 Prohibited

- Submitting AI-generated code without review.
- Using AI to bypass architecture review (e.g., "AI wrote this ADR").
- Training data contamination (copied GPL code from AI suggestions).

---

## 12. Enforcement & Escalation

| Issue | First Step | Escalation |
|-------|------------|------------|
| CLA not signed | Bot comment + 14-day timeout | PR closed; reopen after CLA |
| Architecture violation | Maintainer comment + `arch-violation` label | ARB review; revert if merged |
| Conduct violation | Moderator warning | Temporary ban → Permanent ban |
| License violation | Legal notice | DMCA / injunction |
| Security bypass | Immediate revert | Security Team audit |

---

## 13. Version History

| Version | Date | Author | Change |
|---------|------|--------|--------|
| v1.0 | 2026-09-17 | Architecture Review Board | Initial baseline |

---

**End of Document**