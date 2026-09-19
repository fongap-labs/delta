# Delta Deprecation & Migration Governance

**Version:** v1.0
**Status:** Governance Baseline — Final / Frozen
**Scope:** All public APIs, contracts, schemas, SDKs, capability interfaces, and user-facing Foundation features in `fongap/delta`.

---

## 1. Purpose

This document defines the policy for deprecating and removing public interfaces, migrating users, and evolving the Delta platform while honoring **Continuous Stability** (Architecture Governance §2.1) and **Predictable Upgrades** (§8 Rule 3).

---

## 2. Deprecation Principles

| Principle | Operational Meaning |
|-----------|---------------------|
| **No Surprise Removals** | Minimum 2 minor versions (≈16 weeks) notice before removal. |
| **Clear Migration Path** | Every deprecation has documented replacement + automated migration where feasible. |
| **Parallel Support** | Old and new coexist during deprecation window. |
| **Communication** | Multi-channel notice (release notes, docs, in-app, GitHub). |
| **Telemetry-Informed** | Usage data guides priority (but never blocks deprecation). |
| **Security Exceptions** | Critical vulnerabilities may shorten window (Security Governance §5). |

---

## 3. Deprecation Categories & Timelines

### 3.1 Public Contract / Schema Changes

| Change Type | Deprecation Window | Removal Earliest | Example |
|-------------|-------------------|------------------|---------|
| **Required field → Optional** | 2 minors | Next MAJOR | `timestamp` required → optional |
| **Field rename** | 2 minors | Next MAJOR | `user_id` → `actor_id` |
| **Enum value removal** | 2 minors | Next MAJOR | `status: "deprecated"` removed |
| **Interface removal** | 3 minors | Next MAJOR | `LegacyCapabilityRegistry` removed |
| **Behavior change (breaking)** | 2 minors | Next MAJOR | `sync()` now returns `Result` not `void` |

### 3.2 SDK API Changes (Rust/TS/Python)

| Change Type | Deprecation Window | Removal Earliest | Mechanism |
|-------------|-------------------|------------------|-----------|
| **Function/Method removal** | 2 minors | Next MAJOR | `#[deprecated]` / `@deprecated` / `warnings.warn` |
| **Parameter removal** | 2 minors | Next MAJOR | Optional with default → warn → remove |
| **Type signature change** | 2 minors | Next MAJOR | New function; old marked deprecated |
| **Module removal** | 3 minors | Next MAJOR | Re-export from new location → warn → remove |
| **Error type change** | 2 minors | Next MAJOR | New error enum; old convertible |

### 3.3 Capability Interface Changes

| Change Type | Deprecation Window | Removal Earliest | Notes |
|-------------|-------------------|------------------|-------|
| **Capability ID rename** | 2 minors | Next MAJOR | Alias old ID → warn → remove |
| **Permission scope change** | 2 minors | Next MAJOR | Grant validation warns on old scope |
| **Worker/Connector protocol version** | 3 minors | Next MAJOR | Versioned protocol; old supported |
| **Skill schema change** | 2 minors | Next MAJOR | Schema versioning; migration tool |

### 3.4 User-Facing Feature Changes

| Change Type | Deprecation Window | Removal Earliest | Communication |
|-------------|-------------------|------------------|---------------|
| **UI command/menu removal** | 2 minors | Next MAJOR | In-app notice + docs |
| **Setting/preference removal** | 2 minors | Next MAJOR | Auto-migrate value; warn |
| **Default behavior change** | 1 minor | Next MINOR | Opt-in first; then default |
| **File format change** | 3 minors | Next MAJOR | Auto-migrate on read; write new |

---

## 4. Deprecation Process

### 4.1 Initiation

1. **Proposal:** Maintainer opens GitHub Discussion with `deprecation-proposal` label.
2. **Required Content:**
   - What is being deprecated (exact symbol/feature).
   - Reason (architecture, security, usability, maintenance).
   - Replacement (new API, migration steps).
   - Impact assessment (telemetry usage, extension breakage estimate).
   - Timeline (target deprecation version, target removal version).
3. **Review:** ARB + affected Maintainers (7 days minimum).
4. **Decision:** ARB approves → Deprecation Issue created + tracking label.

### 4.2 Implementation Checklist

| Step | Owner | Verification |
|------|-------|--------------|
| Add deprecation annotations (`#[deprecated]`, `@deprecated`, etc.) | Author | CI warns on usage |
| Update documentation (mark deprecated; link replacement) | Author | Doc CI passes |
| Add migration guide entry | Author | `MIGRATION_GUIDE.md` updated |
| Implement replacement API (if not existing) | Author | Tests pass |
| Add automated migration (codemod, script, CLI) | Author | Migration test passes |
| Telemetry: track deprecated usage (opt-in) | Maintainer | Dashboard updated |
| Release notes: deprecation announcement | Release Manager | Published |

### 4.3 Communication Timeline

