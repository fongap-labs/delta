# Delta Telemetry & Privacy Governance

**Version:** v1.0
**Status:** Governance Baseline — Final / Frozen
**Scope:** Telemetry, diagnostics, crash reporting, and user-data processing implemented by public Delta Foundation.

---

## 1. Purpose

This document defines the privacy principles, data categories, collection policies, user controls, and compliance requirements for Delta telemetry. It operationalizes **Metadata-First Telemetry** (Architecture Governance §53), **Content Diagnostics** (§54), and **Local-First** product positioning (§1).

---

## 2. Privacy Principles

| Principle | Operational Meaning |
|-----------|---------------------|
| **Local-First** | Telemetry processed locally by default; upload requires explicit consent. |
| **Metadata-First** | Default telemetry = metadata only (counts, durations, status); no content. |
| **Purpose Limitation** | Data collected only for: product improvement, security, reliability, performance. |
| **Data Minimization** | Minimum fields necessary; no unique identifiers unless user-consented. |
| **User Control** | Granular opt-in/opt-out; real-time toggle; data deletion on request. |
| **Transparency** | Public schema; in-app viewer; annual transparency report. |
| **No Surveillance** | Telemetry never used for: advertising, profiling, behavioral tracking, employee monitoring. |

---

## 3. Data Categories

### 3.1 Category Definitions

| Category | Description | Examples | Default Collection |
|----------|-------------|----------|-------------------|
| **Required** | Essential for operation; cannot be disabled | App start (version check), License validation (Suite), Security revocation check | Always (no PII) |
| **Standard Telemetry** | Metadata about usage, performance, reliability | Duration, token count, byte count, latency, status, error codes, queue depth, resource usage | **Opt-out** (enabled by default) |
| **Enhanced Telemetry** | Detailed metadata for product improvement | Feature usage funnels, capability adoption, UI interaction paths | **Opt-in** |
| **Content Diagnostics** | Content-bearing data for debugging | Full prompt, model tokens, STT transcript, email body, artifact content | **Explicit Opt-in + Scope + Lifetime** (§54 Arch Gov) |
| **Crash Reports** | Automated crash dumps | Stack traces, register state, memory map (no heap content) | **Opt-in** |

### 3.2 Explicitly Excluded (Never Collected)

| Data Type | Reason |
|-----------|--------|
| **Secrets / Credentials** | Vault authority local (Arch Gov §40) |
| **Encryption Keys** | E2EE keys never leave device (§41) |
| **User Content (default)** | Provenance: "Data by default, not Instruction" (§44) |
| **PII (names, emails, IPs)** | Unless in Content Diagnostics with explicit consent |
| **Model Output Content** | Unless Content Diagnostics opted in |
| **STT Transcript Content** | Unless Content Diagnostics opted in |

---

## 4. Collection Policies

### 4.1 Standard Telemetry (Opt-Out)

**Collected By Default (user can disable in Settings → Privacy):**

| Event | Fields | Retention |
|-------|--------|-----------|
| `app.start` | `version`, `platform`, `arch`, `locale`, `startup_duration_ms` | 13 months |
| `run.completed` | `run_type`, `capability_ids[]`, `duration_ms`, `token_count`, `status`, `error_code?` | 13 months |
| `capability.invoked` | `capability_id`, `duration_ms`, `status`, `error_code?` | 13 months |
| `sync.completed` | `duration_ms`, `bytes_synced`, `status`, `error_code?` | 13 months |
| `extension.installed` | `extension_id`, `version`, `source` | 13 months |
| `error.occurred` | `component`, `error_code`, `severity`, `recoverable` | 13 months |

**Anonymization:**
- No device ID, user ID, IP address.
- `session_id` = random UUID per app session (rotated).
- `installation_id` = random UUID per device (persistent, resettable).

### 4.2 Enhanced Telemetry (Opt-In)

**Requires explicit toggle in Settings → Privacy → "Share detailed usage data":**

| Event | Additional Fields |
|-------|-------------------|
| `ui.interaction` | `screen`, `action`, `element_id`, `duration_ms` |
| `capability.detail` | `parameters_schema_hash`, `result_size_bytes`, `retry_count` |
| `model.request` | `model_family`, `request_tokens`, `response_tokens`, `latency_ms` |
| `search.query` | `query_length`, `result_count`, `source_type` |

### 4.3 Content Diagnostics (Explicit Opt-In + Scope + Lifetime)

**Per Architecture Governance §54:**

