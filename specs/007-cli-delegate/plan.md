# Implementation Plan: CLI Delegate Backend (Claude Code / Codex)

**Branch**: `007-cli-delegate` | **Date**: 2026-09-16 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `specs/007-cli-delegate/spec.md`

## Summary

Adds `cli_delegate` (Claude Code, Codex) as a third chat backend alongside the existing `local` and
`api_key` providers — reusing an existing Claude/Codex subscription rather than a metered API key.
Both backends implement the existing `ProviderAdapter` trait (`stream_chat`/`list_models`) exactly
like `AnthropicAdapter`/`LocalAdapter` today, so `turn.rs`'s turn/step loop needs **no changes**: a
delegate adapter drives its own subprocess to completion and streams the result back as
`StreamChunk::Delta`/`Done`, the same escape hatch non-tool-calling local models already use. The
genuinely new work is (1) spawning and talking to `claude -p`/`codex app-server --stdio` as
subprocesses with vault-portable, host-isolated credentials, and (2) bridging each backend's own live
approval mechanism into haex-vault's _existing_ `tool-permission-request`/`respond_tool_permission` gate
from 003-agent-tool-loop, so Manual/Auto/Plan apply identically to delegate tool use — confirmed
feasible for both backends by live verification against installed CLIs during this planning session
(see [research.md](research.md)), which overturned this feature's own draft assumption that Claude
Code would need a separate "upfront batch approval" design.

## Technical Context

