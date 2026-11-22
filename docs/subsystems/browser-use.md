# Browser use

English | [中文](browser-use.zh.md)

Browser use lets a model inspect and operate web pages through a configured backend. DSH owns the task loop; the provider supplies browser operations and keeps browser state across turns of one live Session.

## Choose a provider

Delta retains the [shared browser-use registry](../../packages/browser-use/browser-use/README.md), but the previous experimental Playwright MCP, Chrome DevTools MCP, Stagehand, and browser runtime packages have been removed. Loading the registry alone does not enable browser tools.

A separately installed provider would have to register its name and own browser actions, resources, configuration, and Session lifecycle. The registry accepts only one named provider at a time; it neither performs browser operations nor selects a provider for the model.

## Session ownership

A launched browser belongs to the exact live Agent and Session that uses it. Calls across turns reuse that browser. Disposing the Session runtime closes its launched resources; reloading or forking a Session starts fresh browser state. Browser profiles and login state are not restored from the Session log.

An attached browser remains externally owned. The provider reserves it for one Session within that provider instance, preserves its existing browser state, and rejects simultaneous attachment by another Session. Teardown disconnects and leaves the external browser running. Separate DSH processes and other clients remain outside this reservation.

Provider shutdown stops tool admission and waits for owned work and resource cleanup before releasing the shared provider registration. Cancellation cannot undo a browser action already delivered.

## MCP initialization

An MCP provider initializes one client for each live Agent created after the provider loads. The existing serial `agent/created` event awaits connection and discovery before creation or resume completes and queued input runs. The client remains with the Session across turns. Startup failure or cancellation rejects creation or resume and triggers client cleanup.

If an attachment is busy, that activation continues without the browser and does not retry on later turns. After release, a newly created or resumed activation can acquire it. Loading or reloading the provider does not adopt already active Sessions. These initialization rules describe the former experimental browser runtime, which is no longer bundled in Delta.

## Tools and recorded results

Provider tools use the normal DSH execution pipeline and Session log. The providers own their tool schemas, result rendering, image support, configuration, and upstream limitations; the shared service adds no model-visible content. The removed Stagehand integration is no longer part of Delta. The shared registry does not provide browser model routing, credential reuse, inference capture, or Session usage accounting; a separately installed provider would own its tool results.

Browser MCP connections also expose [resources and server instructions](mcp.md). Resource calls addressed to a browser server use its Session queue and reject other Sessions; server instructions are assembled only for its owning Session.

The [decision record](../../.agents/notes/implemented/architecture/2026-09-12-browser-use-provider-registration.md) explains the registration-only service and per-Session ownership.

<!-- BEGIN GENERATED cordis-surface (gen-cordis-catalog.ts) — do not edit between markers -->

<a id="cordis-surface"></a>

## Cordis API

Generated from source by `scripts/gen-cordis-catalog.ts` (verified fresh by `pnpm run verify-cordis-catalog` in doc-sync; regenerate with `pnpm run gen-cordis-catalog`) — the language sides differ only in locale-specific paired document paths. Signature blocks use a `ts cordis-catalog` fence and keep the original source JSDoc; dispatch modes are defined in the [primer](../cordis-primer.md#dispatch-modes), and the framework-inherited `ctx` API lives in [cordis-api/inherited.md](../cordis-api/inherited.md).

<a id="ctxbrowseruse--browseruseregistry"></a>

### `ctx.browserUse` — `BrowserUseRegistry`

Owns one optional provider registration in the shared browser-use service.

```ts cordis-catalog
/**
 * Reserve the sole provider slot until the contribution is disposed.
 * A second registration fails even when it repeats the current name. Providers
 * must stop their tools and await owned work before releasing this registration.
 * @param name - provider-owned name used in registration diagnostics.
 * @returns the effect disposer for this exact registration.
 */
register(name: BrowserUseProviderName): () => Promise<void>
```

Source: [`packages/browser-use/browser-use/src/index.ts`](../../packages/browser-use/browser-use/src/index.ts)
<!-- END GENERATED cordis-surface -->
