# Implementation Plan: Unified, Cached Model Capabilities

**Branch**: `012-unified-model-capabilities` | **Date**: 2026-09-20 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/012-unified-model-capabilities/spec.md`

## Summary

Replace five independently-shaped, uncached mechanisms that decide what a model supports (a curated
effort table, a delegate "always all levels" rule, a model-id reasoning heuristic, a version parser
for adaptive thinking, and a provider-kind attachment rule) with **one persisted capability record
per model** — reasoning control, accepted attachment kinds, and one adapter hint — populated live
from the provider where it exposes capability data (Anthropic API key and the Claude Code delegate
share one code path today), computed locally for built-in models, and read from the cached
`models` row by every caller.

The user's selected reasoning option is a **separate, per-model, per-device preference** owned by the
existing Pinia model store (no second store), validated against the record and never written to the
`models` table.

Approach, in delivery order (each stage compiles and passes tests on its own):

1. **Backend record + storage** — `model_capabilities.rs`, migration `0018`, `ModelRow` column,
   trigger version bump, `AttachmentKind` serde.
2. **Providers populate it** — Anthropic wire mapping (covers the Claude delegate), Codex "not
   determined", local at registration plus a lazy backfill for legacy local rows.
3. **Consumers switch to the record** — `send_message`, `inspect_attachment`, the Anthropic
   serializer; the five legacy mechanisms and `get_effort_levels` are deleted.
4. **Frontend** — behavior-preserving split of the oversized `useModelsStore`, then the reasoning
   preference slice, popover states, page cleanup.
5. **Refresh action** — the per-provider "Refresh models" button (FR-022), which the spec depends on
   and the UI did not have.

Two gaps in the source design were found while verifying against the code and are resolved in
[research.md](research.md): the Anthropic serializer needs an adaptive-vs-manual signal the shared
enum could not carry (R2), and legacy local models would never receive capabilities without a
backfill (R4). A third, the missing UI refresh path, became FR-022 (R9).

## Technical Context

**Language/Version**: Rust 1.95, edition 2021 (backend, `src-tauri`); TypeScript 5 / Vue 3
Composition API on a Nuxt 4 SPA with Pinia (frontend). Unchanged.

**Primary Dependencies**: None added. Uses `serde`/`serde_json`, `log`, `rusqlite` via `haex_crdt`,
`wiremock` (already a dev-dependency, used by `anthropic_tests.rs`); frontend uses existing
`usePreferences`, `useDevice`, Pinia.

**Storage**: SQLite (CRDT-tracked). One nullable column `models.capabilities_json TEXT`
(migration `0018_models_add_capabilities`, `HAEX_TRIGGER_VERSION` 9 → 10). The effort preference
uses the existing `preferences` table, device scope, no schema change.

**Testing**: Rust — sibling `*_tests.rs` files and `tests/` integration tests (`cargo test`,
`cargo fmt --check`, `pnpm lint:rust`). Frontend — the established replay harness
`scripts/check-chat-state.ts` (`pnpm check:chat-state`, run in CI) plus `pnpm typecheck`,
`pnpm typecheck:scripts`, `pnpm check:templates`, `pnpm lint`, `pnpm format:check`. No test runner
is introduced.

**Target Platform**: Desktop (Tauri), unchanged.

**Project Type**: Desktop application, Rust backend + Vue SPA frontend.

**Performance Goals**: No new request on model switch or composer open (SC-005): capabilities ride
the list payloads the store already fetches; the composer reads a `computed`. Provider refresh adds
no extra HTTP call — the listing already returns the capability tree.

**Constraints**: No automatic network refresh at launch (FR-012). Storage must not depend on adapter
behavior (shared leaf module). Async code must not block the executor — the model-row read stays
inside the existing `spawn_blocking` lookup in `send_message`/`inspect_attachment`. No `unwrap`/
`expect` on stored or network data.

**Scale/Scope**: Tens of models per provider; a handful of providers. Backend touch points measured
in [research.md](research.md) R5 (17 files with `ChatRequest` literals, 5 with `ModelRow`, 3 with
`ProviderModel`) — mechanical, compiler-enumerated.

## Constitution Check

_GATE: passed before Phase 0; re-checked after Phase 1 design (below). Sources: the repository's
`.spaex/constitution.md` behavior harness and `.specify/memory/constitution.md` (v1.4.0)._

| Requirement                                                           | Assessment                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| --------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| All changes in a dedicated worktree on a topic branch                 | ✅ `.worktrees/012-unified-model-capabilities`, branch `012-unified-model-capabilities`; primary checkout untouched.                                                                                                                                                                                                                                                                                                                                                                                         |
| Test code in separate files, in every language                        | ✅ Rust: new `model_capabilities_tests.rs`, `storage/models_tests.rs`, extended existing `*_tests.rs` (`#[cfg(test)] mod x_tests;` declarations are the allowed registration). Frontend: cases go in the harness script, not in production files.                                                                                                                                                                                                                                                            |
| Graphify consulted before authoring named artifacts                   | ✅ See research R7: no parallel candidate exists; `AttachmentKind`, the `backfill_*` idiom, `usePreferences`, `useDevice`, and the flat-composable convention are extended rather than duplicated. Graph used as the worktree fork-point snapshot.                                                                                                                                                                                                                                                           |
| 500-LoC boundary for hand-maintained files                            | ⚠️ Handled — see Complexity Tracking. `useModelsStore` (625) is **split** as a prerequisite; new files stay well under 500 (the Anthropic wire mapping lives in its own module so `adapters/anthropic.rs`, 448 today, stays under the boundary); three touched files already over 500 have documented exceptions (`chat/commands.rs`, `models/commands.rs`, `[instance].vue`) and shrink or barely change; `providers/mod.rs` (562) and `useChat.ts` (575), neither with an exception on file, each get one. |
| Simplifications carry a `ponytail:` comment                           | ✅ Planned at: `local_model_reasoning` (id heuristic), whole-list attachment granularity, lazy local backfill.                                                                                                                                                                                                                                                                                                                                                                                               |
| Non-trivial logic leaves one runnable check                           | ✅ Each new function has a test in the established runner (data-model.md, research R11).                                                                                                                                                                                                                                                                                                                                                                                                                     |
| Do not abstract merely because things look similar                    | ✅ One record justified by identical ownership, change reason and invariants across the five mechanisms; the three frontend composables split by distinct responsibility, not symmetry.                                                                                                                                                                                                                                                                                                                      |
| No new crate/abstraction without a concrete problem                   | ✅ No crate added; the adapter hint field exists because deleting the version parser otherwise loses behavior (R2).                                                                                                                                                                                                                                                                                                                                                                                          |
| Rust: no `unwrap` on data, errors keep context, closed sets are enums | ✅ `ReasoningControl`/`ThinkingStyle` are enums; JSON parse failure is logged with the model id, not discarded (R6).                                                                                                                                                                                                                                                                                                                                                                                         |
| Non-blocking async                                                    | ✅ Row reads stay in `spawn_blocking`.                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| ADR for decisions materially affecting a Core Principle               | ✅ None affected (secrets, absolute paths, identity, references, opt-in, self-modification, relay, concealment). No ADR needed. Device-scoped preference follows ADR 0001.                                                                                                                                                                                                                                                                                                                                   |
| No local absolute paths, no secrets in committed files                | ✅ All artifacts use repository-relative paths.                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| Agents leave no self-references in project artifacts                  | ✅ None in spec artifacts; commit messages must omit model trailers (constitution MUST NOT).                                                                                                                                                                                                                                                                                                                                                                                                                 |
| Conventional Commits; PR to `main`; no squash                         | ✅ Enforced at commit/PR time per stage (tasks will name types: `feat`, `refactor`, `test`, `docs`).                                                                                                                                                                                                                                                                                                                                                                                                         |
| Phasing discipline                                                    | ✅ Extends already-delivered spec 011 (composer); no later-phase work pulled forward.                                                                                                                                                                                                                                                                                                                                                                                                                        |
| `/speckit-plan` checks against constitution                           | ✅ This table; no unjustified violation.                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |

