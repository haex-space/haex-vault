# Phase 0 Research: CLI Delegate Backend (Claude Code / Codex)

Most of this feature's technical unknowns were resolved by live verification against installed CLIs
(`claude` v2.1.241, `codex-cli` 0.147.0) during this planning session, not from documentation alone —
documentation turned out to be wrong on the single most load-bearing point (§1 below).

## 1. Claude Code (`claude -p`) — live per-tool-call approval

**Decision**: Drive `claude -p` directly (no Agent SDK, no `claude-code-acp`), passing
`--permission-prompt-tool mcp__<server>__<tool>` pointed at a small stdio MCP server haex-vault spawns
per invocation and registers via `--mcp-config`. That server's one tool bridges each pending approval
into haex-vault's existing `ChatState.pending_tool_approvals` / `tool-permission-request` mechanism
(`turn.rs`, unchanged) instead of a Claude-Code-specific approval path.

**Rationale**: The design doc's original assumption (`docs/plans/2026-09-11-agent-tool-loop-design.md`
§8.2, written 2026-09-11) was that `-p` "has no live mid-run round-trip" and can only get a pre-call
tool whitelist — which drove this feature's spec toward a bespoke "upfront batch approval" design for
Claude Code specifically. That assumption was **empirically wrong**, discovered by testing directly:

- A minimal hand-rolled stdio MCP server (newline-delimited JSON-RPC: `initialize`, `tools/list`
  exposing one tool named `approve`, `tools/call` handling it) was registered via a `--mcp-config`
  JSON file.
- Running `claude -p "Run exactly this shell command using the Bash tool: touch
verify_delegate_test3.txt" --mcp-config mcp-config.json --permission-prompt-tool
mcp__approvalserver__approve --permission-mode default` produced a `tools/call` request to the test
  server: `{"name":"approve","arguments":{"tool_name":"Bash","input":{"command":"touch
verify_delegate_test3.txt","description":"..."},"tool_use_id":"toolu_..."}}`.
- The test server's handler slept 4 seconds before responding
  `{"behavior":"allow","updatedInput":{...}}` (wrapped in an MCP text content block). The overall
  `claude -p` run's wall time reflected that full 4-second wait, and `verify_delegate_test3.txt` was
  only created after the response was sent — a genuine blocking, live, per-tool-call round-trip, not
  a fire-and-forget notification or a static pre-declared policy.
- A first attempt at this same test (without `--permission-mode default` explicitly set) showed the
  Bash call sail through with **no** call to the test server at all. Root cause: the operator's own
  `~/.claude/settings.json` sets `"permissions": {"defaultMode": "auto"}` globally, so any `-p`
  invocation without an explicit `--permission-mode` silently inherited that personal setting's
  model-classifier auto-approval instead of running in Manual/`default` mode — a concrete, observed
  instance of exactly the host-config leakage User Story 4 exists to prevent, not a flaw in the
  approval mechanism itself.

**Response schema used** (not fully documented anywhere found — reverse-engineered from the Agent
SDK's `canUseTool` return-type documentation, since `--permission-prompt-tool`'s own wire format
isn't separately documented): a `tools/call` result whose content is a text block containing
`{"behavior":"allow","updatedInput":<original or modified input>}` or
`{"behavior":"deny","message":"<reason>"}`. This worked end-to-end in the live test above. Treat the
exact field names as verified-working, not as officially guaranteed to be stable across Claude Code
versions — worth a smoke test in CI against whatever `claude` version ships.

**Alternatives considered**: The design doc's originally-rejected ACP/Agent-SDK path (§8.1, still
rejected — no new information changes that). The upfront-batch-approval design this feature's spec
itself proposed before this finding — rejected now that live approval is proven to work, since it
would have meant Claude Code's tool-use experience diverges from every other backend's live,
per-action approval for no remaining technical reason.

## 2. Codex (`codex app-server --stdio`) — approval protocol shape

**Decision**: Speak `codex app-server --stdio`'s JSON-RPC protocol directly (no `codex-acp`), holding
one process open per delegate invocation. Route its server-initiated approval requests
(`ExecCommandApprovalRequest`, `ApplyPatchApprovalRequest`, a `PermissionsRequestApprovalRequest`-
shaped request) through the same `pending_tool_approvals` bridge as the Claude Code path.