| Requirement | Implementation |
|-------------|----------------|
| **Explicit User Authorization** | Modal dialog: "Share content for debugging?" with scope selector |
| **Explicit Scope** | Checkboxes: [ ] Prompts [ ] Model Output [ ] STT [ ] Email [ ] Artifacts |
| **Explicit Lifetime** | Dropdown: [ 24h ] [ 7 days ] [ 30 days ] [ Until I disable ] |
| **Necessary Redaction** | Auto-redact: secrets, keys, PII patterns (configurable) |
| **Immediate Disable** | "Stop Sharing" button in banner + Settings |
| **Ledger Record** | `ConsentEvent` in Authority Ledger (Arch Gov §54) |

**Data Handling:**
- Encrypted at rest (device) + in transit (TLS 1.3).
- Access: Delta Security Team only (approved ticket).
- Auto-delete at lifetime expiry.
- Never used for training (unless separate Research Consent).

### 4.4 Crash Reports (Opt-In)

| Field | Included |
|-------|----------|
| Stack trace (all threads) | ✅ |
| Register state | ✅ |
| Memory map (no heap) | ✅ |
| App version, OS, architecture | ✅ |
| **Heap / local variables** | ❌ |
| **User content in memory** | ❌ |

**Upload:** User prompted per crash ("Send crash report?"); can set "Always send" / "Never send".

---

## 5. User Controls

### 5.1 Settings UI (Privacy Panel)

```
┌─ Privacy & Telemetry ────────────────────────────────────┐
│ ☐ Standard Telemetry (metadata only)          [ON] ▼    │
│   └─ Learn what's collected →                                │
│                                                            │
│ ☐ Enhanced Telemetry (detailed usage)           [OFF] ▼   │
│   └─ Learn what's collected →                                │
│                                                            │
│ Content Diagnostics Debug Mode                     [OFF] ▼ │
│   Scope:  ☐ Prompts  ☐ Model Output  ☐ STT  ☐ Email       │
│   Lifetime: [ 7 days ▼ ]                                   │
│   Redaction: [ Auto (secrets, PII) ]  [ Custom Rules ]     │
│   [ Stop Sharing Now ]                                     │
│                                                            │
│ ☐ Crash Reports                                           [ON] ▼ │
│                                                            │
│ [ View Collected Data ]  [ Delete All Data ]  [ Export ]   │
│                                                            │
│ Transparency Report (annual) →                            │
└────────────────────────────────────────────────────────────┘
```

### 5.2 Programmatic Controls

| API | Description |
|-----|-------------|
| `Telemetry.setStandardEnabled(bool)` | App/Extension control (user override wins) |
| `Telemetry.setEnhancedEnabled(bool)` | Requires user opt-in |
| `Telemetry.startContentDiagnostics(scope, lifetime)` | Returns `DiagnosticsSession` token |
| `Telemetry.stopContentDiagnostics(token)` | Immediate stop + queue flush |
| `Telemetry.exportUserData()` | GDPR/CCPA export (JSON) |
| `Telemetry.deleteUserData()` | Full purge (except Required) |

### 5.3 Enterprise Controls (Suite/Managed)

- **Organization Policy:** Admin can enforce telemetry settings via Managed Config.
- **Data Residency:** Enterprise chooses telemetry endpoint region (EU, US, APAC).
- **Audit Log:** All telemetry config changes logged to Authority Ledger.

---

## 6. Data Processing & Storage

### 6.1 Pipeline Architecture

```text
Device (Local Processing)
      ↓
Batch + Encrypt (AES-256-GCM, per-session key)
      ↓
HTTPS → Telemetry Ingestion (Delta-operated)
      ↓
Decrypt → Validate Schema → Anonymize (strip residual PII)
      ↓
Aggregate (no raw events stored > 30 days)
      ↓
Analytics Warehouse (access-controlled)
```

### 6.2 Retention Schedule

| Data Type | Raw Retention | Aggregated Retention |
|-----------|---------------|---------------------|
| Standard Telemetry | 30 days | 13 months |
| Enhanced Telemetry | 30 days | 13 months |
| Content Diagnostics | Per user lifetime (max 30 days) | Never aggregated |
| Crash Reports | 90 days | 13 months (aggregated) |
| Consent Ledger | Indefinite (Authority Ledger) | — |

### 6.3 Access Control

| Role | Access |
|------|--------|
| **Telemetry Engineering** | Aggregated dashboards only |
| **Security Team** | Crash reports + Content Diagnostics (ticketed) |
| **Product Management** | Aggregated dashboards only |
| **Support** | User's own data (with user consent) |
| **Legal/Compliance** | Audit logs only |

