# Implementation Plan: Composer Toolbar Parity (Real Effort, Sub-Agent Activity, Attachments)

**Branch**: `011-composer-toolbar-parity` | **Date**: 2026-09-19 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `specs/011-composer-toolbar-parity/spec.md`

## Summary

Three independent, additive changes to the composer's model/effort popover
(`src/components/chat/ComposerSettingsPopover.vue`) and its toolbar row
(`src/pages/chat/[instance].vue`):

1. **Real effort** (Story 1): replace the composer's fixed low/medium/high
   slider — which today only ever changes `max_new_tokens` and is never even
   sent to a `cli_delegate` subprocess — with a per-model/backend-aware
   control wired to Anthropic's real `output_config.effort` request field
   (direct API) and Claude Code's real `--effort` CLI flag (delegate),
   discovered via a new, tiny, synchronous `get_effort_levels` Tauri command
   that both request-builders also consult, so the displayed options and the
   wire behavior can never drift apart.
2. **Sub-agent activity** (Story 2): teach `adapters/cli_delegate/claude.rs`'s
   NDJSON parser to recognize the `parent_tool_use_id` field Claude Code's
   stream already carries on sub-agent messages (today silently ignored — the
   parser only handles `stream_event`/`result` lines), track active
   sub-agent count and per-turn batch size in a new small state machine, and
   surface it as a new `StreamChunk`/Tauri event the composer renders as a
   live "N agents" pill.
3. **Attachments** (Story 3): a new "+" control using the already-installed
   (currently unused) `@tauri-apps/plugin-dialog`/`@tauri-apps/plugin-fs` to
   pick files, a new `inspect_attachment` command for immediate size/type/
   usability feedback (FR-015/016 need this before send, not at send time),
   and content threaded through the existing per-message `ChatMessage`/
   `ChatRequest` shape into Anthropic image/document content blocks for the
   direct API and into the delegate's own isolated working directory (so its
   already-available Read tool can see it) for Claude Code.

No existing adapter, command, or event is removed; every change is additive
to `ChatRequest`/`SendMessageArgs`/`StreamChunk`, matching this codebase's
existing pattern for optional per-request fields (`autonomy_mode`,
`reasoning_requested`).

## Technical Context

