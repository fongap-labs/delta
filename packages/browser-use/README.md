---
description: "Browser-use capability packages for selecting and registering one browser backend."
kind: "package-group"
---

# packages/browser-use

English | [中文](README.zh.md)

## Summary

Browser-use providers let models inspect and operate web pages. This group owns exclusive provider registration. Experimental providers supply browser tools and manage each Session's browser resources.

## Table of Contents

- [Packages](#packages)
- [Related documentation](#related-documentation)
- [Dev Note](#dev-note)

-----

<a id="packages"></a>
## Packages

Choose one provider and mount the shared registration service.

| Package | Role | ctx key |
|---|---|---|
| [`browser-use`](browser-use/README.md) | Exclusive named provider registration | `ctx.browserUse` |

<a id="related-documentation"></a>
## Related documentation

- [Browser use](../../docs/subsystems/browser-use.md) — registry behavior, opt-in Playwright MCP and Session ownership.
- [Experimental packages](../experimental/README.md) — the restored Playwright MCP provider, Session runtime and optional Browser Use bundle; Chrome DevTools and Stagehand remain removed.

<a id="dev-note"></a>
## Dev Note

None.
