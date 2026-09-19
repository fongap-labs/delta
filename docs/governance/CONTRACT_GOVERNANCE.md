# Delta Contract Governance

Version: 1.0

Status: Architecture Baseline


## 1. Purpose


Contracts define the stable boundary between:

- Delta Foundation
- Delta Suite
- External Extensions


Contracts are more stable than implementations.


---

# 2. Contract Principle


Delta follows:


Contract over Implementation


Consumers depend on:

- Interfaces
- Schemas
- Contracts


Consumers MUST NOT depend on:

- Internal modules
- Private implementation details


---

# 3. Contract Scope


Contracts define:


## Capability Contract

Defines:

- Capability identity
- Input
- Output
- Permission requirement


## Worker Contract

Defines:

- Execution interface
- Status reporting
- Artifact output


## Connector Contract

Defines:

- External integration boundary


## Artifact Contract

Defines:

- Data identity
- Reference
- Provenance


## Event Contract

Defines:

- System events
- State changes


---

# 4. Compatibility Rules


Contract changes require review.


Allowed:

- Add optional fields
- Extend capability metadata
- Add new interfaces


Requires review:

- Remove fields
- Change behavior
- Break compatibility


---

# 5. SDK Boundary


SDK provides:


- Extension interfaces
- Contract access
- Development utilities


SDK does not expose:


- Core internal implementation
- Authority internals
- Private state management


---

# 6. AI Agent Rules


Before changing contracts:


1. Check existing usage.
2. Consider compatibility.
3. Prefer extension.
4. Avoid breaking consumers.


Contracts are architectural assets.

Do not modify casually.