# Delta Security & Vulnerability Response Governance

**Version:** v1.0
**Status:** Governance Baseline — Final / Frozen
**Scope:** All Delta repositories, distributions, managed services, and extension ecosystem.

---

## 1. Purpose

This document defines the security governance model, vulnerability response process, disclosure policy, and security architecture enforcement for Delta. It operationalizes the **Secure Efficiency** principle (Architecture Governance §2.2) and **Fail Closed** rules (§15).

---

## 2. Security Principles

| Principle | Operational Meaning |
|-----------|---------------------|
| **Fail Closed** | Uncertainty at critical boundaries → deny/stop (Arch Gov §15) |
| **Least Privilege** | Workers/Connectors/Skills receive minimal grants (§28, §34) |
| **Defense in Depth** | Sandbox + Grant + Trust + Approval + Ledger (§34-36) |
| **No Security by Obscurity** | Architecture public; security relies on design, not secrecy |
| **Transparency** | Vulnerabilities disclosed responsibly; fixes public for Foundation |
| **Supply Chain Integrity** | Signed artifacts, SBOM, dependency pinning, reproducible builds |
| **Local Authority** | Secret/Trust/Vault authority never leaves device Core (§40) |

---

## 3. Security Organization

### 3.1 Roles

| Role | Responsibility | Composition |
|------|----------------|-------------|
| **Security Lead** | Owns security strategy, incident command, vendor coordination | Delta Core Team (appointed) |
| **Security Team** | Triage, review, fix development, advisory publishing | 3+ engineers (Core + Suite) |
| **Architecture Review Board (ARB)** | Security architecture decisions, kill-switch authorization | See Contribution Governance |
| **External Auditors** | Periodic penetration testing, architecture review | Contracted firms |
| **Community Reporters** | Report vulnerabilities via coordinated disclosure | Anyone |

### 3.2 Security Team SLAs

| Activity | SLA |
|----------|-----|
| Initial triage (acknowledge report) | ≤ 24 hours |
| Severity classification | ≤ 48 hours |
| Fix development (Critical) | ≤ 7 days |
| Fix development (High) | ≤ 14 days |
| Fix development (Medium/Low) | ≤ 30 days / next release |
| Coordinated disclosure publication | Fix release + 7 days |

---

## 4. Vulnerability Reporting & Disclosure

### 4.1 Reporting Channels

| Channel | Use Case | Response |
|---------|----------|----------|
| `security@delta.example.com` | All vulnerabilities | Security Team (encrypted) |
| GitHub Security Advisories (Private) | Foundation repo vulnerabilities | Security Team |
| Internal ticketing | Suite/Managed service vulnerabilities | Security Team + Suite leads |
| Public Issues | **Never** for vulnerabilities | Closed + redirect |

### 4.2 Disclosure Policy

**Coordinated Disclosure (Default):**
1. Reporter submits via private channel.
2. Security Team acknowledges, classifies, develops fix.
3. Fix released to **all supported versions** (see Release Governance).
4. Public advisory published **7 days after fix release** (grace period for users to update).
5. CVE requested for all non-trivial vulnerabilities.

**Exceptions:**
- **Active Exploitation:** Immediate public advisory + emergency release (bypass grace period).
- **Suite/Managed Only:** Advisory limited to affected customers; Foundation unaffected.
- **Researcher Request:** Extended timeline (max 90 days) if fix complex; requires Security Lead approval.

### 4.3 No Retaliation

Delta will **not** pursue legal action against researchers who:
- Report via authorized channels.
- Allow reasonable time for fix.
- Do not access/exfiltrate user data.
- Do not degrade service for others.

---

## 5. Severity Classification

Adapted from **CVSS 4.0** with Delta-specific context:

| Severity | CVSS Range | Delta Criteria | Response |
|----------|------------|----------------|----------|
| **Critical** | 9.0-10.0 | Remote code execution via untrusted input; Trust boundary bypass; Secret exfiltration; SideEffect forgery | Emergency release; kill-switch if in Suite |
| **High** | 7.0-8.9 | Local privilege escalation; Sandbox escape; Approval bypass; Provenance forgery | Patch release within 7 days |
| **Medium** | 4.0-6.9 | Information leak (non-secret); DoS via resource exhaustion; Non-critical trust misclassification | Next minor release |
| **Low** | 0.1-3.9 | UI spoofing (non-security); Minor log leak; Non-exploitable edge case | Next release or backlog |

