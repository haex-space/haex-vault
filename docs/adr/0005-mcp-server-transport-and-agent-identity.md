# External agents reach holzi through one in-process MCP server, with per-agent credentials

Status: proposed
Date: 2026-10-26

## Context

Spec 021 opens holzi to external agents (Claude Code, Codex, other MCP clients, and the CLI
delegates of specs 007/009). `docs/design/founding.md` makes MCP the only way into holzi from
outside. ADR-0006 reserved this number for the decision on how agents connect and how holzi knows
who is calling. The spec requires that an agent can start the registration itself (clarification
2026-10-26), that rights are kept per agent and per device, and that the feature works on every
platform holzi runs on.

## Decision

**One server, in the app process.** The MCP server runs inside the holzi process that holds the
open vault (ADR-0003), built on `rmcp`. It computes each agent's tool list and checks every call
with the same function (`mcp_server::grants`). Nothing outside the app process decides rights.

**Two ways in, both to that server.**

| Way             | Platforms                        | How                                                                                                                                                           | Who the caller is                                          |
| --------------- | -------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------- |
| Program (stdio) | Linux, macOS, Windows            | The agent starts `holzi --mcp-agent <slug>`; this helper relays stdio to a Unix socket (0700 dir, peer UID checked) or a named pipe of the app                | The credential the helper presents, issued at registration |
| HTTP            | all, the only one on Android/iOS | Streamable HTTP on loopback with MCP authorization (OAuth 2.1, PKCE, dynamic client registration); "authorize" is the registration dialog in the holzi window | The refresh token of that client, which is the credential  |

**Credentials, not names.** The name an agent reports is shown as its own claim and never checked.
Registration issues a random credential; holzi stores only its hash. Revocation, a block, or 90
days without a connection invalidate it. Delegates get a credential that holzi passes when it
starts them; the user configures nothing.

**The caller comes from the entrance.** Every call runs as `Caller::ExternalAgent { id }` in Rust
services and as caller `externalAgent` in the frontend action runner, never from request content.

## Consequences

- Web pages cannot reach the program way at all; the HTTP way checks `Host` and `Origin` and
  requires a token, following spec 049's defence pattern.
- A process running as the same OS user that reads an agent's credential file can act as that
  agent. holzi does not defend against that; rights per agent, approval mode, the call log and
  revocation limit the damage (spec 021, assumptions).
- The helper is the holzi executable with a flag, so the setup snippet must carry the real path of
  that executable, which differs per platform and packaging (AppImage).
- Rust remains the only place that checks rights; the frontend action runner only enforces
  `agentCallable` and the guardrail lock as before.
- Later specs (browser tools) add a rights group to `mcp_server::grants`; existing agents do not get
  it until the user grants it.

## Alternatives rejected

- **HTTP only, with a static bearer token in the agent's config**: the agent could not trigger the
  registration itself, and the user would copy secrets around.
- **A separate MCP server process that drives the app**: a second copy of tool list and rights.
- **Trusting the client name or a slug alone**: any local program could claim it; a credential at
  least ties rights to a registration the user saw.
