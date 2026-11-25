# Implementation Plan: Shared memory of the agents

**Branch**: `097-agent-memory-plan` | **Date**: 2026-11-22 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/097-agent-memory/spec.md` (amended 2026-11-22)

## Summary

A vault gets one memory: a set of Markdown pages with title, description and text, connected by `[[Title]]` pointers,
which the models of the local agent and the Abo-Agents read and write as their Agentenprofil lets them. External agents do
not reach it. The plan:

1. One tracked table `haex_agent_memory_pages` and a new Rust module `agent_memory` that owns it: store, limits (1,000
   pages of up to 10,000 characters), the check on writing, pointers parsed from the text, a search over title,
   description and text, Tauri commands for the window. "KI → Gedächtnis" lists, searches, shows and edits pages.
2. The right is a new group `memory` of `Rights` with the levels off, read, read and write, stored as one grant row.
   "Standard" has read and write from the factory, also when it was edited before this feature; every other profile
   starts at off; an external agent always resolves to off.
3. Five actions (`memory.search`, `memory.open`, `memory.add`, `memory.write`, `memory.remove`) with one native executor,
   addressed by title, in a routing domain of their own so local models are offered them.
4. A conversation takes a snapshot of the table of contents at its first turn (`chat_threads.memory_toc`). Every turn
   builds the memory block from that snapshot, cut to a tenth of the model's context, so it never changes in a running
   conversation. Abo-Agents get the same block in their system prompt.
5. A mark on the conversation (`chat_threads.memory_foreign_at`) is set when foreign content is first seen: by the local
   turn, by the MCP dispatch for an Abo-Agent's calls of haex-vault's tools, and by reading an Abo-Agent's own tool calls
   from its output. A write without the mark runs without a question in every chat mode but "Plan"; with it, it asks. The
   rule is one function for every kind of agent. The two existing dialogs get one shared preview.
6. A notice in the window after every write of an agent; a memory request set, measured with larger models (the run of the
   small models was dropped, FR-027 withdrawn 2026-11-25).

## Technical Context

**Language/Version**: Rust (edition of `src-tauri`), TypeScript / Vue 3 / Nuxt, as the rest of the repository.

**Primary Dependencies**: none new. Existing: haex-crdt write path and deletion markers, `grant_rows`/`grants`,
`agent_profiles` (resolver, subjects), `NativeExecutor` and the action catalog, `chat/tools/prompt.rs`,
`mcp_server/redact.rs`, the readers of `adapters/cli_delegate/`, `marked`, `ts-rs`, the settings components, `vue-sonner`.

**Storage**: migration `0069_agent_memory` (next free number after `0068_diagnostic_log`): the tracked table
`haex_agent_memory_pages` and the columns `chat_threads.memory_toc` and `memory_foreign_at`; `HAEX_TRIGGER_VERSION` 42 to 43. One more kind of row in `haex_agent_profile_grants` (group `memory`), no schema change there. No SQLite extension.

**Testing**: `cargo test --lib -- agent_memory grant_rows agent_profiles chat::tools::prompt chat::turn
adapters::cli_delegate`, a migration test next to `identity/migrations_assistant_speakers_tests.rs`, the integration tests
`src-tauri/tests/agent_memory_kinds.rs` and `agent_memory_injection.rs`, `scripts/check-mcp-actions.ts`,
`scripts/check-settings.ts`, a new `scripts/check-agent-memory.ts`, the e2e scenario `agent-memory.test.ts`, and the
measurement of research R11. Tests live in `*_tests.rs` files next to their module.

**Target Platform**: desktop (Linux, macOS, Windows) and Android. Nothing is platform specific; Abo-Agents exist on
desktop only, as today.

**Project Type**: desktop and mobile app (Tauri), Rust backend with a Nuxt frontend.

**Performance Goals**: the memory block costs one read of a thread column per turn. A search scans at most 10 MB in memory;
the pointer map is rebuilt on a change of the table. A write must not change the system prompt of the running
conversation (SC-007, research R5).

**Constraints**: the block stays inside the stable beginning of the system prompt that spec 080 will rely on; a right
this build cannot read counts as off (FR-005); nothing is shortened or removed automatically (FR-003); the page's actions
are not callable by agents; names use the `haex` prefix; no migration of earlier data.

**Scale/Scope**: up to 1,000 pages; one new Rust module of about twelve files, about eighteen touched Rust files and
fifteen frontend files.

## Constitution Check

_Checked against `.specify/memory/constitution.md` 1.4.0 and `.spaex/constitution.md`._

| Principle / rule                                        | Result                                                                                                                                                                                                                                                                                                                                                                                                                          |
| ------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| I No secrets in git                                     | Pass: nothing committed holds a secret; the tests of the check use made-up values.                                                                                                                                                                                                                                                                                                                                              |
| II No local absolute paths                              | Pass.                                                                                                                                                                                                                                                                                                                                                                                                                           |
| III Project identity device independent                 | Pass: a page names a vault device id and a subject key, never a path.                                                                                                                                                                                                                                                                                                                                                           |
| IV, V Pinned references, opt-in sources                 | Pass: no external content is referenced; Hermes Agent and graphify are cited as background only.                                                                                                                                                                                                                                                                                                                                |
| VI Self-modifying instructions review-gated             | Pass: the principle governs the instructions of this repository's own development agents, which this plan does not touch. The memory is data of the user's vault, given to the product's agents as notes and never as instructions (FR-014), visible and deletable. The operator decided that facts are written directly and that learned procedures, the part closest to this principle, will need approval in their own spec. |
| VII Relay unavailability never blocks local work        | Pass: reading and writing the memory are local; other devices get it by the existing sync.                                                                                                                                                                                                                                                                                                                                      |
| VIII No concealment                                     | Pass: every write of an agent raises a notice and is listed on the page; the check refuses text that asks to keep something from the user.                                                                                                                                                                                                                                                                                      |
| Conventional Commits, rebase or merge commit, no squash | Followed by the delivery below.                                                                                                                                                                                                                                                                                                                                                                                                 |
| Guardrails locked for agents                            | Pass: only the window sets the memory right; the page's actions are `scope: 'guardrails'`, `agentCallable: false`.                                                                                                                                                                                                                                                                                                              |
| Every device and OS                                     | Pass.                                                                                                                                                                                                                                                                                                                                                                                                                           |
| Specs in English, chat in German                        | Followed.                                                                                                                                                                                                                                                                                                                                                                                                                       |

No violation, so no entry in Complexity Tracking. The decision that agents write to a shared memory without approval, what
keeps foreign content out of it, and that external agents do not reach it, is recorded as ADR 0029 in PR B.

**Re-check after design (Phase 1)**: unchanged, all rows still pass.

## Delivery (pull requests, in order)

Each PR leaves the app working.

| PR  | Content                                                                                                                                                                                                                                                                                                                   | Visible to the user                                                  | Stories                  |
| --- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------- | ------------------------ |
| A   | Migration 0069, `agent_memory` (store, limits, check, pointers, search, commands), "KI → Gedächtnis" with list, search, page view, editing, store, window actions and texts                                                                                                                                               | the user keeps pages; no agent reads them yet                        | US3 (user's part), US6.4 |
| B   | The group `memory` in `Rights`, rows, resolver (external agents off), editor; the five actions and their executor (built-in agent only); the routing domain; snapshot and block in `start_turn`; the foreign mark from the local turn; the gate in `plan_calls`; the preview in the chat's approval; the notice; ADR 0029 | the local agent's models remember and find; the right in the profile | US1, US3, US4, US5, US6  |
| C   | Abo-Agents: the block in their system prompt; the executor serves their pass; the gate and the foreign mark in `dispatch.rs`; the Claude Code reader takes tool names; the Codex spike and its reader or config; the preview in `CallQuestionDetails.vue`; the integration test of the kinds                              | Claude Code and Codex remember and find, by the same rule            | US2, US5.4               |
| D   | The memory request set, the runner's seeded memory, the new safety kind, `tools.json`; the runs and `results.md` for hosted models and Claude Code/Codex Abo-Agents (no small-model run, FR-027 withdrawn)                                                                                                                | nothing new                                                          | FR-026, FR-027, SC-009   |
| E   | e2e scenario, `CONTEXT.md` (Gedächtnis, Seite, Verweis, Inhaltsverzeichnis, fremder Inhalt), quickstart checked by hand                                                                                                                                                                                                   | nothing new                                                          | all                      |

A is safe on its own. In B the executor refuses every caller but the built-in agent, so an Abo-Agent cannot write before
C gives it the same rule.

## Decisions of the operator (2026-11-22)

- A memory write without foreign content asks in no chat mode; "Plan" refuses it (FR-018).
- Pages with pointers instead of single entries; the table of contents at the start, search and pointers on demand; no
  labels, no SQLite graph extension, no LLM-built graph for now (research R1).
- The page is "Einstellungen → KI → Gedächtnis".
- A model that loses the right, also by a change of model in a running conversation, gets no memory from that turn on.
- Local, cloud and Abo-Agents behave the same (research R7).
- External agents do not reach the memory (FR-020).

For an Abo-Agent, stdout `tool_use` events and haex MCP calls arrive on separate pipes. Each delegate pass therefore
provides an awaited stdout-reader barrier: before dispatch decides a `memory.write`, it asks the reader to drain and
classify every complete stream record already emitted, in stdout order, and waits for its acknowledgement before reading
the foreign mark. The delegate flushes each stream record before issuing a later haex MCP call. If the reader barrier
fails, dispatch fails closed by asking. This prevents a fast `memory.write` handler from outrunning an earlier foreign
`tool_use` event.

Open, for PR C: what the Codex spike finds; if Codex's own tools cannot be seen, research R7 names the fallback, and the
operator sees the result before PR C lands.

## Project Structure

### Documentation (this feature)

```text
specs/097-agent-memory/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   └── commands-and-actions.md
├── analysis/            # PR D: memoryset.json, seed-memory.json
├── results.md           # PR D
└── tasks.md             # /speckit-tasks, not created here
```

### Source Code (repository root)

```text
src-tauri/src/
├── agent_memory/                    # new
│   ├── mod.rs                       # public API, Author
│   ├── store.rs                     # the table: list, get, add, change, remove (check-then-write, markers)
│   ├── limits.rs                    # the limits, memory_full, page_too_long
│   ├── check.rs                     # the check on writing (research R9)
│   ├── links.rs                     # pointers from the text, the map out and in
│   ├── search.rs                    # the search over title, description and text
│   ├── toc.rs                       # snapshot, the block, the cut by context, NO_START_TOC
│   ├── turn.rs                      # what a turn gets: snapshot taken, block, offered memory tools
│   ├── foreign.rs                   # is_foreign(source, scope), the mark on the thread
│   ├── gate.rs                      # decide(mode, foreign)
│   ├── executor.rs                  # NativeExecutor for the five actions, author of a call, the notice
│   ├── commands.rs                  # Tauri commands of the window
│   └── *_tests.rs
├── identity/
│   ├── migrations.rs                # register 0069, HAEX_TRIGGER_VERSION 43
│   ├── migrations_agent_memory.rs   # new
│   └── migrations_agent_memory_tests.rs
├── storage/chat_threads.rs          # read and write of memory_toc, memory_foreign_at
├── mcp_server/
│   ├── grants.rs                    # MemoryLevel, Rights.memory, default_rights, covers_scope
│   ├── grant_rows.rs                # the row of the group `memory`
│   ├── registry.rs                  # load_rights: external agents off
│   └── dispatch.rs                  # PR C: gate and mark for an Abo-Agent's thread
├── agent_profiles/resolve.rs        # "Standard" without a memory row
├── adapters/cli_delegate/claude.rs, codex.rs   # PR C: own tool calls set the mark
├── chat/
│   ├── send.rs                      # snapshot at the first turn, block into the prompt
│   ├── tools/prompt.rs              # system_prompt_with takes the block
│   ├── turn/plan_calls.rs           # the memory gate before decide_call
│   ├── turn/tool_round.rs           # the mark after a foreign call
│   └── eval/everyday/               # PR D: seeded memory, category, safety kind
└── lib.rs                           # commands, executor

