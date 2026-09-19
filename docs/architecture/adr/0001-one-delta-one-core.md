# ADR-0001: One Delta, One Core


Status:

Accepted


## Context


Multiple independent cores create:

- Architecture fragmentation
- Inconsistent authority model
- Difficult maintenance


## Decision


Delta maintains:


- One App
- One Core
- One Runtime


All capabilities extend the same architecture.


## Consequences


Positive:

- Stable architecture
- Clear ownership
- Easier extension


Negative:

- Core changes require discipline