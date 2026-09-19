# Delta Compliance & Export Control Governance

**Version:** v1.0
**Status:** Governance Baseline — Final / Frozen
**Scope:** Legal compliance, export controls, sanctions, and regulatory obligations for public Delta Foundation distributions and project-operated services.

---

## 1. Purpose

This document defines Delta's compliance framework for export controls, sanctions, regulatory requirements, and legal obligations. It ensures Delta can operate globally while maintaining **Local-First** architecture (Architecture Governance §1) and **Flexible Outside** deployment model (§2.3).

---

## 2. Export Control Classification

### 2.1 Product Classification (EAR / US Regulations)

| Component | ECCN | Reason | License Exception |
|-----------|------|--------|-------------------|
| **delta-core** (runtime, trust, state) | EAR99 | General-purpose productivity software | N/A (EAR99 = no license required for most destinations) |
| **delta-app** (UI shells) | EAR99 | End-user application | N/A |
| **delta-sdk** (interfaces, contracts) | EAR99 | Publicly available APIs | N/A |
| **delta-stt** (speech recognition) | 5D002 / EAR99* | *If uses strong crypto → 5D002; else EAR99 | TSU / GOV (if 5D002) |
| **delta-sync** (E2EE sync) | 5D002 | Cryptographic functionality | TSU (publicly available) |
| **delta-connect** (OAuth) | EAR99 | Standard auth protocols | N/A |
| **Suite Capabilities** | EAR99 / 5D002 | Case-by-case | TSU where applicable |
| **Managed Services** | 5D002 / EAR99 | Hosted crypto / productivity | TSU / GOV |

> **Default Assumption:** EAR99 unless cryptographic functionality triggers 5D002.
> **Annual Review:** Classification reviewed by Legal + Security Lead.

### 2.2 Cryptography Controls

| Crypto Feature | Algorithm | Key Size | Classification |
|----------------|-----------|----------|----------------|
| **Vault Encryption** | AES-256-GCM | 256-bit | 5D002 (mass market → TSU) |
| **E2EE Sync** | X25519 + AES-256-GCM | 256-bit | 5D002 (mass market → TSU) |
| **TLS 1.3** | X25519 / P-256 + AES-GCM / ChaCha20-Poly1305 | 256-bit | 5D002 (standard → TSU) |
| **Code Signing** | Ed25519 / ECDSA P-256 | 256-bit | 5D002 (auth only → TSU) |
| **Provenance Hashing** | SHA-256 / BLAKE3 | 256-bit | Not controlled (hash only) |

**TSU (Technology Software Unrestricted) Eligibility:**
- Publicly available (Foundation source + binaries).
- Not designed for military/intelligence end-use.
- Meets §740.13(b) criteria → **no license required** for most countries.

### 2.3 De Minimis & Direct Product Rules

- **De Minimis:** Delta Foundation < 10% US-origin controlled content (verified annually).
- **Direct Product Rule:** Not applicable (EAR99/TSU items not subject to FDP rule for most destinations).
- **Re-export:** Recipients must comply with their local laws.

---

## 3. Sanctions & Restricted Party Screening

### 3.1 Screening Policy

| Screened Entity | Screening Tool | Frequency |
|-----------------|----------------|-----------|
| **Contributors (PR authors)** | GitHub metadata + manual review (high-risk) | Per PR (automated) |
| **Dependency Authors** | `cargo-deny` + `denied-parties` crate | Weekly scheduled |
| **Registry Publishers** | crates.io / npm / PyPI metadata | Per publish |
| **Enterprise Customers** | Compliance DB (Dow Jones / Refinitiv) | Onboarding + quarterly |
| **Managed Service Users** | Real-time API (compliance vendor) | Per transaction |

### 3.2 Restricted Lists Monitored

- **US:** SDN, DPL, Entity List, UVL, NS-PLC, MEU, FSE.
- **EU:** Consolidated Financial Sanctions List.
- **UK:** Consolidated List.
- **UN:** Security Council Consolidated List.
- **Other:** Canada, Australia, Japan, Switzerland lists.

### 3.3 Match Handling

| Match Type | Action |
|------------|--------|
| **Exact Match (Contributor)** | Block merge; notify Legal; contributor can appeal with license |
| **Exact Match (Dependency)** | Block CI; find alternative; document exception if critical |
| **Exact Match (Customer)** | Block onboarding; require OFAC/EU license |
| **Fuzzy Match** | Manual review by Compliance Officer (24h SLA) |

---

## 4. Regional Compliance

### 4.1 Data Residency & Localization

| Region | Requirement | Delta Implementation |
|--------|-------------|---------------------|
| **EU (GDPR)** | Data residency optional | Managed Services: EU region option; Foundation: local-only by default |
| **China (PIPL/CSL)** | Data localization for "important data" | Suite: China region (partner-operated); Foundation: not offered in CN App Store |
| **Russia (152-FZ)** | Personal data localization | Not offered; no Managed Services |
| **India (DPDP)** | Cross-border transfer restrictions | Standard Contractual Clauses; local region if demand |
| **Brazil (LGPD)** | Data localization for sensitive data | SCCs; no local region planned |

### 4.2 App Store & Distribution Compliance

| Platform | Requirements | Delta Compliance |
|----------|--------------|------------------|
| **Apple App Store** | Privacy Manifest, ATT, Export Compliance | Annual export compliance declaration; Privacy Manifest in `Info.plist` |
| **Google Play** | Data Safety Form, Target API | Data Safety Form updated per release; Target API current |
| **Microsoft Store** | Age Rating, Privacy Policy | Age Rating: 12+; Privacy Policy linked |
| **GitHub Releases** | No special requirements | Standard |
| **Package Registries** | License metadata | SPDX in `Cargo.toml` / `package.json` / `pyproject.toml` |

