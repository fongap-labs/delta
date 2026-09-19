# Delta Architecture Governance Specification

**Version:** v1.0  
**Status:** Architecture Governance Baseline - Revised  
**Scope:** Delta product architecture, core runtime, system capabilities, extension ecosystem, managed services, cross-platform implementations, and public contracts.  

> **Revision (2026-09-17):** Aligns with `target-architecture.md`. Key changes: (1) slogan updated from "One App, One Core, One Runtime" to "One Authority Plane, One Runtime"; (2) Mobile/iOS/Android reclassified from current target to future roadmap; (3) Resolver removed as a required architecture layer; (4) Security domains restructured - Secrets and Ledger folded back into Trust per target-architecture.md §3.

---

## 1. Product Positioning

Delta is a:

> **Local-first, cross-platform, extensible personal work agent that keeps users in control.**

Primary product scenarios:

- Everyday productivity
- Research and analysis
- Content creation
- Meeting collaboration
- Knowledge management

Delta is **not** positioned primarily as a coding agent.

Delta supports two model protocol families:

- OpenAI-compatible
- Anthropic-compatible

Protocol compatibility is independent from model deployment location. A model endpoint may be:

- Local
- LAN / self-hosted
- User-configured remote
- Managed remote

Foundation operation must not depend on an official Delta-hosted model service.

---

## 2. Highest-Level Governance Principles

Delta is governed by four long-term principles:

> **Continuous Stability. Secure Efficiency. Flexible Outside, Strict Inside.**

### 2.1 Continuous Stability

Delta should optimize for:

- Stable top-level architecture
- Slowly evolving public contracts
- Predictable upgrades and migrations
- Recoverable durable state
- Fault isolation at external boundaries
- Minimal architectural churn

### 2.2 Secure Efficiency

Security and efficiency are not opposites.

Delta should:

- Keep critical boundaries strict
- Keep normal execution paths lightweight
- Apply strong controls to high-risk operations
- Avoid unnecessary blocking on low-risk operations
- Prefer bounded policies over repeated prompts

### 2.3 Flexible Outside

Delta should remain flexible across:

- Operating systems
- Model providers
- Local, self-hosted, and remote deployment
- Extension types
- Data-plane implementation
- Foundation / Advanced / Managed delivery models
- Internal implementation technologies

The architecture must not be coupled to one cloud provider, one database, one IPC mechanism, or one model vendor.

### 2.4 Strict Inside

The following boundaries are non-negotiable:

- Authority
- Trust
- State
- Secrets
- Execution ownership
- Side effects
- Public contracts

No official, commercial, or advanced capability may bypass them.

---

## 3. Top-Level Architecture

```text
Delta
-├── Product
-  ├── delta-app
-  └── delta-core
-├── System Capabilities
-  ├── delta-connect
-  ├── delta-sync
-  └── delta-stt
-├── Extension Ecosystem
-  ├── delta-connector-*
-  ├── delta-worker-*
-  └── delta-skill-*
-└── Development Contract
    └── delta-sdk
```

The top-level architecture is frozen at v1.0.

New capabilities should first be implemented through:

- Core internal modules
- Capabilities
- Skills
- Workers
- Connectors
- Public contracts

A new top-level `delta-*` component requires formal architecture review.

---

## 4. Capability Tiers

Delta capabilities are grouped into three product tiers:

```text
Foundation
Advanced
Managed
```

All three tiers share:

- One `delta-app`
- One Authority Plane
- One Runtime
- One `delta-sdk`
- One Trust model
- One State model
- One Capability model
- One Ledger model

> Capability tiers may differ. The architecture must not fork.

---

## 5. Foundation

Foundation is the complete base capability set of Delta.

The governing test is:

> If Advanced and Managed capabilities are removed, Delta must still remain a complete, locally runnable personal work agent under user control.

Foundation includes, at minimum:

- `delta-app`
- `delta-core`
- `delta-sdk`
- `delta-stt`
- Basic connectors
- Basic workers
- Basic skills
- Connect client / contract
- Sync client / contract
- Manual OAuth / API-key connection
- Local Vault
- Local Runtime
- Local Automation
- Local Artifact handling

Foundation should prefer:

- Openness
- Local availability
- Replaceability
- User-managed operation

Source availability and licensing are governed separately by the **Delta Source and Licensing Governance Specification**.

---

## 6. Advanced

Advanced capabilities provide higher-value or domain-specific productivity.

Examples:

- Meeting Intelligence
- Professional skills
- Advanced workers
- Advanced connectors
- Domain-specific capabilities

Advanced differs from Foundation by capability, not by architectural privilege.

Advanced capabilities must use:

- The same Core
- The same SDK
- The same Trust rules
- The same State model
- The same Capability model
- The same Ledger
- The same Sandbox rules

The following are **not** valid reasons for architectural privilege:

- Official maintenance
- Paid status
- Premium positioning
- Domain specialization
- Distribution status

---

## 7. Managed

Managed capabilities are services operated by Delta on behalf of users.

Examples:

- Managed Connect
- Managed Sync
- Enterprise Services

Managed capabilities may provide:

- OAuth broker
- Hosted callback
- Relay
- Presence
- Device discovery
- Hosted sync
- Managed catalog
- Enterprise governance

Managed services must not become:

- Core Runtime
- Run Authority
- Trust Authority
- Secret Authority
- The sole source of truth

> Managed service failure must not make local Foundation unusable.

---

## 8. Global Architecture Rules

The following rules are frozen:

1. One `delta-app`.
2. One Authority Plane.
3. One Runtime.
4. One logical Authority per business fact.
5. Core is the only Control / Authority Plane.
6. Core must not become a universal data bus.
7. Platform differences are expressed through Adapters and Capabilities.
8. Skills describe methods; they do not own execution authority.
9. Workers execute; they do not orchestrate.
10. Connectors adapt external protocols; they do not own credentials.
11. Sync synchronizes state; it does not become a second Runtime.
12. Advanced capabilities have no private architectural backdoor.
13. Managed services are optional.
14. Public contracts evolve slowly; internal implementations may evolve continuously.
15. Critical boundaries fail closed.
16. Non-critical failures degrade gracefully.
17. Uncertainty must not be hidden by speculative execution.

---

## 9. delta-app

`delta-app` is the single user-facing product shell.

> **Current target:** Desktop only (Windows, macOS, Linux). Mobile (iOS, Android) is a **future roadmap** item, not a current formal target. Any decision to pursue Mobile must first update this specification and `target-architecture.md`.

```text
delta-app
├── Desktop (current target)
-  ├── Windows
-  ├── macOS
-  └── Linux
-└── Mobile (future roadmap -not current)
    ├── iOS
    └── Android
```

Desktop and Mobile are different hosts for the same product, not different products.

The following are prohibited:

- `desktop-core`
- `mobile-core`
- `core-lite`
- `desktop-runtime`
- `mobile-runtime`
- `desktop-workflow`
- `mobile-workflow`

Platform-specific differences belong in adapters such as:

- Filesystem
- Keychain / Keystore
- Notifications
- Audio
- Background execution
- Network
- Share services
- Process management

---

## 10. App-Core Communication

The App must not directly mutate business facts.

The canonical direction is:

```text
App
 -Command / Intent
 -delta-core
 -State Transition
 -View / Event / Stream
 -App
```

The App may cache:

- UI views
- Render state
- Temporary presentation state

But it must not become a business Authority.

Streaming data may include:

- STT transcript
- Run progress
- Model stream
- Worker progress
- Event feed

These are exposed through public View / Event / Stream contracts.

The architecture does not mandate a specific IPC technology.

---

## 11. delta-core

`delta-core` is the sole Control Plane and Authority Plane.

```text
delta-core
-├── Runtime
-  ├── Session
-  ├── Task
-  ├── Run
-  ├── Step
-  ├── Orchestrator
-  ├── Lifecycle
-  └── Recovery
-├── Trust
-  ├── Policy
-  ├── Permission
-  ├── Approval
-  ├── Approval Policy
-  ├── Capability Grant
-  └── SideEffect
-├── State
-  ├── Durable State
-  ├── Version
-  ├── Change Metadata
-  ├── Snapshot
-  └── Migration
-├── Secrets
-  └── Local Vault
-├── Work
-  ├── Source
-  ├── Provenance
-  ├── Artifact
-  ├── Validation
-  └── Artifact Lifecycle
-├── Capability
-  ├── Host / Registry
-  ├── Device Manifest
-  ├── Execution Grant
-  └── Dispatch Boundary
-├── Automation
├── Learning
├── Model
├── Ledger
└── Storage
```

These are internal Core responsibilities and should not be mechanically turned into top-level products such as:

- `delta-runtime`
- `delta-trust`
- `delta-state`
- `delta-work`
- `delta-capability`
- `delta-model`

---

## 12. Core Internal Security Domains

Core is one product boundary but is internally governed by security domains.

> **v1.0:** Secrets and Ledger are consolidated into the Trust domain per `target-architecture.md` §3, which defines Trust as: Policy, Permission/Risk, Approval, Ledger/Audit, Idempotency, Secrets, Sandbox.

```text
Runtime Domain
├── Runtime
├── State
├── Automation
└── Recovery

Trust Domain
├── Trust
├── Permission
├── Approval
├── Approval Policy
├── SideEffect
├── Secrets
├── Sandbox
├── Ledger / Audit
└── Idempotency

Work / Artifact Domain
├── Work
├── Source
├── Provenance
└── Artifact Lifecycle

Capability / Dispatch Domain
├── Capability
├── Execution Grant
└── Model Dispatch

Default Domain
└── Learning

Storage
-controlled persistence for all domains
```

Rules:

- Cross-domain access must use explicit interfaces.
- Cross-domain state mutation is prohibited unless explicitly governed.
- Storage has no business Authority.
- Security boundaries must not be bypassed for performance reasons.

---

## 13. Authority

The following Authorities belong only to Core:

- Task
- Run
- Orchestration
- Trust
- Approval
- Scheduler
- Recovery
- State
- Secret
- Artifact
- Capability
- Memory
- Ledger

Peripheral components explicitly do **not** own these Authorities:

```text
delta-app          -no business Authority
delta-connect      -no Credential Authority
delta-sync         -no Runtime Authority
delta-stt          -no Task Authority
delta-connector-*  -no Secret Authority
delta-worker-*     -no Run Authority
delta-skill-*      -no Scheduler Authority
Advanced           -no privileged Authority
Managed            -no local Core Authority
```

---

## 14. Core Is Not a Central Data Bus

Core decides:

- Who may act
- What may be done
- When it may happen
- Which capability may execute
- Whether a result is accepted

Core does not carry every byte of data.

```text
Core
 -Execution Grant + Artifact Reference
 -Worker

Worker
 -direct data processing
 -Result / Artifact Reference
 -Core
```

Large payloads must not repeatedly cross Core through serialization.

