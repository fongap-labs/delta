# Delta Architecture Governance

Version: 1.0

Status: Architecture Baseline


## 1. Purpose

This document defines the fundamental architecture rules of Delta.

It applies to:

- Human maintainers
- Developers
- AI coding agents
- Automated tools

Any change affecting architecture, dependency, runtime, capability, or repository boundary MUST follow this document.


---

## 2. Product Identity


Delta is a:

> Local-first extensible personal work agent.


Delta provides:

- User-controlled execution
- Local-first operation
- Extensible capability model
- Recoverable workflows
- Explicit trust boundaries


Delta is one product.

The architecture MUST maintain:

- One Delta
- One Authority Plane
- One Runtime


No extension or capability may create a parallel Delta architecture.


---

## 3. Capability Layers


Delta consists of two capability layers.


## Delta Foundation

Role:

Foundation Layer


Delta Foundation provides the complete foundation of Delta.

Responsibilities:

- Application runtime
- Core architecture
- Trust model
- State model
- Capability framework
- SDK
- Contracts
- Basic capabilities


Delta Foundation MUST remain:

- Complete
- Runnable
- Extensible


Delta Foundation is not:

- A limited version
- A demo version
- A dependency-only framework


---

## Delta Suite

Role:

Advanced Capability Layer


Delta Suite provides advanced capabilities built on Delta Foundation.


Delta Suite may provide:

- Advanced workflows
- Professional skills
- Advanced workers
- Verified connectors
- Managed capabilities


Delta Suite improves Delta experience.

Delta Suite MUST NOT replace Delta architecture.


---

## 4. Core Principle


Delta has one Core.


Core owns:

- Runtime authority
- Execution authority
- State authority
- Trust authority
- Approval authority
- Capability authority


No layer may create:

- Another Core
- Another Runtime
- Another State model
- Another Trust model


---

## 5. Extension Principle


Capabilities extend Delta through:

- SDK
- Contracts
- Schemas


Extensions may provide:

- Skills
- Workers
- Connectors
- Domain capabilities


Extensions MUST NOT bypass Core authority.


---

## 6. AI Agent Rules


Before any architectural modification:

1. Read architecture documents.
2. Identify affected layer.
3. Confirm dependency direction.
4. Prefer extension over core modification.


AI agents MUST NOT:

- Create duplicate runtime systems.
- Create duplicate authority systems.
- Move core responsibility into extensions.
- Introduce hidden dependencies.


If architecture intent is unclear:

Request architecture review.