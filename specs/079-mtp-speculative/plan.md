# Implementation Plan: Faster local answers with multi-token prediction

**Branch**: `079-mtp-speculative` | **Date**: 2026-11-04 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/079-mtp-speculative/spec.md`; the measurement of 2026-11-04 in
[research.md](research.md) R1.

## Summary

Gemma 4 E2B, E4B and 12B get their published MTP drafter.

- **Catalog**: the drafter is pinned by commit, size and hash.
- **Download**: it comes with the model download, lives in the model's own `drafter/` folder, and goes with the
  model when it is cancelled or deleted.
- **Load**: the memory planner counts it.
- **Generation**: a new speculative loop in the llama.cpp engine uses it through the binding's `MtpSpeculative`. The
  model's sampler decides every token, so answers stay the same and only come faster. Measured 1.44× to 1.66×.
- **Fallback**: without a drafter, or when it fails, the engine runs exactly as today.

## Technical Context

**Language/Version**: Rust (edition and MSRV of `src-tauri/Cargo.toml`); TypeScript/Vue for the settings line

**Primary Dependencies**: existing crates; one feature of an existing crate on desktops.

- `llama-cpp-2` =0.1.160 (`speculative::MtpSpeculative`, `LlamaContextType::Mtp`, `new_context_with_ctx_other`,
  `with_flash_attention_policy`, `with_swa_full`, `clear_kv_cache_seq`).
- `speculative::MtpSpeculative` sits behind the binding's `common` feature, which builds llama.cpp's libcommon.
  Spec 070 left it off (its research R2 on the memory plan). The operator chose on 2026-11-04 to switch it on
  for the desktop targets only (Linux, Windows, macOS): phones never use a drafter (research R10), so Android
  and iOS build without libcommon, and `llm/llamacpp/speculative.rs` is compiled for desktops only.
- `reqwest` through `models::download`.
- `sha2` through `models::hash`.

**Storage**: files under `<AppLocalData>/models/<slug>/drafter/`; no database change (data-model.md)

**Testing**:

- `cargo test --lib` for the catalog pins, the placement rules, folders, cleanup and progress totals.
- Ignored tests in `tests/model_tool_eval.rs` with the real files for speed and equality (quickstart.md).
- Frontend checks for the settings line.

**Target Platform**:

- Desktop (Linux, Windows, macOS) with llama.cpp.
- Android builds the same code, and the planner keeps the drafter off there (research R10).
- iOS has no llama.cpp; unchanged.

**Project Type**: desktop and mobile app (Tauri)

**Performance Goals**:

- At least 1.4× the tokens per second with the drafter on the reference desktop (SC-001).
- No longer time to the first word (SC-007).

**Constraints**:

- No GPU allocation may fail because of the drafter: ggml aborts the process (FR-007).
- Flash attention is off only while a drafter is used (FR-012).
- Builds stay within the 10 GB memory scope, with `-j 1` and `CMAKE_BUILD_PARALLEL_LEVEL=2`.

**Scale/Scope**:

- Three catalog entries, one new engine module, one new command.
- Changes to download, delete, list and cleanup, plus one settings line.

## Constitution Check

_GATE: Must pass before Phase 0 research. Re-check after Phase 1 design._

| Principle                                               | Check                                                                                                                                                                                                                                                | Result |
| ------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------ |
| I No secrets in git                                     | Only public repository names, commits and hashes                                                                                                                                                                                                     | Pass   |
| II No local absolute paths                              | Paths are resolved under the app's data folder at run time; docs use placeholders                                                                                                                                                                    | Pass   |
| III Device-independent identity                         | Not touched                                                                                                                                                                                                                                          | Pass   |
| IV Pinned cross-repo references                         | Drafters pinned by full commit SHA and hash (research R2); the llama.cpp version is cited by commit (research R1)                                                                                                                                    | Pass   |
| V External sources opt-in                               | No harness content                                                                                                                                                                                                                                   | Pass   |
| VI Self-modifying instructions review-gated             | No skill, prompt or permission changes; only `.specify/feature.json` points at this feature, reviewed in the PR                                                                                                                                      | Pass   |
| VII Relay never blocks local work                       | Not touched                                                                                                                                                                                                                                          | Pass   |
| VIII No concealment instructions                        | Not touched                                                                                                                                                                                                                                          | Pass   |
| Workflow: PR to `main`, Conventional Commits, no squash | One topic branch, one PR                                                                                                                                                                                                                             | Pass   |
| Phase sequencing                                        | Builds on Gemma 4 in the catalog (#806); the everyday comparison uses spec 078 (#803, merged; research R9)                                                                                                                                           | Pass   |
| spaex: 500 LoC per file                                 | `models/commands.rs` (960) and `chat/model_loading.rs` (822) are already over it; the drafter logic goes into a new `models/drafter.rs`, and both files only gain call sites. The split follows as its own change, planned under Complexity Tracking | Pass   |
| spaex: graphify first                                   | Before each new function, `graphify query` for an existing candidate (the download, staging and hash helpers of `models/` are the expected ones to reuse); `graphify update` first, since this worktree has no graph                                 | Pass   |
| spaex: tests in separate files, one runnable check each | `*_tests.rs` next to each module; ignored real-model tests in `tests/model_tool_eval.rs`                                                                                                                                                             | Pass   |
| spaex: ponytail comments                                | The placement rules of research R7 are a heuristic with a known ceiling (no per-device speed probe); marked with `ponytail:`                                                                                                                         | Pass   |

Checked against `.specify/memory/constitution.md` (v1.4.0) and the spaex harness `.spaex/constitution.md`. No ADR:
the engine choice of ADR 0023 is unchanged; the drafter is a load option of the same engine. Re-check after Phase 1:
unchanged.

## Project Structure

### Documentation (this feature)

```text
specs/079-mtp-speculative/
├── plan.md
├── research.md          # R1–R10
├── data-model.md
├── quickstart.md
├── contracts/
│   └── models-and-catalog.md
└── tasks.md             # $speckit-tasks
```

### Source Code (repository root)

```text
src-tauri/src/catalog/
├── mod.rs                    # CatalogDrafter; CatalogEntry.drafter
├── model_catalog.json        # drafters of the three Gemma 4 entries (research R2)
└── catalog_tests.rs          # pins are well formed

