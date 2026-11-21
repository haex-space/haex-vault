# Research: Unified, Cached Model Capabilities

All unknowns from the plan's Technical Context are resolved below. Each entry records what was
verified in the code, not assumed. Paths are repository-relative.

## R1. Where the record lives and what it contains

**Decision**: One record, `ModelCapabilities`, owned by a new top-level leaf module
`src-tauri/src/model_capabilities.rs`, persisted as JSON in a new nullable `models.capabilities_json`
column and carried on `ModelRow`, `ProviderModel`, `ChatRequest` and both list payloads.

```text
ModelCapabilities {
  reasoning:                  Option<ReasoningControl>,        // None = not determined
  accepted_attachment_kinds:  Option<Vec<AttachmentKind>>,     // None = not determined
  thinking_style:             Option<ThinkingStyle>,           // adapter hint, see R2
}
ReasoningControl = Unavailable | ModelManaged | Presets { options: Vec<ReasoningOption> }
ReasoningOption  = { id: String, label: String }
```

**Rationale**: The spec's unknown-vs-unsupported rule (FR-002) maps directly to `Option` at each
field; `ReasoningControl` is a closed set, so an enum (constitution: enums for closed sets, `Option`
for optional). The module is a leaf so `storage/` and `adapters/` can both use it without `storage`
depending on `adapters` for behavior.

**Alternatives considered**:

- Reuse `EffortLevel` + `clamp`: rejected — one global ordered scale cannot express providers that
  have `max`, `minimal`, or numeric budgets (spec Edge Cases); `EffortLevel` is deleted.
- A second Pinia store or a `get_model_capabilities` command: rejected — the two list DTOs already
  flow into the store, so a second IPC path would duplicate the cache (plan, Frontend design).

**Naming note**: `providers.capability` (`chat` | `transcription`, migration `0016`) already exists.
It classifies a _provider_ and is unrelated; the new names (`ModelCapabilities`, column
`capabilities_json`) are distinct on purpose, and the enum variant is `ModelManaged`, not
`Automatic`, to avoid colliding with the user-side state "Auto" (spec clarification).

## R2. Adaptive vs. manual thinking needs a place in the record (plan gap)

**Finding**: `adapters/request.rs::build_messages_body` picks `thinking: {type: "adaptive"}` or a
manual `budget_tokens` form from `supports_adaptive_thinking(model_id)`, a substring/version parser.
The plan deletes that function and says the serializer "maps `ReasoningControl`" — but
`Presets | ModelManaged | Unavailable` cannot say _how_ a thinking-capable model wants to be asked
(newer models reject `budget_tokens`; older ones reject `adaptive`). The datum has to come from the
live response (`capabilities.thinking.types.{adaptive,enabled}.supported`).

**Decision**: Add `thinking_style: Option<ThinkingStyle>` (`Adaptive | Manual`) to the record,
documented as an adapter-private hint: only the adapter that produced it reads it; local and Codex
leave it `None`. The Anthropic serializer sends `thinking` only when reasoning is
`Presets | ModelManaged` **and** `thinking_style` is `Some`; `None` sends no `thinking` field
(conservative — never a value the model might reject).

**Rationale**: Smallest change that removes the version parser without losing behavior. The
plan's wish that the shared type stay Anthropic-free is honored in spirit: the field is named for
the _concept_ (how thinking is requested), not for Anthropic wire strings.

**Alternatives considered**: an opaque `serde_json::Value` bag per adapter (rejected — speculative
generality, untestable); deriving adaptivity from "has effort presets" (rejected — wrong: Opus 4.5
has effort but manual thinking).

## R3. Anthropic wire mapping

Source: Anthropic Models API, `GET /v1/models` (also used by the Claude Code delegate via
`cli_delegate::fetch_claude_models`, which calls the same `anthropic::fetch_models` — no change in
`cli_delegate`). Response per model: `capabilities.image_input.supported`,
`capabilities.pdf_input.supported`, `capabilities.thinking.{supported, types.{enabled,adaptive}.supported}`,
`capabilities.effort.supported` plus provider-native option keys whose values contain
`supported`. The wire names are data, not a closed set: the adapter preserves every option key,
including names or numeric-budget identifiers introduced by the provider, and does not turn the
current Anthropic vocabulary into a shared enum.