---

## 6. Supported Versions & Patch Policy

### 6.1 Support Window

| Version Type | Support Duration | Patches |
|--------------|------------------|---------|
| **Latest Stable (N)** | Until N+2 released | All severities |
| **Previous Stable (N-1)** | 6 months after N released | Critical + High only |
| **Older (N-2+)** | End-of-life | None (upgrade required) |
| **LTS (if designated)** | 24 months | Critical + High |

### 6.2 Patch Release Cadence

- **Critical:** Emergency patch within 24-72h of fix ready.
- **High:** Batched into weekly patch release (e.g., `1.4.1`, `1.4.2`).
- **Medium/Low:** Batched into next minor release (`1.5.0`).

### 6.3 Suite/Managed Patch Alignment

- Foundation patches **always** released first.
- Suite patches released **same day** or **next business day**.
- Managed service patches deployed **before** client releases (backend-first).

---

## 7. Security Architecture Enforcement

### 7.1 Current Foundation gates

Current automated controls include:

- CodeQL analysis.
- Python, npm, and Rust dependency advisory checks.
- License-policy validation.
- Architecture-boundary, repository-boundary, naming, and contract checks.
- Rust formatting, compilation, Clippy, and tests.
- Python static checks and tests.
- Desktop TypeScript/unit/E2E checks.
- Release SBOM generation and GitHub artifact provenance attestation.

The repository does **not** claim a merge-blocking gitleaks/trufflehog gate,
cosign release signing, or reproducible-build verification unless those controls
are present in the active workflows.

### 7.2 Current Suite gates

Suite CI validates repository boundaries, naming, Python compilation, the pinned
Foundation contract, worker/connector manifests, static checks, and tests. Suite
must not construct a second Foundation authority or import Foundation internals.

### 7.3 Supply-chain baseline

- Rust and JavaScript dependency graphs use committed lock files where those
  ecosystems support them.
- Foundation Python development uses the committed `uv.lock`.
- Release workflows publish SBOM/provenance evidence where configured.
- Dependency and release controls described here are statements of current
  automation, not future certification claims.

---

## 8. Trusted Native Kill Switch Operations

Per Architecture Governance §36, the Kill Switch is a **critical security control**.

### 8.1 Kill Switch Triggers

| Trigger | Authority | Action |
|---------|-----------|--------|
| CVE in Trusted Native worker | Security Lead | Immediate revocation |
| Policy violation (e.g., scope creep) | ARB | Immediate revocation |
| Supply chain compromise (upstream) | Security Lead | Immediate revocation |
| Audit finding (external) | Security Lead + ARB | Scheduled revocation |

### 8.2 Kill Switch Procedure

```text
1. Security Lead authorizes → records in Security Ledger
2. Revoke Execution Grant for target capability_id (all epochs)
3. Disable Dispatch eligibility in Capability Registry
4. Emit Security Ledger Event: KILL_SWITCH_ACTIVATED
5. Notify affected users via in-app alert + email (if managed)
6. Publish advisory (if public impact)
7. Fallback: Capability marked Unavailable → graceful degradation
```

### 8.3 Kill Switch Requirements (from Arch Gov §36)

- Stop issuing new Grants **immediately** (no release wait).
- Terminate active execution **when required** (SideEffect uncertainty → `Uncertain` state).
- Record Ledger Security Event.
- Does **not** auto-destroy Vault credentials (OAuth tokens, API keys).
- Fallback to Sandbox / Capability Unavailable.

---

## 9. Secret & Credential Security

### 9.1 Current storage model

Device-local credentials are stored in Delta-owned private authority files with
user-restricted filesystem permissions. The current 0.1.0 implementation does
not claim application-level encryption at rest.

MCP configuration and connector/model credentials remain under Rust-owned
authority. Capability workers receive only secret keys granted to the current
job, and resolved secret values are delivered in memory rather than persisted in
worker manifests.

### 9.2 Handling rules

- Secrets must not be written to logs, telemetry, artifacts, manifests, or SBOMs.
- Workers receive only grant-scoped secret values.
- Product IPC must not expose stored secret values to the UI.
- Repository and CI configuration must not contain production credentials.
- Any future encrypted vault or cross-device secret sync requires a separate,
  implemented threat model and contract before this document may claim it.

