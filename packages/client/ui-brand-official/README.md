---
description: "Delta sidebar brand renderer."
kind: "package-reference"
---

# @deepseek-ai/dsh-client-ui-brand-official

English | [中文](README.zh.md)

## Summary

This package renders the Delta mark and name through `sidebar.brand.mark` and `sidebar.brand.name`. It contains no upstream whale artwork, remote feedback or telemetry.

## Assets

Canonical Delta logos live under `resources/brand/`. The 256px copy in `src/client/assets/` exists only for browser bundling and must match the canonical logo.

## Runtime

The Host half is inert. The Client half fills only the two sidebar slots and leaves the conversation hero and home fallback unchanged. The package does not need to be activated by the default Web composition.

## Model Experience

None. This UI-only component makes no model or network requests.

## Known Limitations and Deferred Work

The existing two-slot contract and packaged Delta logo remain in use. Future brand migration must preserve the conversation's default rendering.