| Wire                                                                | → Record                                                         |
| ------------------------------------------------------------------- | ---------------------------------------------------------------- |
| `effort.supported == true` and ≥1 provider-native level `supported` | `Presets`, options in provider response order (ids = wire names) |
| else `thinking.supported == true`                                   | `ModelManaged`                                                   |
| else both leaves present and false                                  | `Unavailable`                                                    |
| `thinking`/`effort` subtree missing                                 | `reasoning = None`                                               |
| `thinking.types.adaptive.supported`                                 | `thinking_style = Adaptive`                                      |
| else `thinking.types.enabled.supported`                             | `thinking_style = Manual`                                        |
| `image_input` **and** `pdf_input` both present                      | `Some([Text] + Image if supported + Document if supported)`      |
| either key missing                                                  | `accepted_attachment_kinds = None`                               |

`Text` is always included once attachment support is determined: text files are inlined as a plain
text block (`request.rs`, `AttachmentKind::Text`), which every message-capable model accepts. All
nested wire structs use `#[serde(default)]`, so absent or additive fields never fail the parse
(FR-002).

**Open verification (not a blocker)**: the vendor docs sample elides most capability leaves. A
verification task (quickstart §3) records a fixture from a real listing to confirm `pdf_input`
exists. If it does not, the fallback is
per-kind granularity (`Document` from `pdf_input` only when present) — decided at that point, since
whole-list granularity would otherwise mark every Claude model's attachments as undetermined.

**Where the mapping lives**: `adapters/anthropic.rs` is 448 lines, so the wire structs and the
pure `map_capabilities` function go into a sibling module `adapters/anthropic_capabilities.rs`
(with its own `_tests.rs`), which also lets the seven fixture cases run without an HTTP double.

**Second open verification (delegate path)**: the Claude Code delegate lists models with an OAuth
bearer token, not an API key (`fetch_claude_models`). That the OAuth-authenticated response carries
the same `capabilities` tree is unverified. Quickstart §3 captures both. If the OAuth listing lacks
it, try the single-model endpoint `GET /v1/models/{id}` with the same bearer; if neither returns
capabilities, stop and amend the spec (FR-003) instead of reintroducing a static table (FR-019).

## R4. Local models: no provider to ask, no refresh path

**Finding (plan gap)**: the plan sets local capabilities in `models/commands.rs::register_downloaded`
and calls `NULL` "honestly correct" for legacy rows. That holds for provider rows (a refresh fills
them) but **not for local rows registered before the migration** — no refresh exists for them, so
they would stay undetermined forever and silently lose reasoning (FR-023) after the change.

**Decision**: (1) `register_downloaded` computes `local_model_reasoning(&id)` for new rows;
(2) a new `storage::models::backfill_local_capabilities`, a sibling of the existing
`backfill_tokenizer_repo` / `backfill_source_kind`, fills `NULL` capabilities for
local-provider rows lazily from `list_installed_models` (the same idiom, idempotent, selects only
rows still `NULL`). `local_model_reasoning` returns `ModelManaged` for reasoning families and
`Unavailable` otherwise; local `accepted_attachment_kinds = Some([])`.

**Rationale**: The source_kind precedent applies exactly: a pure function of the row id whose
`NULL` default is actively wrong for old rows. `local_model_reasoning` is the body of today's
`model_supports_reasoning`, moved verbatim (marked with a `ponytail:` comment: naive id heuristic;
upgrade path is reading the GGUF chat template).

