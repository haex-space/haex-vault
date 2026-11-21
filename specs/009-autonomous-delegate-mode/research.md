# Phase 0 Research: Autonomous Delegate Mode

Consolidates the technical unknowns from plan.md's Technical Context. All items below were resolved
against the actual installed CLIs/schemas or the actual codebase during this planning session, not
assumed — the discipline established by 007-cli-delegate's own research.md and its `docs/plans/`
predecessors.

## 1. Where does `AutonomyMode` get threaded through the adapter call chain?

**Decision**: Add `autonomy_mode: AutonomyMode` (default `Standard`) as a new field on `ChatRequest`
(`adapters/mod.rs`), not as a new parameter to `CliDelegateAdapter::new` or a new config struct.

**Rationale**: Confirmed via direct inspection (`mod.rs:195-224`) that `CliDelegateAdapter` has
exactly two `ProviderAdapter` methods, `list_models` and `stream_chat(&self, req: ChatRequest)` —
`req` is already the one thing every adapter (local, api_key, and both delegate vendors) receives
per-call. `spawn_claude_invocation`/`spawn_codex_app_server` and their private `build_command`
helpers take positional scalars today (`binary`, `credentials`, `req`, `context`) with no existing
config struct to extend — adding a field to the struct that's already threaded everywhere is smaller
than introducing a new struct or a new parameter on four different function signatures across two
files. `local`/`api_key` adapters simply ignore the new field (this feature is delegate-only per
spec Assumptions).

**Alternatives considered**: A new parameter on `CliDelegateAdapter::new`, stored as an adapter
field. Rejected — the adapter is constructed once per connected provider, not once per request
(confirmed: `mod.rs:162-179`), so autonomy mode (a _per-request_ choice, spec FR-008) does not
belong on the adapter's own construction-time state; it would have to be re-threaded into
`stream_chat` as an override anyway, which is what putting it on `ChatRequest` already gives for
free.

## 2. Vendor mechanism for `ungated` mode

**Decision — Claude Code**: `--permission-mode bypassPermissions` (confirmed a valid enum value via
`claude --help`, v2.1.274: `choices: "acceptEdits", "auto", "bypassPermissions", "manual", "dontAsk", "plan"`),
with `--mcp-config`/`--permission-prompt-tool` omitted entirely for this mode. Host isolation under
this flag was verified live (design doc §4 item 5, 2026-09-17): a fake `$HOME` carrying a
`SessionStart` hook did not fire even though the session authenticated successfully, confirming
`CLAUDE_CONFIG_DIR` isolation is not weakened by the bypass flag.