---

## 10. Execution Boundary Security

### 10.1 Current enforcement

The Rust Capability Host constructs a typed job boundary from the registered
capability grants and the active workspace. It validates grant timing before
dispatch and supplies only granted secrets and workspace roots.

Worker implementations must enforce the grant dimensions they consume. In
particular, filesystem operations must remain within granted roots and network
operations must reject disallowed local/private targets where the capability
permits external URLs.

Process workers are supervised for timeout and cancellation. An explicit
`exec` grant is required by capabilities that intentionally launch commands.

### 10.2 Scope of the guarantee

Delta 0.1.0 does not claim that every worker runs inside a universal
OS-level/WASM sandbox. The security boundary is the combination of Rust grant
validation, supervised worker execution, and capability-specific enforcement.
A worker that bypasses its declared filesystem, network, secret, or process
boundary is a security defect.

---

## 11. Provenance & Supply Chain Integrity

### 11.1 Provenance Requirements (Arch Gov §44-47)

- All external content: `origin`, `source_refs`, `provenance_chain`, `external_influence`.
- Provenance propagates through Connector → Worker → Skill → Model → Runtime.
- Compaction allowed but **security meaning must survive** (source identity, trust class, external influence).

### 11.2 Artifact Integrity

- All Artifacts: content-addressed (SHA-256 min), signed by producer.
- Core validates Artifact identity, provenance, signature on ingestion.
- Tampered Artifact → `Uncertain` → reconciliation required.

---

## 12. Managed Service Security

### 12.1 Threat Model

Managed services (Connect, Sync, Relay) are **untrusted** from device perspective:
- Relay never sees plaintext (Arch Gov §41).
- Managed Sync never holds decryption keys.
- Device enrollment + Secret sync = separate authorizations.

### 12.2 Managed Service Compromise Response

1. Detect via anomaly monitoring / audit logs.
2. Revoke affected device certificates / relay tokens.
3. Force re-enrollment for affected users.
4. Audit Secret sync logs for exfiltration.
5. Publish advisory to enterprise customers.

---

## 13. Security Testing & Auditing

| Activity | Frequency | Scope |
|----------|-----------|-------|
| **SAST/DAST** | Every PR (CI) | Foundation + Suite |
| **Dependency Scan** | Every PR + Weekly scheduled | All repos |
| **Penetration Test** | Annual (external) | Full stack: App, Core, Managed |
| **Architecture Review** | Per ADR + Annual | Trust boundaries, authority model |
| **Fuzzing** | Continuous (OSS-Fuzz target) | Parsers, protocol handlers |
| **Red Team Exercise** | Biennial | End-to-end: device → managed → device |

---

## 14. Incident Response Plan

### 14.1 Phases

| Phase | Activities | Owner |
|-------|------------|-------|
| **Detect** | Monitoring alerts, user reports, audit logs | Security Team |
| **Triage** | Classify severity, assign lead, communicate internally | Security Lead |
| **Contain** | Kill switch, revoke credentials, isolate affected systems | Security Team + ARB |
| **Eradicate** | Develop fix, test, sign, stage | Security Team + Maintainers |
| **Recover** | Coordinated release, user notification, verification | Release Manager + Security |
| **Postmortem** | Root cause, timeline, action items, publish (blameless) | Security Lead |

### 14.2 Communication Plan

| Audience | Channel | Timing |
|----------|---------|--------|
| Internal (Core Team) | Secure channel (Signal/Slack) | Immediate |
| Enterprise Customers | Email + In-app + Portal | Within 4h (Critical) |
| Public (Foundation users) | GitHub Advisory + Blog + In-app | Fix release + 7 days |
| Researchers | Direct email | Throughout process |

---

## 15. Compliance & Certifications

| Standard | Status | Scope |
|----------|--------|-------|
| **SOC 2 Type II** | Target 2026 | Managed services |
| **ISO 27001** | Target 2027 | Full organization |
| **Common Criteria (EAL2+)** | Evaluation target | Core Trust/Runtime |
| **FIPS 140-3** | Evaluation target | Vault cryptography |

---

## 16. Version History

| Version | Date | Author | Change |
|---------|------|--------|--------|
| v1.0 | 2026-09-17 | Security Lead + ARB | Initial baseline |

---

**End of Document**