**Post-design re-check**: the design added `useModelInventory`, `useModelIntegrity`,
`useReasoningPreference`, `backfill_local_capabilities` and one adapter-hint field. Each is a
concrete, tested unit justified in research (R2, R4, R7); none is speculative. No new violation.

## Project Structure

### Documentation (this feature)

```text
specs/012-unified-model-capabilities/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── tauri-commands.md
│   └── capabilities-json.md
├── checklists/requirements.md
└── tasks.md            # created by /speckit-tasks
```

### Source Code (repository root)

```text
src-tauri/src/
├── model_capabilities.rs             # NEW  record, enums, local_model_reasoning
├── model_capabilities_tests.rs       # NEW
├── lib.rs                            # register module; drop get_effort_levels
├── identity/migrations.rs            # 0018 + trigger version 10
├── storage/
│   ├── models.rs                     # capabilities column, lenient row_to_model, backfill_local_capabilities
│   └── models_tests.rs               # NEW  round-trip, NULL legacy row, malformed JSON, backfill
├── adapters/
│   ├── mod.rs                        # ProviderModel.capabilities; drop effort module
│   ├── effort.rs, effort_tests.rs    # DELETED
│   ├── types.rs                      # AttachmentKind serde; ChatRequest fields
│   ├── anthropic.rs                  # ModelInfo carries the wire capabilities; calls the mapper
│   ├── anthropic_capabilities.rs     # NEW  wire structs + map_capabilities (+ _tests.rs)
│   ├── request.rs                    # serializer reads the record; supports_adaptive_thinking deleted
│   └── cli_delegate/
│       ├── mod.rs                    # Codex arm: default (not determined)
│       └── claude.rs                 # --effort takes &str
├── chat/
│   ├── commands.rs                   # send_message/inspect_attachment read the row; legacy fns deleted
│   └── attachments.rs                # usability_for takes capabilities
├── models/commands.rs                # register_downloaded + list_installed_models backfill call
└── providers/mod.rs                  # compose_model_row, payload; documented size exception
src-tauri/tests/                      # mechanical literal fixes; tests/common/tool_loop_fixture.rs

src/
├── composables/
│   ├── useModelInventory.ts          # NEW  (moved from store) + capabilitiesFor
│   ├── useModelIntegrity.ts          # NEW  (moved from store)
│   ├── useReasoningPreference.ts     # NEW  effort preference slice
│   ├── useModels.ts / useProviders.ts / useChat.ts   # DTO types; sendMessageAsync; drop getEffortLevelsAsync; useChat gets a size-exception header
├── stores/models.ts                  # slimmed; re-exports same API + new exports
├── pages/chat/[instance].vue         # drop local effort refs/watcher; read from store
├── components/chat/ComposerSettingsPopover.vue       # effortState prop
├── components/settings/ConnectDelegateProvider.vue   # Refresh models button
└── i18n/locales/{en,de}.json         # state labels, refresh button

scripts/check-chat-state.ts           # new effort/capability cases; useDevice auto-import
```