**Rationale**: `codex app-server generate-json-schema --out <dir> --experimental` (a built-in
subcommand of the installed CLI) dumps the actual protocol schema without needing external docs or a
live conversation. It confirms:

- `ServerRequest` (the union of requests the app-server sends _to_ the client) includes
  `ExecCommandApprovalRequest`, `ApplyPatchApprovalRequest`, `Item/commandExecution/
requestApprovalRequest`, `Item/fileChange/requestApprovalRequest`, and `Item/permissions/
requestApprovalRequest` — i.e., approval requests are genuine server→client JSON-RPC requests
  (carrying `itemId`, `threadId`, `turnId`, `startedAtMs`), not notifications.
- The paired response type (`ExecCommandApprovalResponse`) carries a `ReviewDecision` enum:
  `approved`, `approved_for_session`, an execpolicy-amendment variant, a network-policy-amendment
  variant, `denied` (with a `rejection` reason, "agent will continue the turn"), `abort` ("turn will
  also be immediately interrupted"), and — notably — `timed_out` ("automatic approval review timed
  out before reaching a decision"). The existence of a timeout outcome is strong structural evidence
  of a real wait, not a static or fire-and-forget check.

This structurally confirms design doc §10 item 1's leading hypothesis, and was then verified with a
**real live round-trip** (tasks.md T008, 2026-09-16) against this machine's already-authenticated
`codex` (0.147.0), using a hand-rolled Python JSON-RPC driver (`initialize` → `thread/start` →
`turn/start` with a prompt asking it to run `touch <file>`):

- The actual wire method is **`item/commandExecution/requestApproval`** (matching the `Item/
commandExecution/requestApprovalRequest` `ServerRequest` variant found by schema alone) — **not**
  the differently-named `ExecCommandApprovalRequest`/`ExecCommandApprovalResponse` pair the schema
  file of that name suggested. Both exist in the schema bundle; this installed version uses the
  `item/commandExecution/…` one on the wire.
- The **response shape is also different from what `ExecCommandApprovalResponse` suggested**: the
  server expects `{"decision": <value>}` where `<value>` is one of `accept`, `acceptForSession`,
  `acceptWithExecpolicyAmendment`, `applyNetworkPolicyAmendment`, `decline`, `cancel` (matching
  `CommandExecutionApprovalDecision`, found alongside the request params in the same schema file,
  not `ReviewDecision`'s `approved`/`denied`/`abort`/`timed_out` naming). Responding with
  `{"decision":"approved"}` (the `ReviewDecision`-shaped guess) triggered a server-side
  deserialization error, logged to stderr, and the server treated the malformed response as a
  rejection ("approval request failed") — a real, observed fail-safe default worth relying on
  deliberately, not just noting.
- **Genuinely blocking, confirmed via timing**: the handler was made to `sleep(3)` before responding.
  The thread's status flipped to `{"type":"active","activeFlags":["waitingOnApproval"]}` immediately
  after the request arrived and stayed there for the full 3 seconds; the command only executed (file
  created, non-null `processId`) after the correctly-shaped `{"decision":"accept"}` response was
  sent. This rules out a fire-and-forget notification or a pre-computed/cached decision.
- `approvalPolicy: "on-request"` and `approvalsReviewer: "user"` in `thread/start`'s params were both
  needed to route the request back to the client at all (per `ApprovalsReviewer`'s doc comment,
  `"user"` is already the default, but setting it explicitly removes any doubt from a possibly
  customized `~/.codex/config.toml` — the same host-config-leakage risk already found on the Claude
  Code side, research.md §3).

**Decision, updated**: `codex.rs` (tasks.md T014/T036) must speak the `item/commandExecution/
requestApproval` / `item/fileChange/requestApproval` / `item/permissions/requestApproval` wire
methods with `CommandExecutionApprovalDecision`-shaped responses (`accept`/`decline` at minimum),
**not** the `ExecCommandApprovalRequest`/`ReviewDecision` pair originally assumed from schema alone.

**Alternatives considered**: `codex exec` (headless mode) — rejected per the design doc's existing
finding ([openai/codex#24135](https://github.com/openai/codex/issues/24135)): closed stdin means any
approval auto-rejects, incompatible with live approval entirely. `codex-acp` — rejected as unnecessary
indirection now that the raw protocol's shape is directly confirmed from the installed CLI itself.

## 3. Host isolation and credential mechanism (Claude Code)

**Decision**: Use `CLAUDE_CONFIG_DIR=<fresh empty temp dir>` + a disposable `cwd` + `CLAUDE_CODE_
OAUTH_TOKEN` (env var, from `claude setup-token`) — **not** `--bare`.

**Rationale**: Current docs (`code.claude.com/docs/en/headless`, fetched 2026-09-16) recommend
`--bare` as "the recommended mode for scripted and SDK calls" for exactly this kind of host
isolation, but state explicitly: "bare mode doesn't use your subscription login... In bare mode,
Claude Code never reads OAuth credentials or the system keychain. For the Anthropic API, set
`ANTHROPIC_API_KEY`." Since this feature's entire premise (vs. the existing `api_key` provider) is
reusing an existing _subscription_, `--bare` is the wrong tool here regardless of its isolation
strength. The non-`--bare` mechanism was verified directly instead:

- Running `claude -p ... ` with `CLAUDE_CONFIG_DIR` pointed at a fresh empty directory produced
  `"apiKeySource":"none"` and `"Not logged in · Please run /login"` in the response, despite the host
  machine having a valid, working `claude` login — confirming credential redirection.
- The same isolated run's event stream showed no `hook_started`/`hook_response` system events, while
  an otherwise-identical non-isolated run showed two (the operator's own `~/.claude/settings.json`
  `SessionStart` hook firing before `system/init`). This confirms `CLAUDE_CONFIG_DIR` also blocks
  host-level _hook_ execution, not just credential lookup.

**Skills/plugins isolation, verified** (tasks.md T009, 2026-09-16): compared `system/init`'s `skills`/
`plugins` fields between a non-isolated and an isolated run of the same `claude -p "say hi"` prompt.
Non-isolated: 47 skills including the operator's personal ones (`brainstorming`, `diagnose`,
`graphify`, etc.) and one real plugin loaded (`superpowers`, `/home/haex/.claude/skills/superpowers`,
v4.1.1). Isolated (`CLAUDE_CONFIG_DIR` pointed at a fresh empty directory): exactly 15 skills — all of
them the CLI's own bundled/built-in set (`deep-research`, `dataviz`, `doctor`, etc.), none of the
operator's personal ones — and zero plugins. This confirms the isolation mechanism blocks user-level
skill and plugin discovery too, not only credentials/hooks/settings — the last previously-unverified
category from `--bare`'s guarantee list.

**Alternatives considered**: `--bare` + `ANTHROPIC_API_KEY` — rejected, collapses this feature into
the existing `api_key` provider and abandons the "bring your subscription" value proposition (spec.md
User Story 1). Relying on `~/.codex`/`~/.claude` host state directly (`plans/001-desktop-mvp.md`'s
original, since-revised assumption) — already rejected in the 2026-09-11 design doc for breaking
vault portability; nothing here reopens that.

## 4. `ProviderAdapter` integration point — no `turn.rs` changes needed

**Decision**: `CliDelegateAdapter` (both Claude and Codex variants) implements the existing
`ProviderAdapter` trait exactly like `AnthropicAdapter`/`LocalAdapter`. It runs its subprocess to
completion internally (including its own tool use, bridged through the approval mechanism above) and
streams the final answer back as ordinary `StreamChunk::Delta` fragments followed by `StreamChunk::
Done` — never emitting `StreamChunk::ToolCalls`.

**Rationale**: `adapters/types.rs`'s existing `ChatRequest.tools` documentation already establishes
that an adapter "never errors on an empty list, it just never emits `StreamChunk::ToolCalls`" for
models that don't support tool calling (`LocalAdapter`'s existing behavior for non-tool-calling local
models). `turn.rs`'s `run_turn`/`run_step` only route through haex-vault's own `ToolRegistry`/permission-
per-round machinery when a step's result actually carries `tool_calls` — the same escape hatch that
already exists for plain-text local models applies unchanged to a delegate backend that does all of
its own tool use internally. Confirmed by reading `turn.rs` end-to-end (no `match ProviderKind` or
delegate-specific special-casing exists there today, nor is any needed). This is why this feature's
plan.md Summary can truthfully say `turn.rs` needs zero changes.

**Alternatives considered**: Exposing the delegate's internal tool_use/tool_result activity as
haex-vault's own `StreamChunk::ToolCalls` round-trips (i.e., making `turn.rs` drive the delegate's tools
one-by-one like it drives the built-in registry). Rejected — the whole point of a CLI delegate is
that it runs its own tool loop with its own tools (Claude Code's Bash/Read/Edit/etc., Codex's
equivalents); re-routing those through haex-vault's `ToolRegistry` would mean re-implementing each
delegate's own built-in tools a second time for no benefit. Tool activity the delegate performs is
instead surfaced as a labeled record (FR-005) via `tool_source` (data-model.md), not replayed through
the built-in tool-call machinery.

## 5. Connect flow acquisition mechanism (`connect_cli_delegate`, US2)

**Decision**: `claude setup-token` requires a real PTY and a two-step interactive exchange; `codex
login --device-auth` does not. The two vendors therefore use different acquisition shapes:

- **Claude**: spawn `claude setup-token` inside a PTY (`portable-pty`, per operator direction — the
  in-app guided flow was chosen over having the user run the command in their own terminal and paste
  the resulting token). Extract the OAuth URL from the PTY output (see below), emit it via
  `delegate-connect-progress` (`status: 'awaiting_browser'`), then accept the authorization code the
  user copies back from the browser via a new `submit_cli_delegate_code` command that writes it (plus
  a line ending) to the PTY's stdin. The final long-lived token is scraped from the PTY's subsequent
  output.
- **Codex**: spawn `codex login --device-auth` over plain piped stdio (no PTY needed — confirmed
  below), extract the URL and one-time code from stdout, emit both via `delegate-connect-progress`,
  then just wait for the process to exit 0 and read the resulting `<isolated CODEX_HOME>/auth.json`.
  No second command needed on the Codex side.

**Rationale, verified 2026-09-16** by running each command under a real PTY (`portable-pty`-equivalent
`pty.fork()` probe) with a short (5-8s) hard timeout and `SIGKILL`, stopping well before any browser
step could be completed — no real credential file was written by any probe (confirmed via `stat` on
the real `~/.claude/.credentials.json` / `~/.codex/auth.json` mtimes before and after):

- **`claude setup-token` is a raw-ANSI Ink/React TUI, not a line-oriented CLI.** Piped, non-PTY stdio
  (even with the `--ax-screen-reader` flat-output accessibility flag) produced **zero bytes** of output
  over an 8-second window — the app never renders without a real terminal. Under a genuine PTY it
  renders a full-screen app: cursor-positioned welcome banner, then "Opening browser to sign in…" (an
  `xdg-open`-style call — this is what caused real browser popups during verification and must be
  expected/documented as a side effect of ever actually running this command for real), then, since no
  browser handler is available in the isolated verification environment, "Browser didn't open? Use the
  url below to sign in (c to copy)" followed by the OAuth URL rendered as an **OSC 8 terminal hyperlink**
  (`\x1b]8;id=...;<url>\x07<visible text>\x1b]8;;\x07`) — the same URL is re-emitted whole inside the
  OSC 8 payload on every redraw even though the _visible_ text is word-wrapped/truncated differently
  each time, so the reliable extraction strategy is to scan raw bytes for OSC 8 sequences and take the
  URI between the second `;` and the terminating BEL/ST, not to regex the rendered/wrapped text. The
  final screen before the probe was killed showed "Paste code here if prompted >" — confirming the
  authorization code from the browser's callback page must be typed/pasted back into the running
  process, not auto-detected via a local callback listener.
- **`codex login` (default, no flag) starts a local HTTP server on `localhost:1455`** and prints a
  plain (non-PTY, line-oriented — confirmed working over a piped, non-tty stdio) URL to open, with an
  explicit hint: "On a remote or headless machine? Use `codex login --device-auth` instead." This
  local-port dependency is avoided in favor of the alternative below, consistent with this feature's
  existing preference (approval-bridge IPC, revision notes above) for mechanisms that don't require a
  local network listener.
- **`codex login --device-auth` is plain, pipe-friendly text output** (ANSI color codes only, no
  cursor-positioning): prints a URL (`https://auth.openai.com/codex/device`) and a short one-time code
  (e.g. `GL00-DVEC2`, "expires in 15 minutes") for the user to enter on that page manually. No local
  port, no manual paste-back into the CLI — after the user enters the code on the website, the process
  itself polls until authorized and then writes `<CODEX_HOME>/auth.json` and exits 0. This is the
  acquisition mechanism used, over the default flow.
- Neither probe was pushed to actual completion (that requires a human to open a real browser tab and
  authorize, which cannot be scripted, and doing so would mint a real, unwanted long-lived credential
  against the operator's live subscription) — so the **exact plain-text format of `claude setup-token`'s
  final "here is your token" screen is not yet directly verified**, only inferred from the tool's
  documented purpose (`setup-token` exists specifically to produce a portable value for the
  `CLAUDE_CODE_OAUTH_TOKEN` env var, per §3 above — i.e. it must print a plain copyable string, not only
  save to a credential file). The implementation therefore extracts the token from ANSI-stripped output
  via a tolerant pattern match (Anthropic's documented OAuth-token prefix, `sk-ant-oat`) rather than a
  hard-coded screen-layout assumption, and this should be treated as the one open item for whoever
  performs the manual quickstart.md walkthrough (tasks.md T046) to confirm against a real completed
  flow.

**Addendum, confirmed live 2026-09-16/17**: a real user completed the flow end-to-end. The final
"here is your token" screen assumption above (`sk-ant-oat` prefix match) held up, but a different part
of the design turned out to be wrong: Anthropic's current OAuth consent page, after "Authorize", shows
a plain "you're all set, close this window" message and **never displays a code to copy back** —
contradicting the "Paste code here if prompted" screen text seen during the truncated 2026-09-16 probe
above. `submit_cli_delegate_code` alone therefore deadlocked permanently for a real user: nothing ever
read the PTY session's event channel again after `connect_cli_delegate` returned `AwaitingCode`, so the
token — which the underlying process still produces on its own once Anthropic's side of the OAuth
round-trip resolves — sat unread forever. Fixed by a background task
(`providers::connect::watch_claude_auto_completion`) that keeps watching the same session for that
token and completes the flow itself; `submit_cli_delegate_code` is kept as a fallback in case some other
`claude` CLI build still shows a code.

Same live session also surfaced a second, independent bug in the same flow: the OAuth URL rendered in
`ConnectDelegateProvider.vue` was not clickable at all. HaexVault's webview had no integration to request
opening an external URL in the system browser — no `tauri-plugin-opener` dependency and no `opener:*`
capability in `capabilities/default.json`. Fixed by adding
`tauri-plugin-opener` (Rust crate + `@tauri-apps/plugin-opener` npm package, `opener:allow-open-url`
capability) and routing both connect-flow link clicks through its `openUrl()` — this is unrelated to the
`CLAUDE_CONFIG_DIR`/`CODEX_HOME` host-isolation mechanism (§3/§6 below), which was already correct and
untouched by either fix.

**Alternatives considered**: Have the user run `claude setup-token` in their own terminal and paste the
resulting token into a plain text field (mirrors the existing `api_key` provider flow exactly, avoids
the `portable-pty` dependency and all ANSI/OSC-8 parsing entirely) — this was the recommended option but
the operator explicitly chose the in-app PTY-driven flow instead for a more consistent "connect inside
haex-vault" UX across both vendors. `codex login` default flow (local `localhost:1455` server) — rejected in
favor of `--device-auth` to avoid a local-port dependency, matching this feature's existing IPC
preferences.

## 6. Isolation, verified with a negative control (not just a valid-credential pass)

**Decision**: `CLAUDE_CONFIG_DIR`/`CODEX_HOME` isolation is genuinely enforced, not a false positive
that happens to work because the invocation silently falls back to this machine's real,
already-authenticated `~/.claude`/`~/.codex` session.

**Rationale**: every isolation-adjacent test elsewhere in this feature (`cli_delegate_claude.rs`'s
stub, `cli_delegate_codex_live.rs`, `cli_delegate_connect.rs`) copies a **valid** credential into the
isolated temp dir — a pass there cannot distinguish "used my isolated copy" from "ignored the isolated
dir and used the host's real login," since both would produce the same successful answer. The decisive
test is a negative control: put a **deliberately wrong** credential in the isolated dir and confirm the
invocation _fails_, on this exact machine, where the real `~/.claude`/`~/.codex` are both valid and
already logged in (this very research session runs under that real Claude Code login). Verified live
2026-09-16, both vendors, via `tests/cli_delegate_isolation_live.rs` (`#[ignore]`d, run with
`--ignored`):

- **Codex**: a garbage `auth.json` (`{"tokens":{"access_token":"garbage-not-a-real-token"}}`) written
  into an isolated `CODEX_HOME` produces a genuine `401 Unauthorized: Missing bearer or basic
authentication in header` from `api.openai.com`/`wss://api.openai.com`, retried 5 times over
  WebSocket then 5 more over HTTPS fallback (~16s total), before the turn ends in failure. The same
  isolated-dir mechanism with the _real_ `auth.json` copied in instead answers correctly ("pong") —
  a clean contrasting pair, not just one passing test.
- **Claude**: a garbage `CLAUDE_CODE_OAUTH_TOKEN` (`sk-ant-oat01-this-is-definitely-not-a-real-token`)
  with an isolated `CLAUDE_CONFIG_DIR` produces `"apiKeySource":"none"`, two `api_retry` events with
  `error_status:401`/`authentication_failed`, and a final result `"Failed to authenticate. API Error:
401 OAuth access token is invalid."` — never the host session's real answer.

**A genuine bug was found and fixed via this check, not just a confirmation**: Codex's real wire
protocol has **no separate `turn/failed` notification for this case** — the auth failure above arrived
as `turn/completed` with `params.turn.status == "failed"` and a populated `params.turn.error`, not the
schema-suggested distinct `turn/failed` method. `codex.rs`'s `turn/completed` handler unconditionally
sent `StreamChunk::Done` without checking `turn.status`, so this failure would have been silently
reported to the user as a normal, empty, successful response instead of an error. Fixed by extracting
`turn_completed_failure(params)` (`codex.rs`, unit-tested in `codex_tests.rs` against the exact live
JSON shape above) and checking it before treating `turn/completed` as success. The separate
`"turn/failed"` match arm is kept as a defensive fallback (schema-derived, never observed on the wire
in this feature's testing) rather than removed.

**Alternatives considered**: Trusting the existing valid-credential tests as sufficient isolation
proof — rejected once it became clear they structurally can't distinguish real isolation from a silent
host-state leak (raised directly by the operator: "the most interesting part is whether claude/codex
can be started directly from haex-vault without possibly using the locally existing claude/codex").

## Summary of resolved Technical Context unknowns

| Unknown                                            | Resolution                                                                                                                                                |
| -------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Claude Code live approval capability               | §1 — `--permission-prompt-tool` + haex-run MCP server, verified live                                                                                      |
| Codex approval protocol shape                      | §2 — verified live: `item/commandExecution/requestApproval` + `CommandExecutionApprovalDecision`-shaped `{"decision":"accept"\|"decline"\|...}` responses |
| Host isolation + subscription credential mechanism | §3 — `CLAUDE_CONFIG_DIR` + disposable `cwd`, not `--bare`; verified for credentials, hooks, _and_ skills/plugins                                          |
| Turn/step loop integration point                   | §4 — `ProviderAdapter::stream_chat`, no `turn.rs` changes                                                                                                 |
| Connect-flow (US2) acquisition mechanism           | §5 — `claude setup-token` needs a PTY + paste-back code (`portable-pty`); `codex login --device-auth` is plain-text, no PTY, no paste-back                |
| Isolation genuineness (negative control)           | §6 — verified with a _wrong_ credential, both vendors; found and fixed a real bug (`turn/completed`'s embedded `status: "failed"` was previously ignored) |

No unresolved `NEEDS CLARIFICATION` markers remain in `plan.md`'s Technical Context, and both of
tasks.md's Phase 2 verification spikes (T008 Codex live round-trip, T009 Claude skills/plugins
isolation) completed 2026-09-16 with live-verified, not just schema/doc-inferred, results. §5's token-
screen format is the one remaining live-verification gap, called out explicitly above rather than
silently assumed.
