# ADR-0057: Workspace sensitivity and what may be sent to a model endpoint

Status: **Proposed** (2026-10-11). The owner asked for options the user chooses from, not a fixed policy (2026-10-11). This ADR defines those options and the rules that hold whichever option the user picks.

## Context

Delta is local-first, but every model turn sends content to the configured model endpoint:

- the messages;
- the excerpts of files a tool read;
- the tool results.

For research and CMC work that content can be unpublished formulations, batch data or stability results. Today (checked, not assumed):

1. **Endpoint resolution.** `ModelAuthority::resolve_runtime_config` (`crates/delta-core/src/model_authority.rs`) resolves a model id to a protocol, a `base_url` and a key. Any configured endpoint can be used for any workspace.
2. **No record of model requests.** The run ledger records tool calls, artifacts, validation and `source.read`, but it has no event for "content of this run was sent to endpoint X". The trust strip can say what a run read, not where it went.
3. **Workspace state.** Workspaces are stored in `core.db` (`workspaces` table: path, last used). The workspace trust authority is `workspace_trust.json` (`control_plane.rs`). Neither carries a sensitivity level.
4. **Approvals already gate other outbound actions** (chat messages, files, connector calls). Model requests are not covered, because they are how the agent works rather than an action it takes.

Organisations differ. Some allow a public cloud model for everything. Some allow only a company gateway. Some allow nothing to leave the machine for certain projects. A single built-in policy would be wrong for most of them, so the user chooses.

## Options offered to the user

Each workspace has one **egress level**, chosen by the user in the workspace settings. The levels are:

| Level | What may receive this workspace's content | Fits |
|---|---|---|
| **Open** | Any configured endpoint | Public material, personal use |
| **Approved endpoints** | Only endpoints the user marked as approved for this level (for example a company AI gateway) | Organisations with a sanctioned cloud or gateway |
| **Private only** | Only endpoints on this machine or the private network: loopback, RFC 1918 and other private ranges, or endpoints the user explicitly marked as private | Confidential R&D data with a local or self-hosted model |
| **Ask each time** | Any configured endpoint, but before the first model turn of each run the user confirms the endpoint for that run | Mixed work where the user wants to decide case by case |

Two more choices, independent of the level:

- **Default for new workspaces.** The user picks which level new workspaces start at. Out of the box it is *Open*, which matches today's behaviour.
- **Endpoint marks.** In the model settings the user can mark an endpoint as *approved* or *private*. Delta can classify a loopback or private-network address by itself. It does not assume that a public host name is private.

### Alternatives considered

**A. One fixed policy for everyone** (for example "confidential workspaces may only use local models").
- Rejected: the owner asked for user choice, and one policy would block legitimate setups such as a sanctioned company gateway.

**B. User-chosen levels, enforced by the Runtime (recommended).** This is the table above.

**C. Labels only, no enforcement.** The user tags a workspace, and Delta only shows a warning.
- Rejected: a label that the Runtime does not enforce is a promise the product cannot keep. It also contradicts "people control consequences" (blueprint section 4.3).

## Decision (proposed): B

Whichever level the user picks, these rules hold:

1. **The Runtime enforces it.** Before each model request, `resolve_runtime_config` (or its caller) checks the workspace's level against the resolved endpoint.
   - A disallowed endpoint fails the turn with a typed error that names the level and the endpoint.
   - The Runtime never falls back silently to another endpoint.
2. **Only the user changes it.** The level is changed only from the workspace settings in the desktop app.
   - No model, Skill, worker, automation or Learning can change it.
   - Lowering the level (toward *Open*) asks for confirmation and is recorded.
   - Raising it takes effect at once.
3. **Skills recommend, the user decides.** A Skill may declare a recommended minimum level. A Suite CMC Skill, for example, may recommend *Private only*.
   - If the workspace's level is lower, Delta shows the recommendation and offers to raise it.
   - It does not refuse to run: the user decides.
4. **Every model request is recorded.** A new ledger event `model.request` is appended for each request. It holds:
   - the run id and turn;
   - the endpoint host;
   - the model id;
   - the workspace's level at that moment;
   - the byte count sent;
   - the ids and SHA-256 of the sources whose content was in the request.

   It never contains the content itself. The trust strip shows "Sent to <host> (N requests, M KB)" for the run that made an artifact.
5. **Scope.** This ADR covers model endpoints. Other outbound paths (web search queries, remote MCP servers, connectors) already go through tools and approvals. Whether the egress level should also restrict them is a follow-up decision, listed below.

## Consequences

- Workspace authority gains an `egress_level` per workspace and a default for new workspaces. A missing value reads as the user's chosen default, which is *Open* unless changed, so existing workspaces keep working.
- Model settings gain per-endpoint marks (*approved*, *private*).
- New ledger event `model.request`, defined in `runtime-public-contract.md` (what it records and what it does not claim).
- Desktop:
  - a level selector in the workspace settings;
  - the endpoint marks in the model settings;
  - the confirmation for *Ask each time* and for lowering a level;
  - the Skill recommendation notice;
  - the "Sent to" line on the trust strip.
- Tests required:
  - a disallowed endpoint fails the turn and sends nothing;
  - *Private only* accepts loopback and RFC 1918 addresses and rejects a public host that is not marked;
  - a host name that resolves to a private address but is not marked is classified by its resolved address at request time, and the check is not fooled by DNS changes between check and connect;
  - lowering a level without confirmation does not change it;
  - nothing but the desktop settings command can change a level;
  - `model.request` is written for every request and contains no message content;
  - *Ask each time* asks once per run, not once per turn;
  - a run that is resumed under ADR-0053 asks again if its level is *Ask each time*.

## Questions for the owner

1. Accept B, with the four levels above?
2. Is *Open* the right out-of-the-box default for new workspaces (keeps today's behaviour), or should the first-run setup ask the user to pick one?
3. Should the egress level also restrict web search, remote MCP servers and connectors, in a follow-up ADR?
