# Extensions reach the agent over MCP and the host over a typed bridge

Status: accepted; direction A settled by spec 018 (see the end)
Date: 2026-09-21

## Decision

Holzi hosts haextensions (signed web bundles that run in a sandboxed iframe).
Traffic between an extension and holzi runs in two directions. Each direction
has its own protocol, its own code path, and the same permission model.

| Direction | Caller → callee                        | Protocol                                                      | Authorization                                                                                         |
| --------- | -------------------------------------- | ------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------- |
| A         | holzi (agent) → extension              | **MCP**: holzi is the client, the extension is the server     | Tool declared in the manifest and confirmed at install; per-call approval gate by the declared effect |
| B         | extension → host operation (file, DB…) | **Typed request/response over the extension's `MessagePort`** | Manifest grants; anything else prompts the user                                                       |

**Direction A.** An extension declares the tools it offers in its manifest and
serves them over MCP at runtime. Holzi connects as an MCP client and registers
each tool in the existing `ToolRegistry` with source `haextension`. The risk
class follows the effect the signed manifest declares (`read`, `change`,
`destructive`); MCP tool annotations are hints and are never read. A tool
call needs an open window of the extension; holzi opens one minimized when none
exists. A call invokes the extension's functions directly and need not be
visible in its UI.

**Direction B.** The extension calls typed host methods (the protocol the
haex-vault SDK already speaks). The frontend only relays JSON between the
iframe port and one Tauri command. A single Rust chokepoint identifies the
extension from its window (never from the payload), checks the permission, and
calls the host function directly. Host operations never pass through the agent.

**One permission model.**

- An extension declares every host operation it needs, and every tool it
  exposes, in its signed manifest. The user confirms both in one dialog at
  install. A manifest change that adds permissions needs a new confirmation.
- A confirmed permission passes without a prompt.
- Any other request prompts the user: allow or deny, with a checkbox to
  remember the choice. A request the manifest never declared is labeled as
  such. An explicit denial takes precedence over a grant.
- A host operation triggered while an agent tool call is running on that
  extension is checked against the _extension's_ grants, and its prompt says
  it was triggered by an agent call.

**An extension reaches a model only through a route the user set.** The bridge
dispatches from a deny-by-default allowlist of host methods. Neither the bridge
nor the tool connection offers MCP sampling, a path into the chat runtime, or
extension-to-extension calls; holzi's MCP client refuses every request from the
extension's server except `ping`. A contract test enumerates the bridge
methods and fails if any reaches chat or model code.

## Rationale

The permission model the operator wants — manifest declaration, install-time
confirmation, silent pass for confirmed grants, prompt with "remember" for the
rest — already exists in haex-vault. MCP adds nothing to it, so direction B
keeps that design instead of re-expressing it.

Using MCP for direction B would also weaken the wall around the LLM. A "host
MCP server for extensions" would sit beside the agent's tool registry, one
wiring mistake from exposing it. Separate protocols and code paths make the
wall structural rather than a convention.

Direction B keeps the haex-vault SDK protocol, so the same extensions run in
haex-vault and holzi. The SDK gains an MCP server part for direction A and
loses nothing.

MCP fits direction A: the agent already consumes MCP tools, and `tools/list`
and `tools/call` carry the schemas and results it needs. It fits direction B
poorly: file transfer, streaming, frequent database queries, and push events
are that direction's normal traffic.

## Consequences

- Specs split as: 017 extension host with the permission chokepoint
  (direction B), 018 extension tools for the agent (direction A, plus the SDK
  addition), 019 adding tools to the existing haextensions.
- The manifest schema gains a tools block. That touches `vault-sdk` and
  `haextension` as well as holzi; cross-repo references stay pinned to full
  commit SHAs.
- Direction B pushes data changes as well as answering requests: the bridge relays
  `vault-data-changed` (`src-tauri/src/vault_events.rs`) to an extension, cut down to the
  tables the extension may read, so an extension's view follows a write from another device,
  window or agent as the built-in views do. The chokepoint filters by the extension's grants,
  never by anything the extension sends.
- Holzi does not plan native-webview extension windows. The iframe plus
  port model leaves one chokepoint and needs no per-command Tauri ACL
  allowlist.
- Prompt handling for direction B needs the queue and duplicate-request
  handling that haex-vault's prompt code already has.
- The bundle format and the two independent checks of extension SQL that spec 017
  settled are recorded in
  [ADR-0008](./0008-extension-bundle-signature-and-sql-authorizer.md).

## Direction A settled (spec 018, 2026-10-12)

- **Effect from the manifest.** Each tool declares its effect in the signed
  manifest instead of being `Risky` by default (operator decision
  2026-10-10): an extension can do only what its permissions allow, and the
  signature binds the declaration. Each tool is stored as a permission of the
  kind `agentTool` (action = effect, target = tool name); a tool left unticked
  at install asks before every call, in every mode.
- **Transport.** MCP messages travel as the port message `haexspace:mcp`
  beside the bridge's messages; the frontend relays them to one Tauri command
  per direction, and Rust identifies the extension from the frame session as
  for the bridge. holzi's client uses `rmcp` 3.5's `(Sink, Stream)` transport,
  which needs no extra feature, with protocol version `2025-11-25`. The
  vault-sdk serves the tools with a small MCP server of its own (no new
  dependency), from haex-space/vault-sdk@280cebf63eba72422d1ddd62e1edeb5ad7e7d3cc.
  The SDK's native Tauri mode gets no MCP path.
- **Models for extensions.** "The LLM is never reachable from an extension"
  becomes "only through a route the user set": the operator wants extensions
  to use holzi's agent once the user can choose which model answers which
  extension's questions (a later spec). Until then no such route exists, and
  neither the bridge nor the tool connection is one.
- **Attributing host operations.** The frame session counts the agent calls
  in flight; a host prompt raised while the count is above zero says it was
  triggered by an agent call.

## Considered alternatives

- **MCP in both directions.** Rejected: it blurs the wall around the LLM,
  breaks compatibility with haex-vault's extensions, fits streaming and binary
  traffic poorly, and does not simplify a permission model that is already
  specified.
- **Host operations routed through the agent.** Rejected: the LLM must never be
  reachable through an extension, and host operations must be direct function
  calls.
- **Native-webview windows that call Tauri commands directly.** Rejected for
  holzi: every extension-facing command would need its own ACL entry and its
  own permission check.

## References

Pinned, for porting behavior rather than copying:

- `haex-space/haex-vault` @ `8dce379d94e18fcd42c3b73686a06f984ca3f574`:
  `src-tauri/src/extension/permissions/` (checker and manager),
  `src/composables/usePermissionPrompt.ts` (prompt queue and decisions),
  `src/composables/extensionMessageHandler.ts` (request routing).
- `haex-space/vault-sdk` @ `502593e84b8d289b0986a2777754d6bd8f52da5e`:
  `src/messages.ts` (`MessagePort` handshake), `src/client/external.ts` and
  `src/commands/ai.ts` (existing route for tool-call style requests).
