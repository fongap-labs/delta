# Delta Core Architecture


Version: 1.0


## 1. Purpose


Delta Core is the architectural center of Delta.


Core provides control and authority mechanisms.

Core is not a capability container.


---

## 2. Core Responsibilities


Core owns:


## Runtime

- Task execution
- Workflow lifecycle
- Scheduling control


## Authority

- Permission validation
- Execution approval


## State

- System state management
- Consistency control


## Trust

- Verification
- Boundary enforcement


## Capability Management

- Registration
- Resolution
- Invocation control


---

## 3. Core Does Not Own


Core MUST NOT contain:

- Domain workflows
- External service implementations
- User-specific skills
- Specialized business logic


Those belong to capabilities.


---

## 4. Modification Rules


Core changes require architecture review.


Before modifying Core:

Check:

1. Can this be a capability?
2. Can this be an SDK extension?
3. Can this be a contract?


Prefer extension over modification.