---

## 15. Fail Closed and Graceful Degradation

Critical-boundary uncertainty defaults to **Fail Closed**.

Examples:

- Unknown Authority
- Unknown Trust state
- Unknown Permission state
- Secret integrity failure
- Approval state uncertainty
- Side-effect uncertainty
- Execution ownership uncertainty
- Critical State integrity failure
- Security Ledger write failure

The following should prefer **Graceful Degradation**:

- UI cache
- Learning
- Search index
- Non-critical telemetry
- A single connector
- A single worker
- A single Advanced capability
- Managed service
- Non-critical auxiliary service

> Security-critical uncertainty should stop execution. Experience-critical failures should isolate and degrade where possible.

---

## 16. Runtime and Orchestrator

Execution structure:

```text
Session
 -Task
 -Run
 -Step
```

Runtime is the sole dynamic Orchestrator.

It is responsible for:

- Execution context
- Execution graph
- Step dispatch
- Model-output handling
- Yield handling
- Capability requests
- Worker / Connector coordination
- Suspend / Resume
- Recovery

---

## 17. Fast Path

Normal low-risk execution should use a Fast Path.

Typical preconditions:

- Local or trusted execution environment
- Existing valid Grant
- Valid `execution_epoch`
- Resources available
- No ownership conflict
- No new human approval required
- No unresolved security condition

```text
Validate
 -Dispatch
 -Result
```

Fast Path reduces orchestration overhead only. It does **not** reduce applicable security checks.

`Validate` must enforce, when applicable:

- Trust
- Execution Grant
- `execution_epoch`
- Approval Policy
- Policy quota
- Provenance constraints
- SideEffect requirements
- Authority Ledger requirements

Therefore:

```text
Fast -Bypass Trust
Fast -Bypass Approval
Fast -Bypass Ledger
```

---

## 18. Fast Path Escalation

If execution encounters a new risk, new capability need, or approval boundary:

```text
Fast Path
   -Risk / Capability / Approval Boundary
   -Escalate
   -Persist Required Context
   -Suspend
   -Trust / Approval / Grant
   -Resume
```

Escalation must preserve:

- `run_id`
- `execution_epoch`
- Provenance
- Artifact references
- Approval consumption
- `operation_id`
- Committed Step state

Fast Path and Heavy Path are not separate runtimes.

> They are two execution intensities of the same Runtime.

---

## 19. Commit Boundary

Not every in-memory change must be durable.

State is divided into:

```text
Ephemeral Execution State
-may be lost or recomputed

Committed Business State
-must be durable
```

Only facts that cross the Commit Boundary must be:

- Atomic
- Durable
- Recoverable

During Fast -Heavy escalation:

1. Persist required context.
2. Then enter `Suspended`.

A committed Step must not be re-executed merely because of escalation, suspend/resume, or recovery.

---

## 20. Lifecycle

Basic lifecycle:

```text
Ready
 -Executing
 -Completed / Failed
```

When suspension is needed:

```text
Executing
 -Suspended
 -Resuming
 -Executing
```

When the result cannot be confirmed:

```text
Executing
 -Uncertain
```

A Run must not depend on process lifetime.

---

## 21. Worker Yield

Workers must not directly call other external capabilities.

If a Worker needs an additional capability:

```text
Worker
 -Yield / Call Request
 -Runtime
 -Trust / Policy
 -Approval / Scoped Grant
 -Capability Host / Registry
 -Worker / Connector / Model
 -Result / Artifact
 -Resume
```

A Worker may request a need, but cannot decide:

- Whether it is authorized
- Which capability executes
- Whether Secrets are accessed
- Whether an external system is called
- Whether a SideEffect is allowed

---

## 22. Recovery

Recovery must cover at least:

- Application crash
- Device restart
- Worker crash
- Connector interruption
- Network failure
- Credential expiry
- Capability loss
- Uncertain SideEffect

If an external SideEffect may already have occurred:

> Delta must not blindly retry because local state is incomplete.

---

## 23. Run Ownership

Every executing Run must include at least:

- `run_id`
- `owner_device_id`
- `execution_epoch`

A Run may have only one execution owner at a time.

Other devices may:

- Observe
- Sync results
- Read Artifacts
- Prepare a handoff

But they may not directly Resume the Run.

---

## 24. Ownership Transfer

```text
Active Owner
 -Release
 -Authorized Transfer
 -New Owner
 -execution_epoch + 1
```

Rules:

1. Normal transfer is initiated by the current owner.
2. Temporary disconnection is not sufficient reason to steal ownership.
3. A new owner must obtain a valid new epoch.
4. Grants, dispatches, and results from an old epoch are invalid.
5. No valid arbitration basis means no epoch increase.
6. If ownership cannot be determined, move to `Suspended / Uncertain`.
7. Availability must not be gained through double execution.

Future mechanisms may include:

- Authenticated handoff
- Managed coordination
- Lease
- Timeout
- Fencing

No device may create a new epoch based only on a local timeout.

---

## 25. Control Plane and Data Plane

```text
delta-core
-Control / Authority Plane

Worker / Connector / STT
-Data / Execution Plane
```

Default transfer semantics:

```text
Small data
-Pass by Value

Large data
-Reference / Handle / Stream
```

Possible implementations include:

- File reference
- File handle
- Stream
- Shared memory
- Platform-specific IPC

The exact threshold is defined by Artifact / Worker IPC contracts.

---

## 26. Artifact

Artifact Authority belongs to Core.

Core governs:

- Identity
- Ownership
- Reference tracking
- Validation
- Provenance
- Temporary-resource lifecycle
- Garbage collection

Workers only receive scoped Artifact access through Grants.

Crash, timeout, or abnormal exit must not orphan temporary resources indefinitely.

---

## 27. Capability

> **v1.0:** "Resolver" removed as a required architecture layer. The canonical flow now aligns with `target-architecture.md` and `capability-abi.md`: Capability Host / Registry -Trust / Policy -Approval -Scoped Execution Grant -Capability Host -Capability ABI -Execution.

Canonical flow:

```text
Runtime / Skill
 -Capability Registry / Manifest
 -Trust / Policy
 -Allow / Confirm / Deny
 -Approval
 -Scoped Execution Grant
 -Capability Host
 -Capability ABI
 -Execution
```

A device manifest may report:

- Available
- Unavailable
- Degraded

The registry may use:

```text
Manifest Cache
+
Event-driven Invalidation
```

Platform differences must not create separate business workflows.

---

## 28. Execution Grant

An Execution Grant must bind at least:

- `run_id`
- `execution_epoch`
- `capability_id`
- `target`
- `permission_scope`
- lifecycle / expiry

Execution Grants exist to:

- Prevent TOCTOU errors
- Prevent post-revocation execution
- Prevent dispatch to unavailable Workers
- Prevent old owners from continuing execution
- Enforce resource and concurrency limits

When `execution_epoch` changes:

> Grants from the previous epoch are invalid.

Lease-style semantics are only required for capabilities involving:

- Scarce resources
- Explicit concurrency ceilings
- Remote nodes
- Cross-device execution

Possible mechanisms:

- Reservation
- Lease
- Fencing

---

## 29. Skill

A Skill describes:

> How a class of work should be performed.

A Skill may declare:

- Workflow
- Instruction
- Prompt
- Capability requirements
- Input / Output Schema
- Validation
- Template
- Domain rules
- Optional Script Reference

A Skill must not directly control:

- Worker
- Connector
- Vault
- Scheduler

---

## 30. Optional Script

A Skill may include script resources, but scripts have no execution authority.

They must execute through:

```text
Skill
 -Script Reference
 -Capability Requirement
 -Runtime
 -Trust / Policy
 -Approval / Scoped Grant
 -Worker
 -Sandbox
```

The following is prohibited:

```text
Skill -exec(script)
```

---

## 31. Extension State

Extensions may own domain-specific state, but may not create a Shadow Database or second State Authority.

Core provides namespaced storage:

```text
extension_state/<extension_id>
```

Core governs:

- Namespace ownership
- Permission
- Version
- Persistence
- Change metadata
- Sync policy
- Migration metadata

The Extension governs:

- Domain schema
- Payload interpretation
- Domain validation

Supported sync policies may include:

- No Sync
- Versioned Replace
- Explicit Conflict
- Append-only
- CRDT
- Custom Merge Contract

If no sync policy is declared:

> Default to `No Sync`.

LWW must not be the universal default.

---

## 32. Extension Merge

If a Custom Merge produces:

- Error
- Timeout
- Invalid output
- Schema mismatch
- Validation failure
- Untrusted result

It must degrade to:

```text
Explicit Conflict
```

> Expose conflict rather than silently corrupt domain state.

---

## 33. Worker

Workers are controlled execution environments.

Examples:

- `delta-worker-python`
- `delta-worker-browser`
- `delta-worker-office`
- `delta-worker-media`

A Worker does not own:

- Task
- Run
- Scheduler
- Approval
- Secret
- Memory
- Retry Authority
- Orchestration Authority

Worker input:

- Task Parameters
- Execution Grant
- Artifact Reference
- Execution Limits

Worker output:

- Result
- Artifact
- Metadata
- Progress
- Yield
- Error

---

## 34. Worker Sandbox

Workers are untrusted by default.

```text
Core
 -Execution Grant
 -Sandbox Host
 -Worker
```

At minimum, the Sandbox must constrain:

- Filesystem
- Network
- Secrets
- Process
- CPU
- Memory
- Timeout
- Environment
- Working directory

Possible implementations include:

- WASM / WASI
- Restricted process
- OS sandbox
- Browser sandbox
- Trusted Native

Foundation and Advanced workers share the same default trust level.

---

## 35. Trusted Native

Trusted Native admission requires all of the following:

1. Official maintenance
2. Auditable source
3. Completed security review
4. Explicit permission scope
5. No second Authority
6. Ordinary Sandbox cannot reasonably support the capability
7. Independent security tests
8. Explicit allowlist entry

Otherwise, the Worker remains untrusted.

---

## 36. Trusted Native Kill Switch

Trusted Native must support immediate revocation.

```text
Vulnerability / Policy Violation
 -Kill Switch
 -Revoke Execution Grant
 -Disable Dispatch
 -Fallback Sandbox / Capability Unavailable
```

Requirements:

- Stop issuing new Grants immediately
- Do not wait for a software release
- Terminate active execution when required
- Record a Ledger Security Event

The Kill Switch revokes:

- Execution Grant
- Capability Grant
- Dispatch eligibility

It does not automatically destroy:

- OAuth Token
- API Key
- Vault Credential

If interrupted execution has not started an external SideEffect:

```text
-Cancelled / Failed
```

If an external SideEffect may already have been sent but its result is unknown:

```text
-Uncertain
-Reconciliation / Adjudication
```

Rollback must never be assumed.

---

## 37. Connector

A Connector describes:

> How Delta communicates with an external system.

Examples:

- `delta-connector-gmail`
- `delta-connector-google-drive`
- `delta-connector-github`
- `delta-connector-slack`
- `delta-connector-notion`

A Connector is responsible for:

- API protocol
- Request / response mapping
- Capability mapping
- Service description
- Error mapping

A Connector is not responsible for:

- OAuth UI
- Vault
- Task
- Scheduler
- Approval
- Credential Authority

---

## 38. delta-connect

`delta-connect` is responsible for:

> Establishing and maintaining user connections to external services.

Responsibilities include:

- Catalog
- Authentication
- OAuth
- Connection lifecycle
- Health

Fixed relationship:

```text
Connect
= establish connection

Connector
= use the service

Core
= Credential + Binding + Capability Authority
```

Connect does not control Connector lifecycle.

---

## 39. Credential Recovery

```text
Connector
 -ERR_AUTH_REQUIRED
 -Core
 -Suspend
 -Auth / Connect
 -Vault Update
 -Resume
```

A Connector must not maintain a parallel authentication state machine.

---

## 40. Secrets / Vault

Secret Authority belongs to the local Core on each device.

Secret classes:

- Device-local
- E2EE Syncable
- Re-auth Required

### Device-local

Never leaves the current device.

### E2EE Syncable

May be synchronized only after explicit user authorization.

### Re-auth Required

A new device must authenticate again.

---

## 41. E2EE Vault Sync

```text
Device A Vault
 -Encrypt for Device B
 -Opaque Blob
 -delta-sync / Relay
 -Device B
 -Local Decrypt
 -Device B Vault
```

Requirements:

- Relay never receives plaintext
- Managed Sync never holds decryption keys
- Device revocation is supported
- Key rotation is supported
- Secret types may prohibit sync
- Device enrollment and Secret synchronization require separate authorization

Vault Authority remains local to each device Core.

---

## 42. delta-stt

`delta-stt` is responsible only for:

```text
Audio
 -Speech Recognition
 -Transcript Stream
```

It is not responsible for:

- Meeting understanding
- Risk assessment
- Follow-up questions
- Action items
- Memory

Platform implementations may differ:

```text
Desktop -sidecar / process
Mobile  -native / embedded (future roadmap)
```

---

## 43. Meeting Intelligence

Meeting Intelligence is an Advanced capability.

It may provide:

- Real-time topic recognition
- Follow-up suggestions
- Conflict analysis
- Risk prompts
- Decision capture
- Action items
- Post-meeting summary

Canonical path:

```text
delta-stt
 -Transcript Stream
 -delta-core
 -Meeting Intelligence
```

Meeting Intelligence must not:

- Bypass Trust
- Directly mutate State
- Directly write Vault
- Create its own Runtime
- Use a privileged private API

---

## 44. Source / Provenance

External content is:

> **Data by default, not Instruction.**

Sources must be classifiable, at minimum, as:

- User Instruction
- System / Policy Instruction
- Trusted Local Rule
- External Content
- Connector Content
- Retrieved Document
- Model-generated Suggestion

Capability Requests must carry at least:

- `origin`
- `source_refs`
- `provenance_chain`
- `external_influence`

---

## 45. Provenance Propagation

Provenance must propagate across:

- Connector
- Worker
- Skill
- Model
- Runtime

Example:

```text
External Email
 -Worker Extract
 -Model Summary
 -Model Planning
 -Capability Request
```

Intermediate processing must not silently convert:

```text
External Influence
```

into:

```text
User Instruction
```

A Trust decision may alter effective handling, but the original provenance must remain available.

---

## 46. Provenance Trust Decisions

Original provenance is immutable.

Trust decisions may change **effective trust classification**, not historical origin.

Example:

```text
Original:
origin = External Content
external_influence = true

Trust Decision:
classification = Reviewed / Accepted

Effective Handling:
higher trust treatment may be allowed
```

It must never be rewritten as:

```text
origin = User Instruction
external_influence = false
```

Any Trust decision that reduces restrictions, raises trust, or clears external-influence restrictions must record:

- `decision_id`
- Actor
- Reason
- Source references
- Previous class
- New class
- Scope
- Logical time

Such events belong in the Authority / Security Ledger.

---

## 47. Provenance Compaction

`provenance_chain` does not need to preserve infinite physical history.

Allowed techniques:

- Summary
- Compaction
- Anchor
- Critical-lineage retention

But the following must survive compaction:

- Source identity
- Trust class
- External influence
- Critical lineage

> Physical history may be compacted. Security meaning may not.

---

## 48. Prompt Injection

Provenance is only one defense layer.

Prompt-injection safety ultimately depends on:

- Source classification
- Trust
- Execution Grant
- Approval Policy
- SideEffect control

Model output such as:

- Tool Call
- Capability Request
- Plan Change

does not itself constitute execution authorization.

---

## 49. delta-sync

`delta-sync` synchronizes:

> Device -Device state.

Eligible data may include:

- Workspace metadata
- Settings
- Preferences
- Memory
- Skill definitions
- Automation definitions
- Extension state
- Artifact metadata
- Selected history
- Ledger
- E2EE Secret payload

The following must not be directly synchronized:

- Executing Run
- Local lock
- Worker Runtime
- Ephemeral memory
- Device execution environment
- Secret plaintext

---

## 50. Multi-Device Consistency

The following require strict consistency:

- Run Ownership
- `execution_epoch`
- SideEffect Authority
- Approval consumption
- Critical permission state

