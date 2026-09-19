# ADR-0003: Capability over Fork


Status:

Accepted


## Context


Forking architecture for each feature creates:

- Duplicate runtime
- Duplicate state
- Long-term maintenance cost


## Decision


New functionality should be implemented as:

Capability

instead of:

Architecture fork


## Consequences


The ecosystem grows through extensions while Core remains stable.