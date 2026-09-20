# Delta Capability Model

Version: 1.0

Status: Architecture Baseline


## 1. Purpose


This document defines the capability model of Delta.


The goal is:

- Keep Core stable.
- Enable extensibility.
- Allow Foundation and optional extensions to share the same architecture model.


---

# 2. Capability Principle


Delta follows:


Capability over Fork


When adding functionality:

Prefer:

Capability

over:

Core modification


Prefer:

Extension

over:

New subsystem


---

# 3. Capability Definition


A Capability is an independently identifiable unit of ability.


A Capability defines:


- Identity
- Purpose
- Required permissions
- Dependencies
- Execution model
- Lifecycle


A Capability does not own system authority.


---

# 4. Capability Types


Delta supports:


## Skill


Purpose:

Provide reusable knowledge or task logic.


Examples:

- Writing assistance
- Analysis workflow
- Domain procedures


---


## Worker


Purpose:

Provide controlled execution capability.


Examples:

- File processing
- Data transformation
- External execution


Workers MUST execute under Core authority.


---


## Connector


Purpose:

Provide external system integration.


Examples:

- Service API
- Data source
- External application


Connectors MUST NOT bypass Trust boundaries.


---


## Domain Capability


Purpose:

Provide specialized workflows.


Examples:

- Professional workflows
- Industry scenarios
- Advanced automation


---

# 5. Capability Lifecycle


Every capability follows:


Define

↓

Register

↓

Validate

↓

Authorize

↓

Execute

↓

Report


---

# 6. Capability Authority


Core owns:


- Capability registration authority
- Execution permission
- Runtime scheduling
- State transition


Capability implementation does not own authority.


---

# 7. Foundation Capability


Delta Foundation provides:


- Core capability framework
- Basic capabilities
- Extension interfaces


Foundation capabilities must remain general-purpose.


---

# 8. Extension Capability


Optional extensions may provide:


- Advanced capabilities
- Specialized workflows
- Official maintained extensions


Extension capabilities MUST use:

- Delta SDK
- Delta Contracts


---

# 9. Forbidden Patterns


Do not create:


## Capability Core

A capability must not become another platform.


## Capability Runtime

A capability must not create another execution engine.


## Capability Authority

A capability must not create independent permission control.


## Capability State

A capability must not create independent system state.