**Alternatives considered**: computing at read time for `NULL` local rows (rejected — a second,
uncached derivation path contradicts the feature's purpose).

## R5. `reasoning_requested` and the request literal

`send_message` reads the cached `ModelRow` once inside its existing blocking history lookup
(`get_model(conn, &session.model_id)`; local ids and composite ids both resolve). From that one
snapshot it derives, with no other lookups:

- `reasoning_requested = matches!(reasoning, Some(Presets | ModelManaged)) && !qwen3_tool_request`
  (the qwen3 tool exception in `reasoning_requested_for` is kept; it is a runtime rule, not a
  capability).
- `reasoning_option`: the client's option id, kept only if it is in `Presets.options`; otherwise
  `None` (FR-013).
- `capabilities` for the serializer.

`ChatRequest.effort_level: Option<EffortLevel>` becomes `reasoning_option: Option<String>` plus
`capabilities: Option<ModelCapabilities>`. `SendMessageArgs.effort_level` becomes
`reasoning_option: Option<String>`. The Claude delegate keeps passing the validated id to
`--effort` (`claude.rs` takes `Option<&str>` instead of `EffortLevel`).

**Touch points (measured, larger than the plan's estimate)**: `ChatRequest` literals in 17 files
(~30 sites; production: `chat/commands.rs`; the rest tests: `adapters/*_tests.rs`,
`chat/turn/step_tests.rs`, `tests/common/tool_loop_fixture.rs`, 10 integration tests);
`ModelRow` literals in 5 files (`models/commands.rs`, `providers/mod.rs` ×2, `storage/models.rs`,
`tests/huggingface_models.rs` ×9, `tests/provider_models.rs` ×2); `ProviderModel` literals in 3 files.
`cargo build --tests` enumerates them exhaustively; the tasks step treats this as one mechanical
change.

## R6. Storage and migration

- Migration `0018_models_add_capabilities`: `ALTER TABLE models ADD COLUMN capabilities_json TEXT;`
  Bump `HAEX_TRIGGER_VERSION` 9 → 10 and add its line to the version list in
  `identity/migrations.rs` (verified: current value 9, last migration `0017`; the file documents
  the bump as mandatory for CRDT-tracked tables).
- `upsert_model` overwrites the column unconditionally (like `context_window`), never `COALESCE`.
  Rationale: a refresh must replace stale capabilities entirely (spec Edge Cases).
- **Malformed JSON (spec wins over plan)**: the plan says "propagate a contextual error"; FR-021
  requires the model list to keep loading. Decision: `row_to_model` parses leniently — on a parse
  error it yields `capabilities: None` and logs `log::warn!` with the model id and the serde error
  (the `log` crate is already used). The error is not discarded, just not fatal.

## R7. Frontend structure and the test harness constraint

**Finding**: `src/stores/models.ts` is 625 lines (over the 500 boundary, no documented exception),
and the state to add must not grow it. The only frontend test mechanism is
`scripts/check-chat-state.ts` (`pnpm check:chat-state`, run in CI): it replays the page's real
`<script setup>` and the real store with Tauri mocked at `invoke`, loading each import through its
own `req()` resolver that **throws on any specifier it does not know**. Composables are resolved
generically by name from `src/composables/`; store sub-modules would not be.

**Decision**: split into three flat composables under `src/composables/`, each a function that
takes its dependencies as parameters and returns refs/computeds/functions the store re-exports,
so no Nuxt auto-import is needed inside them and the harness's existing generic composable branch
loads them with no harness change:

| New file                    | Owns (moved from `models.ts` unless new)                                                                                                                                                                                                                                   |
| --------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `useModelInventory.ts`      | `installedModels`, `catalogEntries`, `providerList`, `providerModels`, `refreshInstalledAndCatalog`, `refreshProviders`, `modelGroups`, `noModelsInstalled`, `findModelName`, **`capabilitiesFor(id)` lookup** (new; the store derives `displayModelCapabilities` from it) |
| `useModelIntegrity.ts`      | `integrityDialog`/`Busy`/`ActionError` and the five `onIntegrity*` handlers                                                                                                                                                                                                |
| `useReasoningPreference.ts` | **new**: `effortLevel`, `updateEffortLevel`, `loadEffortPreference`, revalidation, the load token                                                                                                                                                                          |

`models.ts` keeps `activeModel`, the load lifecycle, `initialize`, listeners and re-exports the same
public API (behavior-preserving; estimated ~350 lines after). The store additionally injects
`useDevice` into the harness's auto-import list (one line in `check-chat-state.ts`), because
`initialize()` now resolves the device UUID.

**Graphify consultation** (constitution): queries for existing capability/reasoning helpers found
only the code being replaced (`model_supports_reasoning`, `supports_adaptive_thinking`,
`effortIndex` in the popover) — nothing to extend in parallel. Queries for store-split precedents
found the flat-composable convention (`useModels`, `useCatalog`, `useChat`) and the earlier
`chat/turn/` split — the three composables follow that convention. `AttachmentKind`
(`adapters/types.rs`) is extended with serde derives rather than duplicated;
`backfill_local_capabilities` extends the `backfill_*` idiom; `usePreferences`/`useDevice` are
reused as-is.

## R8. Preference key and semantics

- Key `chat.reasoning_option.<model-id>` in the **device** scope, with `<model-id>` the composite
  `{provider_id}:{remote_id}` (already unique per connection, verified in `compose_model_row`), so
  one model reached through two connections stores separately (FR-015). `validate_key` only
  requires a non-empty key containing `.` — satisfied.
- Missing key = Auto; selecting Auto calls `clearPrefAsync`.
- `loadEffortPreference(modelId)` uses a monotonically increasing token and re-checks
  `displayModelId === modelId` after each await (FR-016).
- `updateEffortLevel` is optimistic with rollback on a failed `setPrefAsync`/`clearPrefAsync`,
  surfacing through the store's `lastError` (FR-018).
- Revalidation (FR-017): a watcher on `displayModelCapabilities` revalidates the active model
  whenever any list refresh changes it; a non-active model is validated lazily when next selected
  (`loadEffortPreference`). This covers "refreshed **or** next selected" in Story 2 without scanning
  every preference key.

## R9. The missing refresh action (spec amendment)

**Finding**: `refresh_provider_models` and `useProviders().refreshModelsAsync` exist, but nothing in
`src/` calls them; only `add_provider` and the delegate connect flow refresh. Settings offers
connect/"Reconnect" only. The plan's "one click to fix in Settings" did not exist.

**Decision**: add a small "Refresh models" button per connected provider in
`src/components/settings/ConnectDelegateProvider.vue` (273 lines), calling the existing
`refreshModelsAsync`. The chat page already calls `modelStore.initialize()` on mount, which
re-reads both lists, so the Settings component needs no store coupling. New i18n keys in `en.json`
and `de.json`. Recorded as FR-022.

**Alternatives considered**: refresh providers with `NULL` capabilities at store `initialize()`
(rejected — spec FR-012 forbids launch-time network calls); refresh on model selection (rejected —
new hidden network behavior outside this spec).

## R10. Effort UI states (from the clarification)

Composer settings popover (`ComposerSettingsPopover.vue`, plain props today) gains one derived
`effortState`: `hidden` (Unavailable, or no resolved model row — lists still loading, nothing selected), `disabled-managed` (ModelManaged), `disabled-unknown` (a row exists and its capabilities are `None`), `selectable` (Presets). Every offered option uses its provider-native `id`; its adapter-provided `label` is the display fallback and defaults to that id when the wire response has no separate label. Optional localized aliases are presentation-only and MUST NOT constrain which ids are accepted or offered. New keys: managed-by-model label, not-yet-known label with refresh hint. The popover keeps taking props; only its parent's data source changes (plan, Frontend design).

## R11. Testing approach

- Rust: sibling `*_tests.rs` files and `tests/` (constitution: no inline test modules). New:
  `model_capabilities_tests.rs`, `storage/models_tests.rs`; extended: `anthropic_tests.rs`
  (wiremock, already used there), `request_tests.rs`, `cli_delegate/mod_tests.rs`,
  `chat/commands_tests.rs`, `models/commands_tests.rs`.
- Frontend: new cases appended to `scripts/check-chat-state.ts` (the established mechanism). The
  file is already an exception (985 lines, header documents the split plan); extracting the
  harness first is a separate mechanical refactor and is deliberately not bundled here (Complexity
  Tracking).
- Verification commands (CI parity): `cargo fmt --check`, `cargo test`, `pnpm lint:rust`,
  `pnpm check:chat-state`, `pnpm check:templates`, `pnpm typecheck`, `pnpm typecheck:scripts`,
  `pnpm lint`, `pnpm format:check`.