The following may be eventually consistent:

- Settings
- Preferences
- Memory
- Artifact metadata
- History
- Most Extension state

> Delta must not become a globally strong-consistency distributed database.

---

## 51. Ledger Positioning

Ledger is:

- Audit Evidence
- Causal Record
- Security Record
- Recovery Evidence

Ledger is **not**, by default:

- Event Store
- Primary business State
- Full-replay database

Business-state Authority is:

```text
Durable State
+
Snapshot
```

Recovery may use:

```text
Durable State
+
Snapshot
+
Required Ledger Evidence
```

The entire system must not depend on full Ledger replay for recovery.

---

## 52. Ledger Classification

### Authority / Security Events

Must enter the Durable Ledger:

- Approval
- Approval Policy
- Capability Grant
- Execution Grant
- Ownership Transfer
- SideEffect
- Secret Access
- Security Event
- Authority Migration
- Critical Provenance

### Operational Events

May be persisted and later compacted:

- Step transition
- Runtime status
- Recovery transition

### High-Frequency Telemetry

Must not enter the Authority Ledger:

- Model token stream
- STT partial chunk
- Frequent progress updates
- UI typing state
- High-frequency performance metrics

---

## 53. Telemetry

Telemetry is **metadata-first**.

Default telemetry may include:

- Duration
- Token count
- Byte count
- Latency
- Status
- Error code
- Queue depth
- Resource usage

Default telemetry must not include:

- Full prompt
- Model token content
- STT transcript content
- Email body
- Artifact body
- Secret
- Credential

Exemption from Authority/Audit semantics does not exempt Telemetry from Secret and Privacy governance.

---

## 54. Content Diagnostics

Content-bearing diagnostic collection is allowed only in an explicit Debug / Diagnostic mode.

It requires:

- Explicit user authorization
- Explicit scope
- Explicit lifetime
- Necessary redaction
- Ability to disable immediately

Enabling content diagnostics is itself a:

```text
Consent / Security Event
```

and must be recorded in the Ledger.

Specific redaction methods belong in the Telemetry / Diagnostic Privacy Contract.

---

## 55. Ledger Ordering

Cross-device Ledger events must include at least:

- `device_id`
- `local_sequence`
- `logical_time`
- `wall_clock_time`

Semantics:

```text
logical_time
-causal ordering

wall_clock_time
-human display
```

Audit order must not depend only on system wall clock.

---

## 56. Ledger Snapshot / Compaction

Logical append-only semantics do not require infinite physical growth.

Allowed:

- Snapshot
- Compaction
- Archive
- Garbage collection

```text
Active State + Ledger
 -Verified Snapshot
 -Checkpoint / Hash Anchor
 -Compaction
 -Archive / GC
```

Critical audit continuity must remain intact.

---

## 57. SideEffect

SideEffects are classified as:

- Idempotent
- Reversible
- Irreversible
- Uncertain

Examples:

- Send email
- Delete file
- Publish content
- Modify external service
- Payment

A SideEffect must pass:

- Trust
- Approval / Approval Policy
- Execution Ownership
- Ledger requirements

Core generates:

- `operation_id`
- `idempotency_key`

Blind retry is prohibited.

---

## 58. SideEffect Durable State

Active SideEffect information belongs in Durable State.

At minimum:

- `operation_id`
- `idempotency_key`
- `side_effect_type`
- `status`
- `execution_epoch`
- `reconciliation_state`
- Relevant target

Ledger records audit evidence of these transitions.

Therefore:

```text
Durable State
= current execution fact

Ledger
= audit evidence
```

Ledger is not the sole source of `idempotency_key`.

---

## 59. Uncertain SideEffect

```text
Uncertain
├── Reconciled -Completed
├── Reconciled -Failed
├── User Adjudicated -Accepted Completed
├── User Adjudicated -Accepted Failed
└── Remains Uncertain
```

Human adjudication must not rewrite original history.

Ledger records:

- Original status
- Adjudication result
- Evidence
- Actor
- Logical time

---

## 60. Approval Policy

Supported approval forms:

- One-shot Approval
- Scoped Approval Policy

A Policy may constrain:

- `run_id`
- Operation type
- Target
- Resource scope
- Quantity
- Time window
- Risk level
- Additional constraints

Example:

```text
Current Run
Only *@company.com
Maximum 5 emails
Attachments denied
Valid for 30 minutes
```

Inside scope:

```text
automatic execution
```

Outside scope:

```text
request Approval again
```

---

## 61. Approval Quota

Quantity/time consumption belongs to the current Run Authority.

It binds at least:

- `run_id`
- `execution_epoch`
- `policy_id`
- `consumption_state`

Ownership transfer moves remaining quota together with Durable Run State.

Quota consumption and SideEffect state must be updated atomically at the same Commit Boundary.

Recommended semantic states:

```text
Available
 -Reserved
 -SideEffect Dispatch
 ├── Completed -Consumed
 ├── Failed before effect -Released
 └── Uncertain -Held
```

An Uncertain SideEffect temporarily consumes quota until it is reconciled as not having occurred.

This prevents duplicated consumption after ownership transfer.

---

## 62. Approval Policy Lifecycle

Approval Policies must be:

- Revocable
- Expirable
- Auditable
- Run-scoped when appropriate
- Capability-scoped when appropriate
- SideEffect-scoped when appropriate

The following are prohibited:

- Permanent silent approval of high-risk actions
- Automatic scope expansion
- Advanced capabilities bypassing Approval

---

## 63. External Dependency Fault Isolation

