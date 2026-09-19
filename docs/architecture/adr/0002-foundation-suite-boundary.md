# ADR-0002: Foundation and Optional Extensions

Status: Accepted

## Context

Delta needs a stable, open Foundation while allowing optional first-party and
third-party capabilities to evolve independently.

## Decision

Delta Foundation owns the complete application and all product authority.
Optional extensions integrate only through public SDKs, extension APIs,
versioned contracts, manifests, and controlled Worker boundaries.

Foundation never imports an extension implementation and never requires one to
start or provide baseline functionality.

Installed extension manifests are isolated under
`<state>/extensions/<source>/` so one extension cannot overwrite another.

## Consequences

- Delta Foundation remains independently useful and auditable.
- Optional capabilities can evolve without forking Core.
- Extensions cannot create a parallel Runtime or Trust plane.
- Public compatibility contracts, rather than repository layout, define the
  integration boundary.