src-tauri/src/models/
├── drafter.rs                # new: drafter folder, state, staging, hash check, publish, delete, path for a load
├── paths.rs                  # drafter_dir(slug)
├── commands.rs               # call sites only: download with drafter, download_model_drafter,
│                             #   delete removes drafter/, list fills drafter state and size
├── download_check.rs         # size of model plus drafter
├── import.rs                 # cleanup_staging_in_dir also cleans drafter/
└── *_tests.rs

src-tauri/src/chat/model_loading.rs   # resolve the drafter path and pass it on
src-tauri/src/llm/engine.rs           # LoadRequest.drafter
src-tauri/src/llm/llamacpp/
├── model.rs                  # LoadOptions.drafter; LoadedModel keeps the drafter model; placement with drafter
├── memory.rs                 # ModelShape.drafter; Placement.drafter; rules of research R7
├── speculative.rs            # new: the loop of research R6
├── generate.rs               # chooses the speculative or the plain loop; plain loop unchanged
└── *_tests.rs

src-tauri/src/lib.rs                  # register download_model_drafter (vault-scoped, not on the gate's allow-list)
src-tauri/tests/model_tool_eval.rs    # HOLZI_LLAMACPP_DRAFTER, mtp_speed (ignored)

src/composables/useCatalog.ts, src/composables/useModels.ts   # drafter fields, downloadDrafterAsync
src/components/settings/InstalledModels.vue                   # speed-up line and button
src/components/chat/ModelSelection.vue, src/stores/models.ts                         # size with drafter
src/i18n/locales/{de,en}.json                                 # models.installed.drafter.*
```

**Structure Decision**:

- **Engine**: the drafter is a load option of the existing llama.cpp engine and a second file of an existing model.
  No new engine, provider or table.
- **Generation**: `generate.rs` keeps its plain loop untouched and hands over to `speculative.rs` when the
  placement says so.
- **Fallback**: the speculative loop falls back to the same plain code on the same context.

## Delivery order

1. **Spike**: an ignored test loads E2B with its drafter and runs `MtpSpeculative` for one greedy answer. It
   settles the open point of research R6 (partial cache removal on Gemma 4, `swa_full`), and checks that the drafter
   loads with `new_context_with_ctx_other`.
2. **Engine**: `memory.rs` rules, `speculative.rs`, `generate.rs` choice, fallback. Test with `mtp_speed`
   (SC-001, SC-003).
3. **Files**: catalog pins, `drafter/` folder, download with drafter, cancel, delete, cleanup, list, new command.
4. **UI**: sizes with drafter, settings line and button, i18n.
5. **Measure**: `mtp_speed` on the reference laptop for all three sizes; then the everyday comparison of spec 078
   (SC-002); record in `specs/079-mtp-speculative/results.md`.

## Complexity Tracking

No violations. Two files this feature touches are already over the spaex 500-LoC boundary. They only gain call sites
here; the split is a follow-up PR right after this feature, so the two changes can be reviewed apart:

| File                                  | Lines | Split                                                                                                                                                                                                                           | Remains                                            |
| ------------------------------------- | ----- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------- |
| `src-tauri/src/models/commands.rs`    | 960   | `models/hf_commands.rs`: the Hugging Face search, details, install preview, update check and update install commands with their payloads. `models/publish.rs`: `staging_path`, `publish_staged_file`, `rollback_published_file` | download, cancel, import, list and delete commands |
| `src-tauri/src/chat/model_loading.rs` | 822   | `chat/local_model_loading.rs`: `resolve_local_model_metadata`, `verify_local_model_integrity`, `load_local_model_from_metadata`, `load_local_session`, `local_load_phase`                                                       | the load orchestration and the API-key load        |
