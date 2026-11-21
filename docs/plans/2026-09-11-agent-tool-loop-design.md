# Agent Tool Loop — Turn/Step, Tools, Retries, Cancellation

**Status**: §§1-7 (turn/step loop, tool registry, permission gate) implemented in
[PR #32](https://github.com/haex-space/haex-vault/pull/32), covering tasks.md Phases 1-4 (T001-T029).
Cancellation (§ retry/cancellation split, tasks.md Phase 5), retry (Phase 6), and polish (Phase 7)
remain design-only. Written 2026-09-11 from a brainstorming session triggered
by evaluating [deepseek-ai/deepseek-harness](https://github.com/deepseek-ai/deepseek-harness) as a
reference. Conclusion: its turn/step + append-only-session-log pattern is worth adopting; its Cordis
plugin architecture, subagent model, and Agent-SDK-based CLI integration are not (see §9).

**Relationship to existing documents**:

- [`CONTEXT.md`](../../CONTEXT.md) §"Chat runtime" defines `Adapter`, `Active Session`, `Session
Resolver` — this document extends that vocabulary with `Turn`, `Step`, `Tool`, `Permission Gate`.
- [`plans/001-desktop-mvp.md`](../../plans/001-desktop-mvp.md) already defines the three provider
  kinds (`local`, `api_key`, `cli_delegate`) and states `cli_delegate` must prove "reiner
  Chatbetrieb: keine Datei-/Shell-Tools, keine Hooks oder MCP-Ausführung" (line 167). §8 of this
  document revises that constraint.
- Current implementation: [`src-tauri/src/chat/commands.rs`](../../src-tauri/src/chat/commands.rs)
  (`send_message`), [`src-tauri/src/adapters/types.rs`](../../src-tauri/src/adapters/types.rs)
  (`ChatRequest`/`StreamChunk`/`AdapterStream`).

---

## 1. Why

`send_message` today is linear: one LLM call, one stream, done. Cancellation exists (a single
`AbortHandle` in `ChatState.current_generation`), but there is no tool-calling, no retry, and no
concept of a request spanning more than one model call. The user needs all three. deepseek-harness's
architecture doc names a useful shape for this — a **turn** is zero or more **steps**, a step is one
model request plus the tools it calls, and "model-visible means logged": anything that reaches a
model request must be reconstructable from the append-only session log. haex-vault already has an
append-only-ish `chat_messages` table with a `parent_id` chain; this design extends that chain
instead of introducing a new storage layer, and does not adopt Cordis (Node/TS plugin DI — wrong
language, wrong scale for a single-user local agent).

## 2. Turn/Step model

```
Turn
 └─ Step 1: LLM-Request → Stream (Delta*, then ToolCalls | Done)
      └─ if ToolCalls: for each call → execute via registry → append ToolResult
 └─ Step 2: LLM-Request (history now includes ToolCall/ToolResult rows) → Stream
      └─ ... repeats until Done with no ToolCalls, or Cancelled/Error
```

`send_message`'s current one-shot `while let Some(item) = stream.next()` becomes a loop function.
Each step's assistant text, tool call, and tool result are separate `chat_messages` rows chained by
`parent_id` — the log stays the single source of truth for reconstructing history for the next step,
matching CRDT sync's existing per-vault, no-device-discriminator model (no new table, no ADR-0001
change).

## 3. Data model changes

`src-tauri/src/adapters/types.rs`:

- `ChatRequest` gains `tools: Vec<ToolSpec>` (name, description, JSON-Schema), rebuilt from the tool
  registry each step.
- `ChatMessage`'s role (currently `User`/`Assistant` only) gains `ToolCall { id, name, input }` and
  `ToolResult { call_id, content, is_error }`, so history is reconstructable for the next step.
- `StreamChunk` gains `ToolCalls(Vec<ToolCall>)`, emitted before `Done`. Anthropic streams `tool_use`
  blocks incrementally (`input_json_delta`); the adapter buffers these into complete calls so
  `commands.rs` never handles partial JSON.

`src-tauri/src/storage/chat_messages.rs`: extend `MessageRole` with `ToolCall` and `ToolResult`;
add nullable columns `tool_name`, `tool_call_id`, `tool_input` (JSON text), `tool_is_error`, and
`tool_source` (`mcp` / `cli`; `cli_delegate` remains out of scope — see §8). Stays in the existing
`parent_id` chain.

**Retry interacts with persistence, not around it**: an LLM-request retry happens entirely before
any write. Nothing is persisted until a step succeeds — a failed attempt leaves no log row, only an
optional transient `chat-retry` UI event (not persisted), mirroring deepseek-harness's "cancellation
commits neither system nor users" rule.

## 4. Tool registry

A `Tool` trait: `name`, `description`, `input_schema`, `risk_class() -> Safe | Risky`, `async
execute(input) -> ToolResult`. Two source families sit behind it:

- **MCP-client tools**, discovered per connected MCP server via `tools/list`, wrapped to the same
  trait. Default `Risky` — unknown server code — until a user explicitly allowlists one.
- **Host-CLI tool**: one built-in tool spawning `tokio::process::Command`. Always `Risky`.

Individual vault-scoped read/write tools are deliberately deferred; they are a separate incremental
feature built on this registry and are not part of this tool loop. The registry lives in `ChatState`,
populated with the host-CLI tool at app start and with MCP tools on server connection (dynamic). Each
step builds its `tools: Vec<ToolSpec>` from it.

## 5. Permission gate

New device-scoped preference `chat.permission_mode` (`Manual` / `Auto` / `Plan`, default `Manual`),
modeled on Claude Code's mode switcher. Before executing any tool:

- `Manual`: always ask.
- `Auto`: `Safe` runs immediately; `Risky` (always true for the CLI tool) asks — "pause for anything
  risky."
- `Plan`: `Safe` runs; `Risky` is declined with a tool-result the model can see
  (`"blocked: switch out of Plan mode to run this"`), no prompt.

Mechanism: backend emits `tool-permission-request` (with a `request_id`), parks on a
`oneshot::Receiver` held in `ChatState.pending_tool_approvals: Mutex<HashMap<Uuid,
oneshot::Sender<Decision>>>`. A new command `respond_tool_permission(request_id, decision)` resolves
it. The wait races against the existing abort signal via `tokio::select!`, so a cancel during an open
approval request still takes effect immediately.

## 6. Retry policy

Scope: **LLM request only** (network timeout, 5xx, rate limit) — with backoff. Tool execution
failures are not retried by haex-vault; they become a `ToolResult { is_error: true }` and the model
decides whether to try again, same as any other tool-calling harness. No separate tool-retry policy.

## 7. Cancellation

Matches Claude Code's own behavior (verified by inspection, not guessed): cancelling kills the
running local tool process immediately, ends the **whole turn** right there — no further step runs
automatically — and returns control to the user. The cancellation is logged as `Cancelled` so a
later "continue" has the context, but nothing resumes on its own. This is the existing
`abort_current_generation` idea, widened from "abort the LLM stream" to "abort whatever the turn is
currently doing" (LLM stream, tool subprocess, or an open permission wait).

MCP cancellation is necessarily cooperative: for an in-flight `tools/call`, the client sends
`notifications/cancelled` with the original JSON-RPC request ID. The client treats a late response
for that cancelled ID as stale and discards it safely; it must not persist a result or emit a new
tool event for the cancelled turn. The UI marks the local turn as `cancelled` and returns control to
the user while the MCP server may still be finishing work in the background. This differs from the
local host-CLI path, where the owned `tokio::process::Child` is explicitly killed and awaited before
cancellation completes, so the subprocess is no longer running.

## 8. `cli_delegate` (Claude Code / Codex as backends)

### 8.1 Rejected: Agent-SDK / ACP path

Agent Client Protocol (ACP, Zed-originated, `agent-client-protocol` crate on crates.io) looked like
the clean answer: it standardizes `session/request_permission` as an in-band, live round-trip, and
adapters exist for Claude Code (`@zed-industries/claude-code-acp`, built on
`@anthropic-ai/claude-agent-sdk`) and Codex (`codex-acp`, wrapping `codex app-server`). **Rejected
for the Claude Code side**: verified verbatim at
[code.claude.com/docs/en/agent-sdk/quickstart](https://code.claude.com/docs/en/agent-sdk/quickstart):

> Unless previously approved, Anthropic does not allow third party developers to offer claude.ai
> login or rate limits for their products, including agents built on the Claude Agent SDK. Please
> use the API key authentication methods described in this document instead.

haex-vault is exactly the "third party developer's product" this names, and haex-vault has no such approval.
`claude-code-acp` is Agent-SDK-based, so it is out. Whether this clause also covers _this project's_
specific way of driving the raw `claude` CLI directly (not the SDK) is genuinely unclear from the
docs and was not resolved here — **operator decision (2026-09-11): proceed anyway, accepting this as
an open compliance risk**, using the raw-CLI path below rather than the SDK/ACP path specifically
named in the clause. No equivalent restriction was found for OpenAI/Codex, but it was not
specifically checked either — flagged as unverified, not as "confirmed fine."

### 8.2 Chosen approach, per provider

**Codex**: speak `codex app-server --stdio` (Codex's own documented JSON-RPC protocol) directly,
without `codex-acp`. Unlike `codex exec` (closed stdin, any approval auto-rejects per
[openai/codex#24135](https://github.com/openai/codex/issues/24135)), `app-server` is a persistent
bidirectional session — holding it open as the host should allow live per-call approval through our
own permission gate (§5) instead of the fail-closed/full-bypass (`--yolo`) dilemma found in headless
mode. **Not yet verified against a real installation** — treat as the leading hypothesis, not a
confirmed fact.

**Claude Code**: raw `claude -p` CLI, no SDK. ~~This has no live mid-run round-trip — a `-p`
invocation is one task in, one answer out. Tool access is therefore pre-approved _per invocation_,
not per-call... Manual mode, in particular, cannot offer live per-call confirmation here the way it
does for built-in/MCP tools.~~ **Superseded 2026-09-16, see §8.2a** — this was wrong. `-p` mode does
support a live per-tool-call approval round-trip, verified against a real installation.

### 8.2a Verified update (2026-09-16, from spec 007-cli-delegate's clarification pass)

§8.2's original claim above — that Claude Code's `-p` mode has no live mid-run round-trip and can
only get a pre-call tool whitelist — was tested directly against an installed `claude` CLI (v2.1.241)
and found **wrong**. `claude -p` supports `--permission-prompt-tool <mcp-tool-name>`: an MCP tool
haex-vault provides and registers via `--mcp-config`, which Claude Code calls live, and blocks on, for
each individual pending tool-use approval — the CLI-only equivalent of the Agent SDK's `canUseTool`
callback. Verified end-to-end with a purpose-built stdio MCP server: a `-p` run with
`--permission-mode default` requesting a `Bash` call genuinely paused mid-run, the MCP server
received `{tool_name: "Bash", input: {command, description}, tool_use_id}` over `tools/call`, and the
command only executed after the server responded `{"behavior":"allow","updatedInput":{...}}` (a
4-second artificial delay in the test server's response was faithfully reflected in the run's total
wall time). Conclusion: Claude Code needs no batch/pre-approval special-casing — it can use the same
live per-call approval gate as Codex and the local/api_key tool loop, bridged through this MCP tool.

Also verified in that same pass: an isolated, empty `CLAUDE_CONFIG_DIR` redirects both credential
lookup (a `-p` run against it reported "not logged in" despite the host having a valid login) and
host-level hook execution — a first, non-isolated test run picked up the operator's own
`~/.claude/settings.json` (a `SessionStart` hook fired, and its `permissions.defaultMode: "auto"`
silently changed the run's behavior); the isolated re-run showed neither. This supports §8.3/§8.4's
mechanism (`CLAUDE_CONFIG_DIR` + disposable `cwd`) directly. Separately, current docs
(`code.claude.com/docs/en/headless`, fetched 2026-09-16) recommend `--bare` for exactly this kind of
scripted host-isolation, but `--bare` explicitly does not support subscription/OAuth login (API key
only) — so this feature uses the `CLAUDE_CONFIG_DIR`/disposable-`cwd` mechanism instead of `--bare`,
not because `--bare` is unverified but because it's the wrong tool for a subscription-based
credential. Still unverified: whether this mechanism blocks every kind of discovery `--bare`
guarantees (skills, plugins, MCP auto-discovery), or only what was tested (hooks, credentials,
settings). See `specs/007-cli-delegate/spec.md` Assumptions for the fuller writeup.

Codex's approval protocol was also checked, structurally, via the installed CLI itself: `codex
app-server generate-json-schema --out <dir> --experimental` dumps the app-server's actual JSON-RPC
schema, which confirms `ExecCommandApprovalRequest`, `ApplyPatchApprovalRequest`, and a
`PermissionsRequestApprovalRequest`-shaped request are all members of the protocol's `ServerRequest`
union (i.e. genuine server-initiated requests awaiting a client response), each paired with a
response type carrying a `ReviewDecision` (`approved` / `approved_for_session` / `denied` / `abort` /
`timed_out`). The `timed_out` variant in particular implies a real wait-then-timeout, not a
fire-and-forget notification. This structurally confirms design doc §10 item 1's leading hypothesis
without yet exercising a full live round-trip the way the Claude Code test above did — a cheap,
worthwhile pre-implementation spike, not a re-opened unknown.

Also newly relevant to §8.1's ToS question: current docs open the same headless-mode page with
"This page covers using the Agent SDK via the CLI (`claude -p`)" — i.e. official docs now describe
`-p` itself as an Agent SDK surface. §8.1's "genuinely unclear whether this also covers driving the
raw CLI directly" is less unclear now; the operator's 2026-09-11 accept-the-risk decision stands, but
on better-informed footing than when it was made.

### 8.3 Vault-portable credentials, zero host residue

Hard requirement (operator, 2026-09-11): the vault (`*.db`) must be usable on a foreign machine that
has never run `claude login` / `codex login`, without relying on — or leaving behind — any host-level
login state. This **reverses** `plans/001-desktop-mvp.md`'s line 163 ("das aufgerufene Programm
authentifiziert selbst" / host-native login); that line needs updating alongside implementation.

- One-time setup: `claude setup-token` (mints a long-lived OAuth token against the Pro/Max/Team
  subscription, printed once, never stored by the CLI) or `codex login` (produces `auth.json`).
  haex-vault captures the result and stores it encrypted in the vault.
- Per invocation: a fresh temp directory. Claude gets `CLAUDE_CODE_OAUTH_TOKEN` (env var) and
  `CLAUDE_CONFIG_DIR=<tmp>`; Codex gets `CODEX_HOME=<tmp>` pre-populated with `auth.json`. The
  process's `cwd` is also the temp directory — a live Claude Code bug
  ([anthropics/claude-code#3833](https://github.com/anthropics/claude-code/issues/3833)) writes
  `.claude/settings.local.json` into the _cwd_ regardless of `CLAUDE_CONFIG_DIR`, so cwd must be
  disposable too. Delete the temp directory after the process exits.
- Do not pass `--bare` to Claude Code — it ignores `CLAUDE_CODE_OAUTH_TOKEN`.

### 8.4 Host isolation (haex-vault as the only source of truth)

Operator requirement (2026-09-11): Claude/Codex must see _only_ what haex-vault gives them — no host
`CLAUDE.md`/`AGENTS.md`, no host user-global settings, no host project configuration. The mechanisms
in §8.3 mostly already provide this as a side effect:

- `CLAUDE_CONFIG_DIR`/`CODEX_HOME` pointed at an empty temp dir blocks global config discovery, not
  just credential persistence.
- An empty temp `cwd` blocks upward `CLAUDE.md`/`AGENTS.md` project discovery — there is nothing to
  find.
- Additional, explicit belt-and-suspenders: pass `--setting-sources` (or the SDK-equivalent
  `settingSources`) as empty, so Claude Code does not even consult user/project/local setting
  sources, rather than relying solely on "the directory happens to be empty." Exact flag name/CLI
  support **not yet verified**.
- Any project/system context haex-vault wants the delegate to have is passed explicitly (e.g.
  `--append-system-prompt` for Claude; Codex's equivalent) — never left to file discovery. The
  delegate is treated like any other model backend: a model with native tools, not an agent with its
  own project memory.

## 9. Explicitly rejected alternatives

- **Cordis / deepseek-harness's plugin architecture wholesale.** Node/TS DI framework for a
  general-purpose, multi-tenant harness; wrong language and far more machinery than a single-user
  Rust/Tauri personal agent needs.
- **LiteLLM** (or any completion-API proxy). Solves cross-provider chat-completion normalization,
  which is already cheap here (`ProviderAdapter` trait, small remaining adapters per
  `plans/001-desktop-mvp.md:262`). Does not provide a tool loop, registry, or permission gate — the
  actual work — and would add a Python sidecar, which was already rejected once for local inference
  (`mistral.rs` in-process instead of a `llama.cpp` sidecar, specifically because "iOS erlaubt keine
  fremden Subprozesse").
- **Generic Rust agent frameworks** (e.g. `rig`, `swiftide`). Same shape of problem as LiteLLM: they
  replace the already-cheap adapter layer, not the bespoke parts (CRDT-backed storage, the
  Claude-Code-style permission-mode switcher, cli_delegate wiring).
- **`pi` (earendil-works/pi)**: own agent-loop implementation, no permission/approval system at all
  ("a permission-gate or container should be the first thing you add" per its own docs), TS-only.
  Not reusable here.
- **Subagent-style full delegation** (deepseek-harness's actual model for Claude Code/Codex: full
  native tool autonomy, fail-closed, no approval crossing back to the host). Rejected as the default
  tool-call path because it conflicts with "the user always decides" (§5); could resurface later as
  an explicitly separate, clearly-labeled "delegate this whole task autonomously" feature, distinct
  from normal tool-calling.

## 10. Open questions / verification spikes needed before implementation

None of the following should be trusted from this document alone — each needs a check against a
real installed CLI version before code is written against it:

1. Does `codex app-server --stdio`, held open by a persistent parent process, actually deliver live,
   answerable `approval` requests (as opposed to inheriting `codex exec`'s closed-stdin auto-reject
   behavior)? **Structurally confirmed, behaviorally still open** (§8.2a): the protocol schema
   generated from the installed CLI proves genuine server-initiated approval requests with a
   timeout-capable response type exist; a live end-to-end round-trip (the way the Claude Code side
   was tested) has not yet been exercised.
2. Exact current flags: ~~Claude Code's `--setting-sources`/equivalent~~, `--append-system-prompt`,
   `--mcp-config`/`--allowedTools` interaction in `-p` mode; Codex's system-prompt-equivalent flag.
   **Partially resolved 2026-09-16** (§8.2a): current docs confirm `--append-system-prompt` and
   `--mcp-config`/`--allowedTools`/`--permission-prompt-tool` all exist and were exercised live; no
   `--setting-sources` flag exists for the raw CLI — `--bare` is the closest documented mechanism, but
   it drops subscription/OAuth login, an unresolved tension (§8.2a). Codex's equivalent still
   unchecked.
3. Whether `CLAUDE_CONFIG_DIR` and `CODEX_HOME` redirect _everything_ (telemetry, cache, logs) or
   only the documented subset — both official doc sources were incomplete on this. **Partially
   resolved 2026-09-16** (§8.2a): `CLAUDE_CONFIG_DIR` confirmed to redirect credential lookup; whether
   it also redirects `~/.claude`-level hooks/settings discovery specifically is still unconfirmed.
   `CODEX_HOME` untested.
4. Whether driving the raw `claude` CLI (not the Agent SDK) for a third-party product's end users
   falls under the quoted Agent-SDK ToS clause (§8.1) — a question for Anthropic, not for docs
   archaeology. **Better-informed, still open** (§8.2a): current docs now describe `-p` itself as
   "the Agent SDK via the CLI," making this more likely to apply than the 2026-09-11 framing assumed.
   Equivalent check not yet done for OpenAI/Codex at all.
5. `chat.permission_mode` UX details (how "Auto" surfaces a paused risky action, how a remembered
   allow/deny would work) are unspecified — out of scope for this document, first pass is
   ask-every-time-for-Risky in `Manual`/`Auto`.