### 4.3 Government / Enterprise Certifications

| Certification | Target | Scope | Timeline |
|---------------|--------|-------|----------|
| **FedRAMP Low** | Managed Services (US Gov) | Sync/Connect/Relay | 2027 |
| **StateRAMP** | Managed Services (US State) | Sync/Connect | 2027 |
| **IRAP** (China) | Managed Services (China) | Via partner | TBD |
| **C5** (Germany) | Managed Services (EU) | Sync/Connect | 2026 |
| **ISO 27001** | Organization | Full | 2027 |
| **SOC 2 Type II** | Managed Services | Sync/Connect/Relay | 2026 |

---

## 5. Open Source Compliance

### 5.1 License Compliance (See Source Licensing Governance)

- **Automated:** `cargo-deny`, `license-checker`, `pip-licenses` in CI.
- **SBOM:** CycloneDX per release (Foundation + Suite).
- **Attribution:** `NOTICE` file in all binaries; in-app "Licenses" screen.

### 5.2 Contribution Compliance

- **CLA Required:** All code contributions (Contribution Governance §3).
- **Corporate CLA:** Required for orgs with 5+ contributors.
- **Government Employees:** Special acknowledgment (US: `GOV` license exception).

### 5.3 Dependency Risk Management

| Risk | Mitigation |
|------|------------|
| **Malicious Dependency** | `cargo-deny` advisories; `gitleaks`; signed packages; reproducible builds |
| **Abandoned Dependency** | `cargo-outdated` + `deps.rs` monitoring; vendor critical deps |
| **License Change** | CI fails on new license; manual review required |
| **Supply Chain Attack** | SLSA Level 2 target; sigstore signing; provenance attestation |

---

## 6. AI/ML Model Compliance

### 6.1 Model Protocol Compatibility

Delta supports **OpenAI-compatible** and **Anthropic-compatible** protocols (Arch Gov §1).

| Model Deployment | Compliance Consideration |
|------------------|--------------------------|
| **Local (llama.cpp, Ollama)** | No export control (user-provided) |
| **LAN / Self-Hosted** | User responsible for model licensing |
| **User-Configured Remote** | User responsible for provider ToS |
| **Managed Remote (Delta)** | Delta ensures provider compliance (if offered) |

### 6.2 Model Governance

- **No Bundled Models:** Delta ships **zero** model weights.
- **Model Registry:** Optional; only references (name, protocol, capabilities).
- **Provenance Tracking:** Model identity in `provenance_chain` (Arch Gov §44-47).

---

## 7. Accessibility Compliance

| Standard | Target | Status |
|----------|--------|--------|
| **WCAG 2.1 AA** | Delta App (all platforms) | In progress (v1.2 target) |
| **Section 508** (US Federal) | Delta App | Aligned with WCAG 2.1 AA |
| **EN 301 549** (EU) | Delta App | Aligned with WCAG 2.1 AA |

**Implementation:** Platform-native accessibility APIs; semantic UI; screen reader testing in CI.

---

## 8. Audit & Documentation

### 8.1 Compliance Artifacts

| Artifact | Owner | Update Frequency |
|----------|-------|------------------|
| **Export Classification Matrix** | Legal + Security Lead | Annual |
| **Sanctions Screening Log** | Compliance Officer | Continuous (automated) |
| **SBOM Archive** | Release Manager | Per release |
| **DPA / SCC Repository** | Legal | Per customer / annual |
| **Certification Evidence** | Security Lead | Per audit cycle |
| **Accessibility Audit Report** | Engineering Lead | Per major release |

### 8.2 Record Retention

| Record Type | Retention |
|-------------|-----------|
| Export classifications | 5 years post-shipment |
| Screening logs | 5 years |
| SBOMs | 7 years (aligned with support window) |
| Customer DPAs | 7 years post-contract |
| Certification evidence | 3 years post-expiry |

---

## 9. Incident Response (Compliance)

### 9.1 Compliance Incident Types

| Type | Example | Response |
|------|---------|----------|
| **Export Violation** | Shipment to embargoed country | Legal notification (voluntary self-disclosure); 5-day internal investigation |
| **Sanctions Match** | Contributor on SDN list | Immediate block; Legal review; OFAC license if applicable |
| **License Violation** | GPL dependency in Foundation | Immediate yank/replace; Legal assessment |
| **Data Residency Breach** | EU data processed in US without SCC | Immediate containment; DPA assessment; Supervisory Authority notification (72h GDPR) |

### 9.2 Voluntary Self-Disclosure (VSD)

- **Policy:** Proactive disclosure for export/sanctions errors.
- **Process:** Legal prepares VSD → submits to BIS/OFAC → implements corrective actions.
- **Documentation:** All VSDs tracked in Compliance Register.

---

## 10. Training & Awareness

| Audience | Training | Frequency |
|----------|----------|-----------|
| **All Engineers** | Export Controls Basics; Open Source Compliance | Annual |
| **Maintainers/Release Managers** | Release Compliance Checklist; SBOM Generation | Per release cycle |
| **Sales/Support** | Sanctions Screening; Data Residency | Annual |
| **Legal/Compliance** | Regulatory Updates; Case Law | Quarterly |

---

## 11. Version History

| Version | Date | Author | Change |
|---------|------|--------|--------|
| v1.0 | 2026-09-17 | Legal + Security Lead + ARB | Initial baseline |

---

**End of Document**