**Language/Version**: Rust 1.77.2, edition 2021 (backend, `src-tauri`), TypeScript 5 (frontend, Nuxt
4 SPA) — unchanged from 003-agent-tool-loop.
**Primary Dependencies**: `tokio::process` (already used by `chat/tools/cli.rs`'s host-command tool)
for spawning `claude`/`codex` subprocesses. `rmcp` 3.3.0 (already pinned for the MCP _client_ path)
gains real (non-test-only) use of its `server` + `transport-io` features: MCP's stdio transport means
`claude` spawns the `--mcp-config` server as _its own_ child process, not something reachable
in-process — so haex-vault's permission-prompt-tool MCP server actually runs in a small separate process
(the haex-vault binary re-invoked with a hidden internal flag via `std::env::current_exe()`, never
initializing Tauri/GUI), which then relays each approval request back to the main process over a
local Unix-domain-socket/named-pipe hop using `tokio::net` (feature `net`, newly added — no new
crate; a local-HTTP-server alternative was considered and explicitly rejected per operator direction:
prefer a pure-Rust IPC mechanism over spawning an HTTP stack, even though this whole feature is
desktop-only regardless of that choice, since subprocess spawning itself is impossible on mobile).
Codex needs no such bridge: haex-vault already owns its `app-server --stdio` pipe directly. No MCP
dependency needed for Codex either way: `codex app-server --stdio` speaks its own JSON-RPC protocol
directly (hand-rolled newline-delimited JSON-RPC framing, no crate), verified against the installed
CLI's own `codex app-server generate-json-schema` output — see [research.md](research.md) §2. Parsing
Claude Code's `--output-format stream-json` (newline-delimited JSON events) is new parsing code,
structurally similar to but distinct from `adapters/anthropic.rs`'s SSE parsing.
**Storage**: SQLite via SQLCipher through haex-crdt. `storage/providers.rs`'s `ProviderKind::CliDelegate`
variant and `credentials: Option<Vec<u8>>` column already exist (Etappe-2 groundwork, unused since);
this feature is what actually populates `credentials` for `CliDelegate` rows (a Claude OAuth token or
Codex's `auth.json` bytes) and relaxes `add_provider`'s current validation, which explicitly rejects
setting `adapter` for `CliDelegate` (`providers/mod.rs:112-123`) — `adapter` becomes the vendor
discriminator (`"claude"`/`"codex"`) for delegate rows, mirroring its existing use for `api_key`
vendors. No new column needed for the credential itself. `chat_messages.tool_source` (existing,
free-text, already carries `"mcp"`/`"cli"`) gains delegate-specific values (`"cli_delegate:claude"`,
`"cli_delegate:codex"`) for tool activity a delegate reports as its own — no schema change, per
`chat/tools/mod.rs:69`'s existing doc comment that this column is meant to be extended this way.
**Testing**: `cargo test --lib` / `cargo test --test <name>`; tests in sibling `*_tests.rs` files,
never inline `#[cfg(test)] mod tests` (confirmed repo-wide, no exceptions). `pnpm typecheck` for the
new frontend connect/backend-selection UI.
**Target Platform**: Desktop only (Linux primary, macOS/Windows in mind) — a delegate backend spawns
a real subprocess, same desktop-only reasoning as the existing host-CLI tool; no compile-time feature
gate needed (matches `chat/tools/cli.rs`'s precedent of a runtime availability check, not a
`#[cfg(...)]` split like `llm-cpu`).
**Performance Goals**: Stopping a delegate-backed response terminates the underlying subprocess
within the same short window `chat/tools/cli.rs`'s existing process-group kill already achieves for
the host-command tool (SC-005) — no new, independent performance target.
**Constraints**: Per-invocation ephemeral `CLAUDE_CONFIG_DIR`/`CODEX_HOME` + disposable `cwd`, deleted
after the process exits regardless of success/failure (RAII/`finally`); `CLAUDE_CODE_OAUTH_TOKEN`
(not `--bare`) since `--bare` doesn't support subscription/OAuth login (verified —
[research.md](research.md) §3); the approval-bridge MCP server (Claude Code side) and the
`app-server` JSON-RPC session (Codex side) both terminate haex-vault's existing `pending_tool_approvals`
oneshot flow from `turn.rs`/`session.rs`, not a parallel approval mechanism; a single delegate call is
bounded by the same overall turn/step model as any other backend (no new tool-round cap needed, since
the delegate itself is one step from `turn.rs`'s perspective, per the Summary above).
**Scale/Scope**: Single-user, one active model/session at a time (existing `ChatState`/
`ActiveSession` invariant, unchanged); two new backend kinds, each with exactly one live subprocess
per in-flight request, no persistent cross-request session (a fresh `claude -p`/`codex app-server`
process per request, matching "backend selection is made per request" from spec.md Assumptions and
keeping cancellation/host-isolation uniform with the existing host-CLI tool's per-call lifecycle).

## Constitution Check

_GATE: Must pass before Phase 0 research. Re-check after Phase 1 design._

Evaluated against the haex-vault Constitution (`.specify/memory/constitution.md`, hard-pinned from
haex-hive, revision `336eaf1e`):

| Principle                                               | Status | Rationale                                                                                                                                                                                                                                                                        |
| ------------------------------------------------------- | ------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| I. No Secrets in Git                                    | ✓ PASS | Delegate credentials (Claude OAuth token / Codex `auth.json`) are stored the same way existing `api_key` credentials already are: an encrypted-at-rest SQLite/SQLCipher column (`providers.credentials`), never a git-committed file. No new secret-handling pattern introduced. |
| II. No Local Absolute Paths in Versioned Config         | ✓ PASS | Per-invocation temp directories (`CLAUDE_CONFIG_DIR`/`CODEX_HOME`, disposable `cwd`) are runtime state, not versioned config.                                                                                                                                                    |
| III. Project Identity Is Device-Independent             | ✓ PASS | No changes to project-identity or device-scoping mechanisms; delegate credentials live in the same vault-scoped `providers` table as existing credentials.                                                                                                                       |
| IV. Cross-Repo References Pin Immutable Revisions       | ✓ PASS | `rmcp`'s `server` feature moves from test-only to a real dependency use, still under the existing exact version pin (`=3.3.0`) — no new external harness content, same reasoning 003's plan.md already applied to this same dependency.                                          |
| V. External Sources Are Opt-in Per Project              | ✓ PASS | N/A — no external harness content involved.                                                                                                                                                                                                                                      |
| VI. Self-Modifying Instructions Are Always Review-Gated | ✓ PASS | This planning session's edits to `docs/plans/2026-09-11-agent-tool-loop-design.md` (§8.2a addendum, §10 updates) and `specs/007-cli-delegate/spec.md` (clarification supersession) land through this feature's normal PR review, not silently.                                   |
| VII. Relay Unavailability Never Blocks Local Work       | ✓ PASS | Delegate calls are local subprocess + local vault reads/writes, independent of haex-vault's sync relay.                                                                                                                                                                          |
| VIII. No Concealment Instructions in Agent Output       | ✓ PASS | FR-005 requires every delegate response to visibly identify its backend and record its tool use; no hidden behavior introduced.                                                                                                                                                  |

**Result**: All gates PASS. No Complexity Tracking entry needed.

## Project Structure

### Documentation (this feature)

```text
specs/007-cli-delegate/
├── plan.md                    # This file
├── spec.md                    # Feature specification (existing, from /speckit.specify + /speckit.clarify)
├── research.md                # Phase 0 output (this command)
├── data-model.md              # Phase 1 output
├── quickstart.md              # Phase 1 output
├── contracts/
│   └── tauri-commands.md      # New/changed Tauri commands and events
├── checklists/
│   └── requirements.md        # Spec-quality checklist (from /speckit.specify)
└── tasks.md                   # Phase 2 output (/speckit.tasks — NOT this command)
```

### Source Code (repository root)

Extends the existing Tauri desktop structure; no new top-level project.

```text
src-tauri/src/
├── adapters/
│   ├── cli_delegate/                    # NEW module
│   │   ├── mod.rs                       # CliDelegateAdapter dispatch (Claude vs Codex by `adapter` column)
│   │   ├── claude.rs                    # `claude -p` process driver: argv build, temp CLAUDE_CONFIG_DIR/cwd,
│   │   │                                #   stream-json NDJSON parsing -> StreamChunk::Delta/Done
│   │   ├── codex.rs                     # `codex app-server --stdio` JSON-RPC session driver: temp CODEX_HOME,
│   │   │                                #   request/response framing, ServerRequest approval routing
│   │   ├── approval_bridge.rs           # Runs in the MAIN process: socket/named-pipe listener +
│   │   │                                #   translates a pending approval (either backend) into
│   │   │                                #   ChatState.pending_tool_approvals + tool-permission-request event,
│   │   │                                #   reusing turn.rs's existing oneshot-channel mechanism unchanged
│   │   ├── permission_mcp_server.rs     # Runs in the SEPARATE bridge child process (see lib.rs below) —
│   │   │                                #   rmcp `server`+`transport-io` stdio MCP server exposing the one
│   │   │                                #   `--permission-prompt-tool` tool; relays each call to
│   │   │                                #   approval_bridge.rs's listener over the socket/pipe
│   │   └── *_tests.rs                   # per module, repo convention (no inline #[cfg(test)] mod tests)
│   ├── mod.rs                           # + cli_delegate module registration (doc comment at mod.rs:9-10 updated)
│   └── types.rs                         # unchanged — StreamChunk::{Delta,Done} already sufficient (research.md §4)
├── providers/
│   └── mod.rs                           # build_adapter: replace CliDelegate's Err(InvalidInput) (line ~400) with
│                                         #   real CliDelegateAdapter construction; add_provider: allow `adapter`
│                                         #   for CliDelegate (currently rejected, line ~112-123), accept a
│                                         #   `credentials` payload from the new one-time connection commands;
│                                         #   + connect_cli_delegate / disconnect_cli_delegate Tauri commands
│                                         #   (spawn `claude setup-token`/`codex login` in an isolated temp dir,
│                                         #   capture the result, store it via existing provider insert path)
├── storage/
│   └── providers.rs                     # doc comment (line 13-17: "For local and cli_delegate ... it is None")
│                                         #   updated — cli_delegate now does populate credentials
├── chat/
│   ├── tools/
│   │   └── permission.rs                # unchanged — PermissionMode/Decision reused as-is by approval_bridge.rs
│   └── session.rs                       # pending_tool_approvals reused unchanged; + tracking of the delegate
│                                         #   subprocess handle so Story 5 (stop) can kill it, mirroring cli.rs's
│                                         #   process-group kill pattern
├── identity/
│   └── migrations.rs                    # no new migration expected (credentials/adapter columns already exist);
│                                         #   confirmed/revisited in data-model.md
└── lib.rs / main.rs                     # + a hidden internal-entrypoint branch, checked before Tauri init:
                                          #   `--internal-cli-delegate-approval-bridge --socket <path>` runs only
                                          #   permission_mcp_server::run_bridge_process(), then exits — no GUI

src/
├── composables/
│   └── useProviders.ts                  # already exposes add/delete/refresh (existing, currently unused by any
│                                         #   .vue component per Explore finding) — first real caller added here
├── components/settings/
│   └── ConnectDelegateProvider.vue       # NEW — first "add/connect provider" UI in the app (no existing
│                                         #   precedent; DefaultModelSetting.vue only reads providers today)
└── i18n/{de,en}/*.json                  # + strings: connect flow, delegate backend labels, unavailable/expired/
                                          #   reconnect messaging
```

**Structure Decision**: New `adapters/cli_delegate/` module bundles all delegate-specific process/
protocol/approval-bridge logic, mirroring how 003 isolated its new tool-registry logic in
`chat/tools/` rather than inflating existing files. `turn.rs` and `chat/tools/permission.rs` are
reused unmodified — the delegate adapters are additive at the `ProviderAdapter` boundary and at the
approval-bridge boundary, not a parallel execution path. The one exception is the hidden
`lib.rs`/`main.rs` entrypoint branch for the approval-bridge child process — unavoidable given MCP's
stdio transport model (`claude` spawns the server named in `--mcp-config` as its own child, so that
code cannot simply live in-process), kept to the smallest possible surface (one flag check before
Tauri initializes, delegating immediately to `cli_delegate::permission_mcp_server`). No new top-level
structure otherwise, no separate backend/frontend split beyond what already exists.

## Complexity Tracking

_No entries — Constitution Check found no violations._
