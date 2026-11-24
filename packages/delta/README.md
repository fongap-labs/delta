---
description: "Package map for Delta-owned capabilities: computer use, browser use, forge, worktree and LiveShare."
kind: "package-group"
---

# delta/ - Delta-owned capabilities

## Summary

The `delta/` group holds capabilities that Delta implements itself instead of absorbing experimental upstream code. Each capability is off by default and runs only after the user enables it. Package READMEs own the contracts; this group has no standalone subsystem page.

## Packages

- [`browser-use`](browser-use/README.md): web page operation through a Playwright provider.
- [`computer-use`](computer-use/README.md): authorized observation and operation of Windows application windows.
- [`forge`](forge/README.md): GitHub and GitLab change request access.
- [`liveshare`](liveshare/README.md): read-only, time-limited, revocable session sharing through a self-hosted HTTPS/WSS relay.
- [`worktree`](worktree/README.md): isolated Git working directories for sessions.