The following are unstable external boundaries:

- Model provider
- Connector service
- Remote Worker
- Managed Connect
- Managed Sync
- External API

Design for:

- Timeout
- Cancellation
- Bounded retry
- Backoff
- Circuit breaker
- Fallback
- Degraded mode

But:

> Retry is not SideEffect blind retry.

SideEffects remain governed by:

- `operation_id`
- `idempotency_key`
- Uncertain
- Reconciliation

---

## 64. delta-sdk

`delta-sdk` is the single public development contract for the ecosystem.

It includes, at minimum:

- App-Core Contract
- Runtime Lifecycle Contract
- Fast Path / Escalation Contract
- Worker Contract
- Worker Yield Contract
- Connector Contract
- Skill Contract
- Extension State Contract
- Capability Schema
- Execution Grant Schema
- Approval Policy Schema
- Permission Schema
- Artifact Contract
- Source / Provenance Contract
- Telemetry / Diagnostic Privacy Contract
- Stream Contract
- Manifest Schema
- Sync Payload Schema
- Ledger Event Schema
- Test Kit

The following are prohibited:

- edition-specific SDK forks
- privileged private SDKs

---

## 65. Contract Stability

Governance freezes:

- Semantics
- Boundaries
- State-machine expectations
- Security requirements
- Error semantics
- Compatibility requirements

Governance does **not** freeze:

- Internal data structures
- Runtime implementation
- Scheduler implementation
- Storage implementation
- IPC implementation
- Cache implementation

> Public contracts evolve slowly. Internal implementation may be continuously refactored.

---

## 66. SDK Version Governance

Use Semantic Versioning:

```text
Major -Breaking Change
Minor -Compatible Feature
Patch -Compatible Fix
```

Deprecated contracts must:

- Be explicitly marked
- Provide a replacement
- Appear in release notes
- Declare a removal version

---

## 67. Model

Model is a Core-managed Capability.

Supported protocol families:

- OpenAI-compatible
- Anthropic-compatible

Canonical flow:

```text
Runtime
 -Policy / Capability
 -Model Adapter
 -Local / Self-hosted / Remote
```

A Model may propose:

- Next Action
- Capability Request
- Tool Call
- Plan Change

But it has no Execution Authority.

---

## 68. State Governance

Authority writes must be:

- Atomic
- Durable
- Recoverable
- Fail Closed when integrity is uncertain

The following are prohibited:

- Dual Authority
- Shadow State
- Silent fallback
- Partial commit followed by continuation

Migration policy:

```text
Hard Cut
+
Explicit Migration
```

Code rollback and data migration/recovery are governed separately.

---

## 69. Architecture and Source Governance

The **Delta Architecture Governance Specification** governs:

- Technical boundaries
- Authority
- Runtime
- Trust
- Dependencies
- Public contracts

The **Delta Source and Licensing Governance Specification** governs:

- Foundation open-source scope
- Licenses
- Third-party dependency compatibility
- Attribution and notices

If the two documents conflict:

1. Document the conflict.
2. Analyze impact.
3. Perform architecture governance review.
4. Record the ruling.
5. Update all affected documents in the same change.
6. Preserve the review record.

Neither document may silently override the other.

---

## 70. Naming Rules

Top-level names are fixed:

```text
delta-app
delta-core
delta-connect
delta-sync
delta-stt
delta-sdk
```

Extensions:

```text
delta-connector-<service>
delta-worker-<capability>
delta-skill-<capability>
```

Canonical Core internal names:

```text
runtime
trust
state
secrets
work
capability
automation
learning
model
ledger
storage
```

Avoid:

```text
delta-runtime
delta-trust
delta-state
delta-work
delta-model

utils
common
extra
hub
plus
pro
```

unless a future architecture review explicitly creates an independent lifecycle and boundary.

---

## 71. Prohibited Patterns

The following product forks are prohibited:

```text
Edition-specific Core
Platform-specific Core

Extension-owned Runtime
Parallel Runtime

Privileged SDK
Internal-only SDK

Local Trust
Remote Trust
```

The following implementation patterns are prohibited:

- UI directly mutating business facts
- Skill directly controlling Worker
- Skill directly controlling Connector
- Skill directly executing scripts
- Worker directly calling external capabilities
- Worker becoming Orchestrator
- Connector owning Secret Authority
- Connect controlling Connector lifecycle
- Advanced Worker automatically becoming Trusted Native
- Trusted Native without Kill Switch
- Sync becoming Runtime
- Managed service becoming Authority
- Advanced bypassing Trust
- Meeting Intelligence using a private backdoor
- Extension creating a Shadow Database
- Custom Merge silently discarding conflict
- Old `execution_epoch` continuing execution
- Epoch growth without valid Ownership Transfer
- Secret plaintext entering Sync Relay
- External Content automatically becoming Instruction
- Provenance being silently removed after multi-hop processing
- Telemetry collecting business content by default
- Debug content capture without user consent
- Fast Path bypassing Trust
- Fast Path bypassing Approval Policy
- Fast Path bypassing Authority Ledger requirements
- Ledger becoming primary business State
- Mandatory full Event Sourcing
- SideEffect blind retry

---

## 72. Architecture Change Threshold

A new top-level `delta-*` module must prove all of the following:

1. Current boundaries cannot reasonably contain the capability.
2. The responsibility is genuinely independent.
3. The lifecycle is independent.
4. A distinct security or deployment boundary is required.
5. No second Authority is introduced.
6. The capability cannot be implemented through an internal Core module, Capability, Worker, Connector, Skill, or Extension.

