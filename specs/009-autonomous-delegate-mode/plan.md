# Implementation Plan: Autonomous Delegate Mode

**Branch**: `009-autonomous-delegate-mode` | **Date**: 2026-09-17 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `specs/009-autonomous-delegate-mode/spec.md`

## Summary

Adds a per-request **Autonomy Mode** (`standard` / `ungated` / `gated-permissive`) to the existing
`cli_delegate` backends (007-cli-delegate), letting Claude Code or Codex run without haex-vault's live
per-tool-call human approval — either fully ungated (the delegate's own native full-autonomy
mechanism, no bridge) or gated-but-permissive (007's existing approval bridge stays wired and every
call is still recorded, but the response is auto-`allow` unless an operator-configured, persisted
**Deny Rule** matches). Standard (today's shipped, unchanged) stays the default; the new modes never
persist across requests (spec FR-008), Deny Rules do persist device-wide (spec FR-014). No new
backend, no new credential type, no ACP, no circuit-breaker — this is additive to 007's adapters and
approval bridge, not a parallel system.

## Technical Context

**Language/Version**: Rust 1.77.2, edition 2021 (backend, `src-tauri`), TypeScript 5 (frontend, Nuxt
4 SPA) — unchanged from 007-cli-delegate.
**Primary Dependencies**: No new crates. Reuses `tokio::process` (claude.rs/codex.rs subprocess
spawning, already in place) and `rmcp` 3.3.0's existing `server`+`transport-io` use in
`permission_mcp_server.rs` — unchanged for the `gated-permissive` mode since it is the exact same
bridge wiring as 007's shipped default, only the decision function changes (see Data Model). The
`ungated` mode needs no bridge process at all for Claude Code (its `--mcp-config`/
`--permission-prompt-tool` args are simply omitted) and no schema/library change for Codex (the
existing `thread/start` JSON-RPC call already accepts the fields needed — see below).
**Storage**: SQLite via SQLCipher through haex-crdt. Migration `0017_chat_messages_add_autonomy_mode`
adds the nullable `chat_messages.autonomy_mode` column for persisted turn attribution. The existing
generic `preferences` table (`storage/preferences.rs`) needs no schema change; it receives one new
device-scoped preference key,
`cli_delegate.deny_rules` (JSON array of category identifiers, e.g.
`["workspace_escape","network_access","credential_paths"]`), read/written through the existing
generic `get_pref`/`set_pref` Tauri commands exactly like `chat.permission_mode` is today
(`preferences_commands.rs:59-136`). The selected mode travels as a per-request field on
`ChatRequest` and is never persisted as a preference or carried into the next request (spec FR-008);
only the mode actually used is recorded on that request's message rows for audit/history.
**Testing**: `cargo test --lib` / `cargo test --test <name>`; new tests in sibling `*_tests.rs` files
(repo convention, no inline `#[cfg(test)] mod tests`, no exception found anywhere in the codebase).
`pnpm typecheck` for the new composer control. Live-CLI verification spikes (flag/schema behavior)
already run during the 2026-09-17 design session — see `docs/plans/2026-09-17-autonomous-delegate-mode-design.md`
and [research.md](research.md) §1-2 — are not repeated as automated tests (they test vendor CLI
behavior, not haex-vault's own code) but their _results_ are hard-coded as the two vendor branches below.
**Target Platform**: Desktop only — unchanged from 007-cli-delegate (spec FR-012); no new
mobile-adjacent code path.
**Performance Goals**: Stopping an autonomous delegate response terminates the subprocess within the
same short window 007's existing kill path already achieves (spec SC-005) — no new, independent
target; `ungated` mode has strictly _less_ per-call overhead than `standard`/`gated-permissive`
(no bridge round-trip), not a regression risk.
**Constraints**: `ungated` and `gated-permissive` MUST NOT weaken 007's host-isolation guarantee
(`CLAUDE_CONFIG_DIR`/`CODEX_HOME` + disposable `cwd`) — re-verified live under both vendors' native
full-autonomy mechanisms during the design session (design doc §4 item 5), not merely assumed to
still hold when a new CLI flag/JSON field is added. Deny Rule evaluation for `gated-permissive` MUST
reuse the same fail-safe-deny behavior `approval_bridge.rs` already has for a broken pending response
(spec FR-010) — no new failure-handling path, same function extended.
**Scale/Scope**: Single-user, one active model/session at a time (unchanged `ChatState`/
`ActiveSession` invariant). Two new enum-like concepts (`AutonomyMode`, a fixed, small
`DenyCategory` set — see Data Model for exactly which categories are actually evaluable per vendor),
no general-purpose rule engine.

### Vendor mechanism verified for `ungated` (2026-09-17 design session + this planning session)

- **Claude Code** (`claude.rs::build_command`, currently claude.rs:117-145): replace the hardcoded
  `.arg("--permission-mode").arg("default")` with `.arg("--permission-mode").arg("bypassPermissions")`
  and _omit_ `.arg("--mcp-config").arg(mcp_config)` / `.arg("--permission-prompt-tool").arg("mcp__haex-approve__approve")`
  entirely for this mode — confirmed via `claude --help` (v2.1.274) that `bypassPermissions` is a
  valid `--permission-mode` enum value, and confirmed live (design doc §4 item 5) that host isolation
  holds under it.
- **Codex** (`codex.rs::spawn_codex_app_server`, `thread/start` call at codex.rs:299-316): the
  approval posture is **not a CLI flag** for the `app-server` transport 007 actually uses (unlike the
  standalone `codex`/`codex exec` CLI's `-a`/`-s` flags, which do not apply here) — it is the
  `approvalPolicy` field of the `thread/start` JSON-RPC params. Verified 2026-09-17 against the
  installed CLI's own schema (`codex app-server generate-json-schema`, `ThreadStartParams` under the
  `v2` schema set): `approvalPolicy` accepts `"untrusted" | "on-request" | "never"` (an `AskForApproval`
  enum), and a separate, currently-unset `sandbox` field accepts `"read-only" | "workspace-write" | "danger-full-access"`
  (a `SandboxMode` enum). `ungated` therefore sets `"approvalPolicy": "never"` **and** explicitly adds
  `"sandbox": "workspace-write"` to the existing params object — the second part is a genuinely new
  field 007 never sends today (it currently relies on Codex's own default), made explicit here so
  `ungated` never silently degrades to `danger-full-access` if that default ever changes upstream.
  (Codex also exposes an `approvalsReviewer: "auto_review"` option — an automatic reviewing subagent —
  considered and not used: it is Codex's own built-in equivalent of `gated-permissive`, but adopting
  it would mean trusting Codex's internal review model instead of haex-vault's own deny-rule list, which
  is exactly the "harness decides risk, haex-vault's rule is an explicit additional filter" split the spec
  clarification rejected reversing.)

### Vendor mechanism for `gated-permissive` — reuses 007 unchanged, only the decision changes

Both vendors keep **exactly** their `standard`-mode command construction (`--permission-mode default`
for Claude; `"approvalPolicy": "on-request"` for Codex) — per the spec clarification, "what needs
approval" stays the delegate's own judgment, unchanged. The only change is inside
`approval_bridge::request_approval` (approval_bridge.rs:25-68): when the invocation's `AutonomyMode`
is `gated-permissive`, skip the `permission::decide(mode, risk)` call entirely (that reads
`chat.permission_mode`, which does not apply to this mode per spec FR-004) and instead evaluate the
incoming `(tool_name, input)` against the persisted `cli_delegate.deny_rules` — `Allow` unless a
configured category matches, `Deny` if it does, with the existing `Ask`-path's oneshot/event
machinery simply never invoked for this mode (no human turn in this loop by design).

### Per-vendor Deny Rule evaluability (verified against real schemas this session — a real asymmetry, not assumed uniform)

| Category                                                              | Claude Code                                                                                                                                                                 | Codex                                                                                                                                                                                                                                                                                                                                                                                                                 |
| --------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Workspace-root escape (write/read outside the invocation's workspace) | Evaluable — file-editing tool `input` carries `file_path` directly                                                                                                          | Evaluable for `CommandExecutionRequestApprovalParams` (`command`, `cwd` fields present) but **not evaluable** for `FileChangeRequestApprovalParams` — that payload (verified via `FileChangeRequestApprovalParams.json` schema) carries no path at all, only `itemId`/`reason`/`threadId`/`turnId`/`grantRoot`. A Codex file-change deny rule cannot be enforced from the approval payload alone with today's schema. |
| Network access                                                        | Recognized `Bash`/`WebFetch` tool names and command patterns are denied; an unclassifiable command fails closed (Claude's approval payload has no structured network field) | Structured and reliable — `CommandExecutionRequestApprovalParams.networkApprovalContext` (`host`, `protocol`) is present precisely when the call is network-triggered                                                                                                                                                                                                                                                 |
| Credential/secret-path patterns (`.ssh`, `.aws`, `.env`, `id_rsa`, …) | Evaluable — same `file_path`/`command` text matching as workspace-root escape                                                                                               | Evaluable for `CommandExecutionRequestApprovalParams` (`command` text), **not evaluable** for `FileChangeRequestApprovalParams` for the same reason as workspace-root escape                                                                                                                                                                                                                                          |

This table's Codex file-change policy is resolved: because
`FileChangeRequestApprovalParams` exposes no path, an enabled `WorkspaceEscape` or
`CredentialPaths` rule denies `CodexFileChange` fail-closed (FR-015, T008, T029). The other
categories keep their documented evaluability for Claude and Codex command execution; no upstream
schema change or category exception is required.

## Constitution Check

_GATE: Must pass before Phase 0 research. Re-check after Phase 1 design._

Evaluated against the haex-vault Constitution (`.specify/memory/constitution.md`, hard-pinned from
haex-hive, revision `336eaf1e`):

| Principle                                               | Status | Rationale                                                                                                                                                                                                                                                                                                           |
| ------------------------------------------------------- | ------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| I. No Secrets in Git                                    | ✓ PASS | No new secret-handling surface — Deny Rules are non-secret category identifiers in a preference row; delegate credentials are unchanged from 007.                                                                                                                                                                   |
| II. No Local Absolute Paths in Versioned Config         | ✓ PASS | Nothing new is versioned; workspace-root/deny-rule state is runtime preference data, not committed config.                                                                                                                                                                                                          |
| III. Project Identity Is Device-Independent             | ✓ PASS | `cli_delegate.deny_rules` is device-scoped exactly like `chat.permission_mode`, following the existing `PrefScope::Device` pattern — no new identity concept.                                                                                                                                                       |
| IV. Cross-Repo References Pin Immutable Revisions       | ✓ PASS | No new external harness content or dependency; `rmcp`'s existing pin is untouched.                                                                                                                                                                                                                                  |
| V. External Sources Are Opt-in Per Project              | ✓ PASS | N/A — no external harness content involved.                                                                                                                                                                                                                                                                         |
| VI. Self-Modifying Instructions Are Always Review-Gated | ✓ PASS | This feature's docs/spec edits land through normal PR review like any other change.                                                                                                                                                                                                                                 |
| VII. Relay Unavailability Never Blocks Local Work       | ✓ PASS | Autonomy mode and deny-rule evaluation are local subprocess + local preference reads, independent of haex-vault's sync relay.                                                                                                                                                                                       |
| VIII. No Concealment Instructions in Agent Output       | ✓ PASS | Spec FR-005 requires every gated-permissive turn to record every tool call and to make the active autonomy mode visible; `ungated` runs are explicitly documented (spec US2 scenario 2) as intentionally not producing a per-call record, which is disclosed to the user via the visible mode label, not concealed. |

**Result**: All gates PASS. No Complexity Tracking entry needed.

## Project Structure

### Documentation (this feature)

```text
specs/009-autonomous-delegate-mode/
├── plan.md                    # This file
├── spec.md                    # Feature specification (existing, from /speckit.specify + /speckit.clarify)
├── research.md                # Phase 0 output (this command)
├── data-model.md              # Phase 1 output
├── quickstart.md              # Phase 1 output
├── contracts/
│   └── tauri-commands.md      # New/changed Tauri commands and events
└── tasks.md                   # Phase 2 output (/speckit.tasks — NOT this command)
```

### Source Code (repository root)

Extends the existing `cli_delegate` module from 007; no new top-level project, no new module beyond
one new shared file.

```text
src-tauri/src/
├── adapters/
│   ├── mod.rs                           # ChatRequest gains `autonomy_mode: AutonomyMode` field (new,
│   │                                     #   default Standard) — the one cross-cutting change every
│   │                                     #   ProviderAdapter sees; local/api_key adapters ignore it
│   │                                     #   (spec: this feature is delegate-only)
│   └── cli_delegate/
│       ├── autonomy.rs                  # NEW — `AutonomyMode` enum (Standard/Ungated/GatedPermissive),
│       │                                 #   `DenyCategory` enum + its per-vendor match logic (the
│       │                                 #   table above), `PREF_DENY_RULES` constant + typed
│       │                                 #   get/set helpers over `storage::preferences`
│       ├── autonomy_tests.rs            # NEW — per-vendor deny-category matching unit tests, incl.
│       │                                 #   the Codex FileChange non-evaluability case as an explicit
│       │                                 #   documented behavior, not a silent gap
│       ├── claude.rs                    # spawn_claude_invocation (claude.rs:148-153) reads the
│       │                                 #   existing `req.autonomy_mode` — Ungated swaps the permission-mode
│       │                                 #   arg and omits --mcp-config/--permission-prompt-tool;
│       │                                 #   Standard/GatedPermissive unchanged
│       ├── codex.rs                     # spawn_codex_app_server's thread/start params (codex.rs:299-316):
│       │                                 #   branch on `req.autonomy_mode` — Ungated sets
│       │                                 #   approvalPolicy:"never" + sandbox:"workspace-write";
│       │                                 #   Standard/GatedPermissive unchanged
│       ├── approval_bridge.rs           # request_approval (approval_bridge.rs:25-68): branch at the
│       │                                 #   top on the invocation's AutonomyMode — GatedPermissive
│       │                                 #   calls the new autonomy::evaluate_deny_rules(...) instead
│       │                                 #   of permission::decide(mode, risk); Standard unchanged;
│       │                                 #   Ungated never reaches this function for Claude (no bridge
│       │                                 #   spawned) and never receives a callback for Codex
│       │                                 #   (approvalPolicy:"never" means Codex itself never asks)
│       └── mod.rs                       # CliDelegateAdapter::stream_chat (mod.rs:195-224) continues
│                                         #   passing its existing ChatRequest into
│                                         #   spawn_claude_invocation / spawn_codex_app_server; each
│                                         #   spawn function reads `req.autonomy_mode` directly, with
│                                         #   no separate AutonomyMode parameter
├── storage/
│   └── chat_messages.rs                 # migration 0017 adds nullable `autonomy_mode` beside
│                                         #   the existing `tool_source` column, satisfying spec
│                                         #   FR-005's "make clear which mode a turn ran under"
└── chat/
    └── turn/
        └── tool_round.rs                 # unchanged — this feature's approval path is entirely inside
                                          #   cli_delegate's own bridge, not the built-in tool loop

src/
├── composables/
│   └── useProviders.ts                  # + a typed accessor for the deny-rule preference, mirroring
│                                         #   the existing getPrefAsync/setPrefAsync usage pattern
│                                         #   (usePreferences.ts:33-50) — no new Tauri command, see
│                                         #   contracts/tauri-commands.md
├── components/
│   ├── chat/
│   │   └── DelegateAutonomyControl.vue  # NEW — a third ChatComposerControl-based toolbar control,
│   │                                    #   alongside ComposerSettingsPopover and PermissionPrompt in
│   │                                    #   [instance].vue:1032-1060, following PermissionPrompt.vue's
│   │                                    #   exact mode/update:mode prop shape (PermissionPrompt.vue:13-14,24-29).
│   │                                    #   Rendered only when the active model resolves to a
│   │                                    #   `cli_delegate` provider, reusing the existing
│   │                                    #   `p.kind === 'cli_delegate'` lookup already at [instance].vue:461
│   └── settings/
│       └── DelegateDenyRulesSetting.vue # NEW — persistent deny-rule checklist, following
│                                        #   DefaultModelSetting.vue's device/vault-scope-aware
│                                        #   preference-editing pattern, placed alongside
│                                        #   ConnectDelegateProvider.vue in settings (not inside it —
│                                        #   that component has no preference-editing code today)
├── pages/chat/[instance].vue            # + local `autonomyMode` ref, `update:mode` handler mirroring
│                                        #   `updatePermissionMode` (lines 483-500) but writing into
│                                        #   the outgoing ChatRequest instead of a persisted preference
│                                        #   (per spec FR-008 — never persisted)
└── i18n/locales/{de,en}.json            # actual path: src/i18n/locales/{de,en}.json; strings:
                                         #   autonomy mode labels/descriptions, deny-rule category
                                         #   labels, "not available for this backend" message
```

**Structure Decision**: One new backend file (`cli_delegate/autonomy.rs`) holds the two new enums and
the per-vendor deny-category matching logic in one place, since that logic is inherently
vendor-comparative (the asymmetry table above) and does not belong inside either `claude.rs` or
`codex.rs` alone. Every other backend change is a small, localized branch inside an existing function
(`build_command`, `thread/start` params, `request_approval`) — no parallel execution path, no new
top-level module, matching 007-cli-delegate's own precedent of staying additive at existing seams.
Frontend follows the same pattern: one new composer control (per-request, ephemeral) and one new
settings component (persistent deny rules), both modeled directly on an existing sibling component
rather than introducing a new UI pattern.

## Complexity Tracking

_No entries — Constitution Check found no violations._