---

## 7. Compliance

### 7.1 Regulations Addressed

| Regulation | Applicability | Delta Approach |
|------------|---------------|----------------|
| **GDPR** (EU) | Global (users in EU) | Lawful basis: Legitimate Interest (Standard) / Consent (Enhanced/Content); DPO appointed; Art. 28 processors |
| **CCPA/CPRA** (California) | Users in CA | Opt-out = "Do Not Sell" honored; deletion API; no sale |
| **LGPD** (Brazil) | Users in BR | Similar to GDPR |
| **PIPL** (China) | Users in CN | Local data residency option (Suite); consent management |
| **ePrivacy** (EU) | Cookies/Trackers | No cookies; local storage only for telemetry config |

### 7.2 Data Processing Agreement (DPA)

- **Standard Telemetry:** Legitimate Interest assessment documented; no DPA required for users.
- **Enterprise Customers:** DPA included in Commercial Agreement (Suite/Managed).
- **Subprocessors:** Published list (ingestion hosting, analytics warehouse); 30-day notice for changes.

### 7.3 Data Subject Rights

| Right | Implementation |
|-------|----------------|
| **Access** | `Telemetry.exportUserData()` → JSON download |
| **Rectification** | N/A (telemetry immutable); can supplement via Support |
| **Erasure** | `Telemetry.deleteUserData()` + backend purge (30 days) |
| **Restriction** | Toggle off Standard/Enhanced; Content Diagnostics auto-expires |
| **Portability** | Export includes schema; machine-readable |
| **Objection** | Opt-out toggles = objection honored immediately |

---

## 8. Transparency & Accountability

### 8.1 Public Artifacts

| Artifact | Location | Update Frequency |
|----------|----------|------------------|
| **Telemetry Schema** | `docs/telemetry/schema.json` | Per release |
| **Data Dictionary** | `docs/telemetry/data-dictionary.md` | Per release |
| **Transparency Report** | `https://delta.example.com/transparency` | Annual |
| **DPIA Summary** | `docs/privacy/dpia-summary.md` | Per major change |

### 8.2 Transparency Report Contents

- Total events ingested (by category).
- Government requests (count, type, compliance rate).
- Security incidents involving telemetry data.
- Policy changes.
- Retention compliance audit results.

---

## 9. Managed Services Telemetry

### 9.1 Managed Connect / Sync / Relay

| Component | Telemetry | User Visibility |
|-----------|-----------|-----------------|
| **Relay** | Connection count, bandwidth, latency, error rates | Admin dashboard (Enterprise) |
| **Managed Sync** | Sync duration, bytes, conflict count, device count | Admin dashboard |
| **Managed Connect** | OAuth flow success/failure, token refresh rate | Admin dashboard |

**Principles:**
- **No content** ever logged by Managed Services.
- **E2EE preserved:** Relay never sees plaintext (Arch Gov §41).
- **Admin-only access:** End users' telemetry not visible to org admins.

---

## 10. Third-Party Integrations

| Integration | Data Shared | User Control |
|-------------|-------------|--------------|
| **Error Tracking (Sentry)** | Crash reports (opt-in) | Disable in Privacy settings |
| **Analytics (Custom)** | Standard/Enhanced aggregates | Opt-out = no send |
| **Update Checks** | Version, platform | Required (security) |

**No third-party analytics by default.** All integrations opt-in or enterprise-configured.

---

## 11. Audit & Enforcement

### 11.1 Internal Audits

| Audit | Frequency | Owner |
|-------|-----------|-------|
| **Schema Compliance** | Per release (CI) | Telemetry Engineering |
| **PII Scan** | Weekly (automated) | Security Team |
| **Retention Compliance** | Quarterly | Privacy Officer |
| **Consent Validity** | Monthly (sample) | Privacy Officer |

### 11.2 Violation Response

| Violation | Response |
|-----------|----------|
| Accidental PII in Standard | Immediate purge; incident report; user notification if identifiable |
| Content Diagnostics beyond scope | Immediate stop; purge; user notification |
| Retention exceeded | Purge; process fix; transparency report note |
| Unauthorized access | Security incident response (Security Governance) |

---

## 12. Version History

| Version | Date | Author | Change |
|---------|------|--------|--------|
| v1.0 | 2026-09-17 | Privacy Officer + ARB | Initial baseline |

---

**End of Document**