| Milestone | Channel | Timing |
|-----------|---------|--------|
| **Deprecation Announced** | GitHub Release Notes, Discussions, `DEPRECATIONS.md` | At deprecation release (e.g., 1.4.0) |
| **Reminder** | In-app banner (dev/build), Discord, Discussions | At 1 minor before removal (e.g., 1.5.0) |
| **Final Warning** | In-app banner (all), Release Notes, Migration Guide top | At removal release (e.g., 1.6.0) |
| **Removed** | Release Notes, Breaking Changes doc | Removal release |

---

## 5. Migration Support

### 5.1 Automated Migration Tooling

| Target | Tool | Trigger |
|--------|------|---------|
| **Rust SDK** | `cargo fix` / custom `delta-migrate` CLI | `cargo update` + `--migrate` flag |
| **TypeScript SDK** | `delta-migrate` (jscodeshift codemods) | `npx delta-migrate@latest` |
| **Python SDK** | `delta-migrate` (libcst/ast) | `pip install delta-migrate` |
| **Contracts/Schemas** | `delta-schema-migrate` (JSON/Protobuf transform) | CI integration |
| **User Config/State** | In-app migration on startup | Automatic + backup |

### 5.2 Migration Guarantees

- **Zero-data-loss:** Migration never deletes user data without explicit confirmation.
- **Rollback:** Previous version installable alongside new (side-by-side).
- **Verification:** Migration tool validates output (schema check, round-trip test).
- **Support:** Community/Maintainers assist with migration issues for 1 minor after removal.

### 5.3 Extension Author Support

- **Early Access:** Extension authors get pre-release SDK access (via GitHub prereleases).
- **Compatibility CI:** Suite CI runs extension compatibility tests (Dual-Repo Design §14).
- **Direct Channel:** `extensions@delta.example.com` for migration questions.

---

## 6. Exception Handling

### 6.1 Security-Driven Accelerated Deprecation

Per **Security Governance §5**, if a deprecated interface has **Critical/High vulnerability**:

| Normal Window | Accelerated Window | Authorization |
|---------------|-------------------|---------------|
| 2 minors | 1 patch release (or emergency) | Security Lead + ARB |

**Requirements:**
- Migration path **must** exist (even if manual).
- Communication **immediate** (security advisory channel).
- No removal without replacement unless **no safe alternative exists** (then: disable + guide to workaround).

### 6.2 Abandoned/Unmaintained Extensions

If a deprecated interface is used by **unmaintained third-party extensions**:

- Delta **does not** block deprecation.
- Delta **does** provide: migration guide, automated tooling, 2-minor window.
- Extension users: encouraged to fork/maintain or migrate.

---

## 7. Version-Specific Policies

### 7.1 Pre-1.0 (Current: 0.x)

- **No deprecation guarantees.** Breaking changes allowed in MINOR.
- **Best effort** migration tooling.
- **Clear communication** still required.

### 7.2 Post-1.0 (v1.0+)

- **Strict adherence** to windows in Section 3.
- **MAJOR version** = cumulative breaking changes from deprecations.
- **LTS branches** receive backported deprecation annotations (no removals).

### 7.3 MAJOR Release Process

1. **Deprecation Freeze:** 2 minors before MAJOR, no new deprecations.
2. **Removal Sprint:** All scheduled removals implemented.
3. **Migration Testing:** Full matrix (Core, SDK, Extensions, Suite).
4. **Release Candidate:** Extended RC period (4 weeks minimum).
5. **GA:** Migration guides finalized; support channels staffed.

---

## 8. Tracking & Metrics

### 8.1 Deprecation Registry

Maintained in `DEPRECATIONS.md` (machine-readable YAML front-matter):

```yaml
- id: DEP-0042
  target: "delta_sdk::capability::LegacyRegistry"
  type: "sdk_api"
  deprecation_version: "1.4.0"
  removal_version: "2.0.0"
  replacement: "delta_sdk::capability::Registry"
  migration_tool: "delta-migrate registry"
  status: "active"  # active | removed | deferred
  telemetry_usage: "low"  # high | medium | low | unknown
```

### 8.2 Success Metrics

| Metric | Target | Measurement |
|--------|--------|-------------|
| **Deprecated API usage at removal** | < 5% of peak | Telemetry (opt-in) + crate download stats |
| **Migration tool adoption** | > 80% of affected projects | Tool download / CI runs |
| **Extension breakage reports** | 0 critical | GitHub Issues labeled `migration-failure` |
| **User migration completion** | > 90% within 2 minors | In-app telemetry (opt-in) |

---

## 9. Relationship to Other Governance

| Document | Interaction |
|----------|-------------|
| **Architecture Governance** | Predictable upgrades (§2.1); Public contracts evolve slowly (§8 Rule 14) |
| **Release Governance** | Deprecation timeline drives MINOR/MAJOR cadence |
| **Contract Governance** | Contract changes follow same deprecation rules |
| **Security Governance** | Accelerated deprecation for vulns |
| **Contribution Governance** | Deprecation PRs follow standard review + ARB approval |

---

## 10. Version History

| Version | Date | Author | Change |
|---------|------|--------|--------|
| v1.0 | 2026-09-17 | ARB + Release Manager | Initial baseline |

---

**End of Document**