**Decision — Codex**: Not a CLI flag. `codex.rs::spawn_codex_app_server` drives `codex app-server
--stdio` (confirmed: `codex.rs:26-35`'s `build_command` passes only `app-server`/`--stdio` plus the
isolated `CODEX_HOME`/`cwd`/piped-stdio setup — no approval or sandbox flag anywhere in that
function). The approval posture is set in the `thread/start` JSON-RPC call's params object
(`codex.rs:299-316`, currently `"approvalPolicy": "on-request"`, `"approvalsReviewer": "user"`).
Verified against the installed CLI's own schema (`codex app-server generate-json-schema --out
<dir> --experimental`, codex-cli 0.147.0 — the same version 007-cli-delegate's own research already
verified against): `ThreadStartParams` (`v2` schema set) accepts `"approvalPolicy": "untrusted" |
"on-request" | "never"` (the `AskForApproval` enum — the same three values `codex exec --help`'s
`-a/--ask-for-approval` flag documents, confirming the enum is shared even though the *app-server*
path takes it as a JSON-RPC field rather than a CLI flag) and an independent, currently-unsent
`"sandbox": "read-only" | "workspace-write" | "danger-full-access"` field (`SandboxMode`).
`ungated` sets `"approvalPolicy": "never"` and *explicitly* adds `"sandbox": "workspace-write"` —
made explicit rather than left to Codex's own default, so a future upstream default change cannot
silently widen `ungated` to `danger-full-access`.

**Alternatives considered**: Driving the standalone `codex`/`codex exec` CLI's own `-a`/`-s` flags
directly instead of `app-server --stdio`. Rejected — 007-cli-delegate already committed to
`app-server --stdio` specifically because it is a persistent, bidirectional session capable of live
per-call approval (design doc §8.2), unlike `codex exec`'s closed-stdin auto-reject behavior; nothing
about `ungated` changes that architectural choice, it only changes two fields in the same
already-open session's start params. Also considered: Codex's own `approvalsReviewer: "auto_review"`
(an automatic reviewing subagent) as the mechanism for `gated-permissive` instead of haex-vault's own deny
list. Rejected per the spec clarification: the operator explicitly wants haex-vault's own, small,
inspectable deny list as an _additional_ filter over the delegate's own judgment, not a second
AI-driven reviewer whose logic haex-vault does not control or see.

## 3. Per-vendor Deny Rule evaluability — the FileChange path gap

**Finding**: `CommandExecutionRequestApprovalParams` (Codex schema, verified via
`generate-json-schema`) carries `command`, `cwd`, `commandActions` (parsed action type), and
`networkApprovalContext` (`host`/`protocol`) — enough to evaluate all three deny categories below.
`FileChangeRequestApprovalParams` (same schema generation) carries only `itemId`, `reason`,
`threadId`, `turnId`, and an unstable `grantRoot` — **no file path at all**. A workspace-boundary or
credential-path deny rule cannot be verified against a Codex file-change approval with today's
schema. Claude Code has no equivalent gap: its `tools/call` `input` for a file-editing tool
(`Write`/`Edit`) carries `file_path` directly (confirmed live during the earlier subagent-visibility
spike, design doc §4 item 3 — the logged `Bash` calls carried `command`/`description`, and Claude's
own documented tool schemas for `Write`/`Edit` carry `file_path` the same way).

**Decision**: When an enabled deny category needs data a given approval payload does not expose
(today: Codex `FileChangeRequestApprovalParams` against the workspace-escape or credential-path
categories), treat the call as denied rather than allow it through unverified — same fail-safe
default as an evaluator failure (spec FR-010), now generalized as spec FR-015. This needs no schema
fix upstream, drops no category for the vendor/action-type combination that _can_ evaluate it
(Claude Code entirely; Codex command execution), and never silently under-enforces a rule the
operator explicitly turned on.

**Alternatives considered**: (a) Silently skip evaluation and allow through — rejected, directly
contradicts the operator's intent in enabling that category and the project's established fail-safe
convention. (b) Drop the two path-dependent categories for Codex entirely (keep them Claude-only) —
rejected as a worse user experience than "deny the unverifiable subset," since it would silently
under-protect Codex file changes specifically with no signal to the operator that the category they
enabled has a gap for that vendor. (c) File an upstream request for Codex to add a path field to
`FileChangeRequestApprovalParams` — worth doing separately, not a blocker for this feature; tracked
as a follow-up, not part of this plan's scope.

## 4. Deny Rule category set

**Decision**: Three fixed categories for v1 of this feature — `workspace_escape`, `network_access`,
`credential_paths` — matching the three concrete examples already named in
`docs/plans/2026-09-17-autonomous-delegate-mode-design.md`'s original Variant B sketch. No
general-purpose pattern/regex language (spec clarification: deny rules are a small, additional
filter, not a rule engine the operator authors from scratch).

**Rationale**: Each category maps to a concrete, evaluable signal for at least one vendor+action-type
combination (see the asymmetry table in plan.md), keeping the matching logic in `autonomy.rs` a
short, fully-tested `match`, not an open-ended parser. Matches the project's established preference
for a small closed enum over a flexible-but-unbounded mechanism (mirrors `PermissionMode`,
`RiskClass`, `DelegateVendor` all being closed enums, not strings).

**Alternatives considered**: Freeform tool-name blocking or command/pattern matching (both floated
during the 2026-09-17 design clarify session). Not chosen for v1 — no concrete driver for anything
finer-grained than the three named categories has surfaced yet; can be revisited if a real gap
appears, consistent with the project's "don't build speculative flexibility" stance (the same
reasoning that already dropped the circuit-breaker for this feature).

## 5. Preference storage pattern for the new Deny Rule setting

**Decision**: `cli_delegate.deny_rules`, a device-scoped preference (`PrefScope::Device`) holding a
JSON array of `DenyCategory` string identifiers, stored/read through the existing generic
`get_pref`/`set_pref`/`clear_pref` Tauri commands (`preferences_commands.rs:59-136`) — no new Tauri
command, no schema migration.

**Rationale**: Confirmed (`storage/preferences.rs`) there is no dedicated command-per-preference
pattern anywhere in the codebase — `chat.permission_mode`, `chat.default_model_id`, and
`chat.last_active_model_id` all go through the same three generic commands, each with its own
locally-declared `const PREF_*` string and its own hand-written parse function; there is no shared
constants module or ts-rs-generated binding for any preference type today (confirmed: `src/types/bindings/`
holds only instance-lifecycle types, none for preferences). Following the exact established pattern
(own local `PREF_DENY_RULES` const in `autonomy.rs`, own `parse`/`serialize` pair, hand-mirrored
TS union type) is more consistent with the codebase than introducing the first
shared-constants/generated-binding pattern as a side effect of this feature.

**Alternatives considered**: A dedicated `get_deny_rules`/`set_deny_rules` Tauri command pair.
Rejected — no precedent anywhere in the codebase for a preference-specific command; would be new
machinery for no behavioral benefit over the existing generic commands the frontend already calls
for every other preference.

## 6. Frontend placement

**Decision**: Two new components. `DelegateAutonomyControl.vue` — a third `ChatComposerControl`
alongside the existing model picker and `PermissionPrompt.vue` in the composer toolbar
(`[instance].vue:1032-1060`), shown only when the active model resolves to a `cli_delegate` provider
(reusing the existing `p.kind === 'cli_delegate'` lookup at `[instance].vue:461`), following
`PermissionPrompt.vue`'s exact `mode`/`update:mode` prop shape. `DelegateDenyRulesSetting.vue` — a
settings-page component modeled on `DefaultModelSetting.vue`'s scope-aware preference-editing
pattern, not added inside `ConnectDelegateProvider.vue` (confirmed that component has no
preference-editing code today — it is purely the OAuth-style connect/reconnect flow).

**Rationale**: Confirmed via direct inspection that the codebase already has both exact patterns
needed — a per-turn composer control (`PermissionPrompt.vue`, ephemeral, not persisted) and a
persistent scope-aware settings component (`DefaultModelSetting.vue`) — so this feature needs no new
UI pattern, only two new instances of existing ones, matching the different persistence semantics
spec FR-008/FR-014 require (autonomy mode ephemeral, deny rules persistent).

**Alternatives considered**: Putting the autonomy-mode selector inside `ConnectDelegateProvider.vue`
(the settings page). Rejected — that component manages one-time vendor connection state, not
per-request chat behavior; a per-request choice belongs in the chat composer where the equivalent
`chat.permission_mode` control already lives, not in a settings page the user would have to leave
the conversation to reach for every request.