**Language/Version**: Rust 1.77.2, edition 2021 (backend, `src-tauri`),
TypeScript 5 / Vue 3 Composition API (frontend, Nuxt 4 SPA) — unchanged.
**Primary Dependencies**: No new npm packages — `@tauri-apps/plugin-dialog`
(`^2.7.3`) and `@tauri-apps/plugin-fs` (`^2.5.2`) are already
`package.json` dependencies with zero current callers, so Story 3 is their
first real use. One new direct Rust dependency, `base64 = "0.22"`, added
the same way `sha2` already was (Cargo.toml's own comment on that line):
already present as a transitive dependency (pulled in by several existing
crates), pinned directly because this feature's attachment-encoding
boundary now calls it explicitly — no new external code enters the
dependency closure. Backend attachment reading uses `std::fs`; no image/PDF
parsing library is added — attachments are read as raw bytes and
base64-encoded, the same way Anthropic's Messages API already expects
image/document content blocks, with type/size classified from the file
extension and a byte-length check, not by parsing file contents.
**Storage**: No schema change. Attachments are per-message and
send-time-only (spec.md: out of scope to persist across messages), so they
never reach `chat_messages`/SQLite — they exist only as in-memory composer
state (frontend) and a request-scoped `Vec<Attachment>` (backend), read
fresh from disk on every send per FR-018.
**Testing**: `cargo test --lib` / `cargo test --test <name>`, tests in
sibling `*_tests.rs` files (repo convention, no inline `#[cfg(test)] mod
tests`). `pnpm typecheck` for the frontend changes. New pure logic (the
effort capability tables, the sub-agent batch tracker, attachment
classification) is designed as free functions precisely so each gets direct
`*_tests.rs` coverage without needing a live subprocess or HTTP mock,
mirroring `claude.rs::parse_line`'s existing test-friendliness (its own doc
comment: "so `claude_tests.rs` can exercise it directly without spawning a
real process").
**Target Platform**: Desktop only — unchanged; nothing here is delegate- or
subprocess-specific in a way that changes 007's existing desktop-only
reasoning, and file attachment via a native file dialog is itself a
desktop-shaped affordance (already reflected in `plugin-dialog` being a
Tauri, not web, dependency).
**Performance Goals**: The sub-agent activity indicator must update within
the same live-streaming latency budget `chat-token` deltas already meet
(effectively immediate, same event-emission path) — no new performance
target, reusing the existing `StreamChunk` → Tauri-event pipeline.
**Constraints**: Anthropic's `output_config.effort` is only sent for models
in a curated, versioned support table (Assumptions) — never sent to a model
outside it, since an unsupported model is not documented to ignore the
field gracefully. Claude Code's `--effort` flag is sent unconditionally
whenever the user picked a level, relying on Claude Code's own documented
per-model fallback ("Unsupported levels fall back to highest supported
level at or below the requested one") rather than haex-vault maintaining a
second copy of Claude Code's model/level table. A per-attachment size cap
(5 MB image / 32 MB PDF / 5 MB text, mirroring Anthropic's own Messages API
limits) and a 5-attachments-per-message cap, enforced both at attach time
(`inspect_attachment`) and again at send time (defense in depth, FR-018).
**Scale/Scope**: Single active generation at a time (existing
`ChatState`/`ActiveSession` invariant, unchanged). Sub-agent tracking is
scoped to one level of "is this tool call's id later referenced as a
parent" bookkeeping per in-flight delegate invocation — bounded by that
invocation's own tool-call count, discarded when the stream ends.

## Constitution Check

_GATE: Must pass before Phase 0 research. Re-check after Phase 1 design._

Evaluated against the haex-vault Constitution (`.specify/memory/constitution.md`,
hard-pinned from haex-hive, revision `336eaf1e`):

| Principle                                               | Status | Rationale                                                                                                                                                                                                           |
| ------------------------------------------------------- | ------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| I. No Secrets in Git                                    | ✓ PASS | No new credential or secret material. Attachment bytes are transient (read from disk at send time, never persisted, never logged).                                                                                  |
| II. No Local Absolute Paths in Versioned Config         | ✓ PASS | An attachment's filesystem path is per-invocation runtime state passed over Tauri IPC and into a request body/subprocess arg — never written into any versioned file.                                               |
| III. Project Identity Is Device-Independent             | ✓ PASS | No change to project/device identity or vault scoping.                                                                                                                                                              |
| IV. Cross-Repo References Pin Immutable Revisions       | ✓ PASS | No new external harness content. `base64` is a new _direct_ Cargo dependency, but not new external content — already resolved transitively (see Technical Context), same pattern this file already uses for `sha2`. |
| V. External Sources Are Opt-in Per Project              | ✓ PASS | N/A.                                                                                                                                                                                                                |
| VI. Self-Modifying Instructions Are Always Review-Gated | ✓ PASS | This spec/plan land through normal PR review like any other feature.                                                                                                                                                |
| VII. Relay Unavailability Never Blocks Local Work       | ✓ PASS | Effort levels, sub-agent tracking, and attachment reading are all local computation/subprocess/HTTP-adapter concerns, independent of haex-vault's sync relay.                                                       |
| VIII. No Concealment Instructions in Agent Output       | ✓ PASS | The agent-activity indicator's entire purpose is _more_ visibility into delegate behavior, not less; no hidden behavior introduced.                                                                                 |

**Result**: All gates PASS. No Complexity Tracking entry needed.

## Project Structure

### Documentation (this feature)

```text
specs/011-composer-toolbar-parity/
├── plan.md                    # This file
├── spec.md                    # Feature specification (existing)
├── research.md                # Phase 0 output (this command)
├── data-model.md              # Phase 1 output
├── quickstart.md              # Phase 1 output
├── contracts/
│   └── tauri-commands.md      # New/changed Tauri commands and events
├── checklists/
│   └── requirements.md        # Spec-quality checklist (existing)
└── tasks.md                   # Phase 2 output (/speckit.tasks — NOT this command)
```

### Source Code (repository root)

Extends the existing Tauri desktop structure; no new top-level project.

```text
src-tauri/src/
├── adapters/
│   ├── effort.rs                        # NEW: EffortLevel enum, as_str/parse, the curated
│   │                                     #   per-Anthropic-model support table, the fixed
│   │                                     #   Claude-Code-delegate level set, and a clamp()
│   │                                     #   helper shared by request.rs and the new Tauri
│   │                                     #   command (data-model.md)
│   ├── effort_tests.rs                  # NEW
│   ├── types.rs                         # + ChatRequest.effort_level: Option<EffortLevel>;
│   │                                     #   + ChatMessage.attachments: Vec<Attachment>
│   │                                     #   (data-model.md); + StreamChunk::AgentActivity
│   ├── request.rs                       # build_messages_body: + output_config.effort when
│   │                                     #   req.effort_level is Some and the model is in
│   │                                     #   effort::anthropic_supported_levels(); attachment
│   │                                     #   content blocks folded into the current user
│   │                                     #   message's `content` array (research.md §3)
│   ├── anthropic.rs                     # unchanged parsing — no new SSE event type needed
│   │                                     #   for effort or attachments
│   └── cli_delegate/
│       ├── claude.rs                    # build_command: + `--effort <level>` arg (no
│       │                                 #   per-model gating — Claude Code self-clamps,
│       │                                 #   research.md §1); parse_line: + assistant/user
│       │                                 #   message-type lines feed subagents::Tracker;
│       │                                 #   spawn_claude_invocation: writes attachment
│       │                                 #   bytes into the existing per-invocation `tmp`
│       │                                 #   dir and mentions their paths in the transcript
│       │                                 #   prompt (research.md §3)
│       ├── claude_tests.rs              # + parse_line/subagent-tracking cases
│       ├── subagents.rs                 # NEW: Tracker — pending/active tool_use ids,
│       │                                 #   batch sizing, active-count bookkeeping
│       │                                 #   (data-model.md "Sub-Agent Batch")
│       ├── subagents_tests.rs           # NEW
│       └── mod.rs                       # + subagents module registration
├── chat/
│   ├── events.rs                        # + EVENT_CHAT_AGENT_ACTIVITY, AgentActivityEvent
│   ├── attachments.rs                   # NEW: classify_attachment (kind/size/usability per
│   │                                     #   provider kind+adapter), read_attachment_content
│   │                                     #   (data-model.md "Message Attachment")
│   ├── attachments_tests.rs             # NEW
│   ├── commands.rs                      # SendMessageArgs: + effort_level, + attachments;
│   │                                     #   send_message: builds the current turn's
│   │                                     #   attachments into its ChatMessage; + a small
│   │                                     #   effort_levels/inspect_attachment command pair
│   │                                     #   (contracts/tauri-commands.md)
│   └── turn/
│       └── step.rs                      # consume_stream: + StreamChunk::AgentActivity arm
│                                         #   emitting EVENT_CHAT_AGENT_ACTIVITY
└── lib.rs                               # + get_effort_levels / inspect_attachment command
                                          #   registration

src/
├── composables/
│   └── useChat.ts                       # + AgentActivityEvent/onAgentActivity; + effort/
│                                         #   attachment fields on SendMessageArgs;
│                                         #   + getEffortLevelsAsync/inspectAttachmentAsync
├── components/chat/
│   ├── ComposerSettingsPopover.vue       # effort section becomes a ShadcnSelect over a
│   │                                     #   dynamic `effortLevels` prop (+ "auto"),
│   │                                     #   hidden entirely when that list is empty,
│   │                                     #   replacing the fixed 3-stop slider
│   ├── AgentActivityIndicator.vue        # NEW: small "● N agents" pill, `v-if="count > 0"`
│   └── ComposerAttachments.vue           # NEW: "+" trigger (plugin-dialog file picker) +
│                                         #   the attached-file chip list with per-file
│                                         #   remove and a usability/error state
└── pages/chat/
    └── [instance].vue                    # wires the three new pieces into the toolbar row;
                                           # effortLevel default becomes `null` ("auto");
                                           # new attachments ref, reset alongside `input`
                                           # on send; new activeAgentCount ref, reset
                                           # alongside streamingMessageId/busy

specs/011-composer-toolbar-parity/ (this feature's own docs, listed above)
```

**Structure Decision**: Every backend change is additive to the existing
`adapters`/`chat` module layout — a new `adapters/effort.rs` (mirrors how
`adapters/request.rs` already isolates one cohesive concern),
`cli_delegate/subagents.rs` (mirrors `approval_bridge.rs` living next to
`claude.rs`/`codex.rs` as delegate-specific, not shared, logic — Codex has
no sub-agent concept per spec.md's own scoping), and `chat/attachments.rs`
(mirrors `chat/tools/` being where request-adjacent-but-not-adapter logic
already lives). No existing module is split or renamed. On the frontend,
`ComposerSettingsPopover.vue` is edited in place (its model-picking half is
untouched; only its effort half changes shape) rather than split into two
components, because Claude Code's own reference toolbar already renders
model name and effort level inside one pill — the existing combined-trigger
button already matches that shape, so this feature does not need to invent
a second pill. `AgentActivityIndicator.vue` and `ComposerAttachments.vue`
are new because neither concept has any existing analog to extend.

## Complexity Tracking

_No entries — Constitution Check found no violations._