Otherwise:

> Do not add a new top-level module.

---

## 73. Governance / Contract / ADR / Implementation Boundaries

### Governance defines

- Principles
- Authority
- Boundaries
- Non-negotiable security rules
- Long-term semantics

### Contract defines

- Schema
- State machine
- Command / Event
- Error semantics
- Compatibility
- Validation

### ADR defines

- Why a technical implementation is chosen
- Alternative analysis
- Platform-specific decisions
- Database / IPC / runtime choices

### Implementation defines

- Code
- Algorithms
- Performance parameters
- Internal structure

> Do not move Contract or implementation detail back into Governance unless it changes a long-term architectural invariant.

---

## 74. Contract Priority

After v1.0, Contract work proceeds in this order:

1. Runtime / Lifecycle / Fast Path Escalation
2. App-Core Command / Event / Stream
3. Capability / Execution Grant
4. SideEffect / Approval Policy
5. Worker IPC / Yield / Sandbox
6. Artifact Reference / Lifecycle
7. Source / Provenance
8. Telemetry / Diagnostic Privacy
9. Extension State / Merge
10. Connector / Credential / Error
11. Sync / E2EE Secret Envelope
12. Ledger / Snapshot / Compaction

---

## 75. Required Contract Validation Scenarios

### Fast Path

Validate:

- Trust cannot be bypassed
- Grant cannot be bypassed
- Approval quota is consumed correctly
- Authority Ledger requirements are honored
- Fast -Heavy escalation is lossless
- Committed Steps are not re-executed

### Provenance

Validate:

- Multi-hop propagation
- Multi-source merging
- External influence inheritance
- Trust-decision audit
- Compaction without loss of security meaning

### SideEffect

Validate:

- Crash before dispatch
- Crash during request
- Crash after remote commit
- Uncertain reconciliation
- Idempotency recovery
- Quota reservation / hold / release semantics

### Telemetry

Validate:

- Secret redaction
- Content disabled by default
- Debug consent
- Retention
- Diagnostic shutdown
- Redaction strategy by data class

---

## 76. Stream Contract

The Stream Contract must support at least:

- STT transcript
- Model token stream
- Worker progress
- Run event
- Meeting Intelligence

It must define:

- Ordering
- Backpressure
- Cancellation
- Reconnect
- Resume
- Error propagation
- Latency budget
- Buffer policy

Meeting Intelligence is a real-time stress case.

---

## 77. Implementation Phases

v1.0 is a governance baseline, not a requirement to implement all advanced mechanisms at once.

### Phase 1

- App-Core Contract
- Runtime
- Lifecycle
- Fast Path / Escalation
- Execution Grant
- Worker IPC
- Artifact
- SideEffect
- Basic Provenance

### Phase 2

- Worker Yield
- Extension State
- Approval Policy
- Trusted Native Kill Switch
- Ledger classification
- Snapshot / Compaction
- Telemetry Policy

### Phase 3

- Multi-device Sync
- E2EE Vault Sync
- Ownership automation
- Lease / Fencing
- Managed services

> Freeze semantics first. Implement incrementally.

---

## 78. Final Governance Model

```text
                      Delta

        ┌──────────── Flexible Outside ────────────-
Platforms
-Windows / macOS / Linux (current) / Mobile (future roadmap)

Models
-Local / Self-hosted / Remote

Extensions
-Skill / Worker / Connector

Services
-Foundation / Advanced / Managed

Implementation
-continuously replaceable and evolvable

        └─────────────────────────────────────────-                           -                    Stable Contracts
                           -        ┌──────────── Strict Inside ──────────────-
                       delta-core
                           -                Runtime / Trust / State
                           -           Capability / Grant / Ownership
                           -             SideEffect / Ledger / Recovery

        └─────────────────────────────────────────-```

---

## 79. Final Principles

Delta permanently maintains:

```text
One Delta
One Authority Plane
One Runtime
One Trust model
One State model
One Capability model
One SDK
One logical Authority per business fact
```

Normal execution:

```text
Normal case
-Fast Path

Risk escalation
-Escalate

Dynamic need
-Yield back to Core

High-risk action
-Scoped Approval

External SideEffect
-Durable Operation State

Unknown SideEffect result
-Reconcile / Adjudicate

Cross-device execution
-Ownership + Epoch

Secrets
-Local Authority + Optional E2EE

Large data
-Reference / Handle / Stream

Long-running system
-Snapshot + Compaction

Extension state
-Core Namespace

External content
-Data by Default

Observability
-Metadata-first Telemetry

External failure
-Isolation + Graceful Degradation
```

The final governance principles are:

> **Continuous Stability. Secure Efficiency. Flexible Outside, Strict Inside.**

> **Core facts stay strict; peripheral capabilities stay flexible. Critical boundaries stop safely; non-critical failures degrade gracefully. The Control Plane remains unified, the Data Plane remains open, public contracts stay stable, and internal implementations continue to evolve.**

> **Foundation preserves control, Advanced increases productivity, and Managed provides convenience. All three share the same Core, Runtime, SDK, Trust model, and Authority rules. Delta must never split into two architectures.**

**From v1.0 onward, the top-level architecture is governed by this specification. Schemas, detailed state machines, IPC mechanics, compaction thresholds, backpressure parameters, performance limits, and concrete algorithms belong in Contracts, ADRs, and implementation specifications. A new top-level `delta-*` component requires the full architecture-change threshold defined in this document.**
