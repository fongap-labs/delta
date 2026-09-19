# Delta Extension Distribution Governance

**Version:** v1.1  
**Status:** Governance Baseline  
**Scope:** Discovery, installation, validation, update and removal of optional Delta extensions.

## 1. Principles

Delta Foundation is complete without extensions. Installing an extension never
changes which component owns Runtime, Trust, State, Approval or Recovery.

Every extension must be:

- explicitly identifiable by a stable source/id;
- isolated from other installed extensions;
- described by versioned public manifests;
- granted only the permissions it declares and the user approves;
- removable without damaging Foundation state.

## 2. Installed layout

The canonical local layout is:

```text
<delta-state>/
└── extensions/
    ├── <extension-a>/
    │   ├── capability-workers.json
    │   └── connector-catalog.json
    └── <extension-b>/
        ├── capability-workers.json
        └── connector-catalog.json
```

A single root-level manifest is not an extension boundary because independent
packages could overwrite one another.

## 3. Discovery

Foundation enumerates extension directories deterministically and validates each
manifest independently.

A missing extension directory means Foundation-only operation.

A malformed extension is skipped and reported. It must not prevent Foundation
or an unrelated valid extension from starting.

Duplicate capability ids, tool names or connector ids are rejected at the
public contract boundary.

## 4. Installation

Installation must:

1. create or update only the extension's own directory;
2. write manifests atomically;
3. never write credentials into manifests;
4. never mutate Foundation authority databases directly;
5. leave other extension directories untouched.

Uninstall removes only that extension's installed artifacts. Durable user data
owned by Foundation remains governed by Foundation APIs.

## 5. Permission model

Permissions are job-scoped Runtime grants. Extension metadata may request
filesystem, network, secret, process or hardware access, but only Foundation may
approve and enforce the effective grant.

No extension receives blanket access merely because it is installed.

## 6. Compatibility

Extensions depend on released public contracts. Production CI must test against
an explicit compatible Foundation version/ref rather than an unpinned default
branch.

Development may additionally test the latest Foundation main as a non-blocking
compatibility signal.

## 7. Security and provenance

Distribution systems may add package signatures, hashes, provenance, SBOMs and
revocation metadata. Those mechanisms are transport and supply-chain concerns;
they do not grant Runtime authority.

## 8. Hard rules

- Foundation never downloads or imports a privileged extension implementation.
- Extension code never bypasses CapabilityHost/Trust/Approval.
- One extension cannot overwrite another extension's manifests.
- Credentials never live in extension manifests.
- A broken optional extension cannot make local Foundation unusable.