**Structure Decision**: Existing Rust-backend / Vue-frontend layout is kept. The capability module is
top-level (a shared leaf, like `crate::error`) so `storage/` and `adapters/` share it without
`storage` gaining a behavioral dependency on `adapters`. Frontend helpers are flat composables (the
repository's convention, and the only shape the replay harness loads without changes — research R7).

## Complexity Tracking

| Violation                                                                                             | Why Needed                                                                                                             | Simpler Alternative Rejected Because                                                                                                                                                                                                                                                                                             |
| ----------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `scripts/check-chat-state.ts` grows past its documented ~985-line exception (+ the new cases)         | The store's new behavior can only be tested through this harness — it is the sole frontend test mechanism and CI gate. | Extracting the harness into `scripts/lib/` first (the file header's own split plan) is a separate mechanical refactor of a 985-line file; bundling it would mix a refactor with a behavior change. Header count is updated and the header states a cap: no further cases before extraction (T053); extraction stays a follow-up. |
| `providers/mod.rs` (562 lines) and `useChat.ts` (575 lines) touched with no documented size exception | The feature adds ~4 lines (`compose_model_row`, payload).                                                              | Splitting it here is unrelated scope; instead a header documents the reason and a concrete split plan, satisfying the rule without a drive-by refactor.                                                                                                                                                                          |
| `thinking_style` adapter hint inside the shared record                                                | Removing `supports_adaptive_thinking` loses adaptive-vs-manual selection unless the live datum is stored.              | A per-adapter opaque blob (speculative generality) or deriving it from effort presets (incorrect) — see research R2.                                                                                                                                                                                                             |