src/
├── components/settings/agentMemory/MemoryView.vue, MemoryPageView.vue   # new
├── components/agents/MemoryRightsFields.vue                             # new
├── components/agents/RightsEditor.vue
├── components/agents/MemoryWritePreview.vue                             # new: shared by both dialogs
├── components/agents/CallQuestionDetails.vue, the chat's approval component
├── components/wm/Desktop.vue, appRoutes.ts                              # the notice listener, the route
├── lib/actions/memoryActions.ts, settingsAgentMemoryActions.ts (new), catalog.ts, scopes.ts, agentTools.ts
├── lib/actions/settingsAgentProfileActions.ts                           # RIGHTS_SCHEMA
├── lib/agents/rights.ts, lib/settings/registry.ts
├── stores/agentMemory.ts (new), stores/settingsActionHandlers.ts
├── types/bindings/                                                      # generated
└── i18n/locales/de.json, en.json

scripts/
├── check-agent-memory.ts (new), check-mcp-actions.ts, check-settings.ts
└── e2e/scenarios/agent-memory.test.ts (new)

docs/adr/0029-agents-write-a-shared-memory.md
```

**Structure Decision**: a new top-level Rust module `agent_memory`, as `agent_profiles` is one: the chat turn, the MCP
server, the Abo-Agent readers and the window all use it and none of them should own it. The rights shape stays in
`mcp_server/grants.rs`, the prompt assembly in `chat/tools/prompt.rs`.

## Complexity Tracking

No constitution violation to justify.
