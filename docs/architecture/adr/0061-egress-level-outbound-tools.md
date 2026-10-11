# ADR-0061: The workspace egress level also governs outbound tools

Status: **Accepted** (owner, 2026-10-11). Partly implemented: see [Implementation status](#implementation-status). Follows [ADR-0057](0057-workspace-sensitivity-model-egress.md). The owner decided that the egress level must also cover web search, remote MCP servers and connectors, in a separate ADR (2026-10-11).

## Context

ADR-0057 gives each workspace a user-chosen egress level (Open, Approved endpoints, Private only, Ask each time) and enforces it on model requests. Model requests are not the only way content leaves the machine. Tool arguments are model-chosen text, and a search query or a connector query can carry the same confidential content as a prompt. Today (checked, not assumed):

1. **Web tools.** `web_search` and `web_fetch` are declared `risk_level="medium"`, `requires_approval=True`, category `web`. Since ADR-0058 (baseline work capabilities) they are native Rust tools (`crates/delta-core/src/web/mod.rs`); the older Python tools in `integrations/web/` declare the same metadata. ADR-0060 adds a controlled browser that uses the same address guard.
   - Policy maps approval-gated medium risk outside local categories to L3 (`policy.rs`, `band_level`).
   - L3 is allowed only with an explicit approval or a standing policy grant (`restrict_grants`). So a standing policy can let web search run without asking.
   - `web_search` goes to a fixed provider host (DuckDuckGo by default; Tavily or Brave with a key, `integrations/web/providers.py`). `web_fetch` goes to any public host the address guard accepts.
2. **MCP servers.** There are two transports (`mcp_runtime.rs`, `mcp.rs`):
   - **HTTP:** a remote server at a configured URL.
   - **stdio:** a local process. Whatever network access that process makes on its own is invisible to Delta.

   Each server's tools default to `requires_approval: true`, but the server configuration can set it to `false`, which makes them low risk (`mcp_runtime.rs`).
3. **Connectors.** Connector tools (mail, messaging, Notion and others in `integrations/connectors/`) go to fixed SaaS hosts.
   - Read tools can run without approval (`risk_level="low"` when approval is not required).
   - Send tools require approval.
   - Suite adds more connectors and an interactive browser that reaches arbitrary hosts.
4. **Worker grants.** The Capability ABI grant has a `network` list of allowed outbound targets (`capability/abi.rs`). Whether the operating system enforces it was not verified for this ADR.
5. **Ledger.** The ledger records tool calls with redacted arguments (`redact.rs`). It records neither the destination host nor the egress level in force.

The result: with ADR-0057 alone, a *Private only* workspace would still send a model-chosen search query to a public search engine whenever a standing policy or a server setting allows it.

## Options

**A. Model requests only (ADR-0057 as is).**
- Rejected by the owner: the level would not mean what the user thinks it means.

**B. One level governs every outbound path, with per-path overrides the user chooses (recommended).**
- The level sets a default rule for each class of destination.
- The user may override a path for a workspace in either direction. Loosening needs confirmation and is recorded.
- Plus:
  - one setting does what the user expects;
  - it still allows the common CMC case of confidential data plus public literature search, as a deliberate, visible choice.
- Minus: the per-path rules are part of the contract and must be shown clearly.

**C. Separate independent switches per path, no level.**
- Plus: maximal flexibility.
- Minus: easy to misconfigure; the user has to understand every path to be safe; it contradicts ADR-0057's single choice.

## Decision: B

### 1. Destination classes

Every tool that can send data off the machine declares its destination class in its metadata, and the Rust policy reads it:

| Class | Examples | What Delta can know |
|---|---|---|
| `fixed_public` | `web_search` provider, SaaS connectors, a remote HTTP MCP server on a public host | The exact host |
| `any_public` | `web_fetch`, browser automation | Only the host of each call |
| `private` | Loopback or private-network hosts, or hosts the user marked private under ADR-0057 | The exact host |
| `local_process` | stdio MCP servers, local workers with a network grant | Nothing about the process's own traffic |

A tool in category `web`, `connector` or `mcp` that does not declare a class is treated as `any_public`: it fails closed.

### 2. Default rule per level

| Level | `fixed_public` | `any_public` | `private` | `local_process` |
|---|---|---|---|---|
| **Open** | As today (approval settings apply) | As today | As today | As today |
| **Approved endpoints** | Allowed only for hosts the user approved | Each call needs approval, and no standing policy applies | Allowed | Allowed only if the user marked the server or worker *local-only* |
| **Private only** | Blocked | Blocked | Allowed | Allowed only if marked *local-only* |
| **Ask each time** | Each call needs approval, and no standing policy or "no approval" setting applies | Same | Same | Same |

What "blocked" means:

- The tool is not offered to the model in that workspace.
- If it is called anyway, dispatch refuses it with a typed error that names the level and the destination.
- Nothing is sent.

What *local-only* means: the user's statement that a local process does not reach the network. Delta records it as the user's statement and does not claim to verify it.

### 3. Per-path overrides

In the workspace settings the user can override the default rule for a single path in that workspace. The paths are:

- web search, by provider;
- web fetch;
- each MCP server;
- each connector.

The rules for overrides:

- **Loosening** (for example, allowing `web_search` in a *Private only* workspace for public literature) shows what will leave the machine (model-chosen query text to the named host) and asks for confirmation.
- **Tightening** takes effect at once.
- Every override is recorded, and the workspace settings list all of them.
- No model, Skill, worker, automation or Learning can set an override.
- A Skill may recommend an override or a stricter rule. The user decides, as in ADR-0057.

### 4. Approvals cannot widen the level

Where the level requires per-call approval, standing policy grants and per-server `requires_approval: false` do not apply in that workspace. The approval card names:

- the destination host;
- the workspace's level;
- the exact text that will be sent.

### 5. Recording

The ledger event of every outbound tool call gains an `egress` object. It holds:

- the destination class;
- the host;
- the level in force;
- the override in force, if any;
- the decision (allowed, approved, blocked).

It holds no content beyond the already redacted arguments. The trust strip's "Sent to" line (ADR-0057) includes these hosts.

### 6. Before the first-run choice

ADR-0057 point 5 applies to outbound tools too: until the user has chosen a default level, outbound tool calls are refused like model requests.

## Consequences

- **Tool metadata** (Python `ToolMetadata`, MCP tool descriptors, Suite connector descriptors) gains a destination class and, where known, the host. Suite connectors and the browser declare theirs through the public contracts.
- **Policy** (`policy.rs`) evaluates the workspace's level and overrides before risk banding, and ignores standing grants where point 4 says so.
- **Workspace authority** stores per-path overrides beside the egress level of ADR-0057. Model settings and MCP settings gain the *local-only* mark.
- **Desktop:** the per-path override list in the workspace settings, with confirmation for loosening; the approval card fields of point 4; the hosts on the trust strip.
- **Tests required:**
  - *Private only* hides and refuses `web_search`, `web_fetch`, remote HTTP MCP tools and connector tools, and sends nothing;
  - a tool in a governed category without a destination class is treated as `any_public`;
  - an unmarked stdio MCP server is refused in *Private only* and allowed once marked *local-only*;
  - *Approved endpoints* allows only approved hosts, and asks per call for `web_fetch`;
  - *Ask each time* asks per call even with a standing policy or `requires_approval: false`;
  - a loosening override without confirmation does not take effect;
  - only the desktop settings command can set an override;
  - every outbound call's ledger event carries the `egress` object;
  - before the first-run choice, outbound tool calls are refused.

## Owner's answers (2026-10-11)

1. B accepted, with the destination classes and default rules above.
2. Per-path overrides that loosen the level are allowed, with the confirmation and recording of point 3.

## Implementation status

Step 1 (runtime decision and ledger record):

- Every outbound tool call is decided with the workspace level before it is proposed. A refused call does not run; a call that must be asked is approved for that call only, and standing grants and automatic approval do not apply to it.
- Tools declare `destination`. Web tools and browser automation are `any_public`; remote HTTP MCP servers are `fixed_public` with their URL, judged by where they resolve; stdio MCP servers are `local_process`. A governed category (`web`, `mcp`) without a declared destination is `any_public`.
- `tool.proposed` carries the `egress` object (`class`, `host`, `level`, `decision`).

Deviations from the decision above, to be closed in step 2:

- `web_search` is `any_public`, not `fixed_public`. Its engines come from a credential-store value that is not read at dispatch, so the gate cannot yet name the provider hosts. `any_public` is the stricter class: in *Approved endpoints* it asks for each call instead of allowing approved providers.
- Local MCP servers can be marked *local-only* (`set_local_only`, with confirmation). The mark is stored in the egress authority and read by the gate. Settings ▸ Privacy lists the local (stdio) servers and sets the mark.
- Tools that the level refuses for every call are not offered to the model (`offered_to_model`). A call refused for a particular target is still refused at dispatch.
- Per-path rules (`set_path_rule`, point 3): a rule for one path (`web_search`, `web_fetch`, `browser`, `open_url`, or `mcp:<server>`) in one workspace, or for every workspace. It is `allow`, `ask` or `deny`, replaces the level's decision for that path, and is the only way a rule can be set (desktop settings command). A rule that lets more content go than the level does now asks for confirmation; removing it takes effect at once. The ledger's `egress` object names the path and the rule in force.
- Connector tools are governed: a tool in category `connector` with no declared destination is `any_public` (fails closed), and its path is `connector:<name>` (`ToolMetadata.connector`), so a rule can name one connector. Suite connectors have not yet declared a destination: most of them call several hosts, and a single `destination_url` per tool cannot describe that. A per-tool host list is the next contract change, before Suite declares `fixed_public` for its connectors.
