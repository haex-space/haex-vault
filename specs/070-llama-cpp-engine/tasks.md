---
description: 'Task list for spec 070-llama-cpp-engine'
---

# Tasks: Second local inference engine (llama.cpp)

**Input**: Design documents from `/specs/070-llama-cpp-engine/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md),
[contracts/tool-call-formats.md](./contracts/tool-call-formats.md), [quickstart.md](./quickstart.md)

**Tests**: Included. The constitution requires tests with every behaviour change.

- **Rust**: tests go into a `*_tests.rs` file next to the module, never inline. Pure parts (templates,
  parsers, holdback, engine choice, the catalog gate) need no model.
- **With a model**: ignored tests gated on `HOLZI_TEST_GGUF` (like `tests/local_inference.rs`) and the
  83-sentence evaluation (`HOLZI_EVAL_ENGINE=llamacpp`, spec 032). They run on the development machine, not
  in CI.
- **Always build in a targeted way** (`-j 4`, only the affected target; `holzi-cargo-in-worktrees`).

**Organization**: Grouped by PR (plan.md "Stages"), in the order a PR needs them. The user stories map to
stages: US1 and US2 are answered by the spike and made real in B and C; US3 is D (and E for Gemma 4); US4 is
F; US5 is G.

**Shipping note** (each PR is reviewed and merged before the next starts on `main`):

| PR   | Tasks           | Content                                                                           |
| ---- | --------------- | --------------------------------------------------------------------------------- |
| plan | T002            | this plan, tasks, quickstart and contract                                         |
| A    | T001, T003–T008 | ADR, build: devshell cmake, features, CI leg, CUDA decision applied, module shell |
| B    | T009–T015       | template rendering and the Hermes and Qwen XML formats, fixtures, unit tests      |
| C    | T016–T023       | `LlamaCppAdapter`: load, memory check, streaming, cancelling, probe, model tests  |
| D    | T024–T029       | engine choice in the loader, catalog `architecture` and the gate, Qwen3.5 returns |
| E    | T030–T034       | Gemma 4 format and evaluation, catalog tool-use figures                           |
| F    | T035–T041       | Android, iOS, Windows and macOS, CPU kernels (R3), documentation                  |
| G    | T042–T045       | the one-or-two-engines decision (US5) and wrap-up                                 |

`[P]` marks tasks that touch different files and can run side by side.

## Planning docs (PR plan)

- [x] T001 Write the ADR `docs/adr/0023-second-local-inference-engine.md`: why a second
      engine, the choice by architecture, mistralrs wins while it works, the CUDA linking decision once R1 is
      settled (amended in T008).
- [x] T002 This plan, the tasks, the quickstart and `contracts/tool-call-formats.md`.

## PR A – Build and shell (FR-005)

- [ ] T003 Add `cmake` to the devshell, and what R1 found the static CUDA link needs
      (`research.md` "R1"): `cudaPackages.libcublas.static` and a `CUDA_LIBRARY_PATH` directory with a
      `lib64` of `libcudart_static.a`, `libculibos.a`, `libcublas_static.a`, `libcublasLt_static.a` and
      `stubs`. `.devshell/packages.nix` is delivered by the atoms package, so the change goes into the atoms
      repo (as the Android devShell did) and holzi bumps the pin; record both PR numbers here.
- [x] T004 [P] Settle R1 (CUDA linking): static cuBLAS from the nixpkgs CUDA packages works (option (a));
      the finding is in `research.md`.
- [x] T005 `src-tauri/Cargo.toml`: promote the spike features to `llm-llamacpp`, `llm-llamacpp-cuda` and
      `llm-llamacpp-metal` (platform features for Android in T035); keep `llama-cpp-2 =0.1.160`; drop the
      `[profile.dev.package.llama-cpp-sys-2]` override if the CI leg does not need it; fix the stale comment
      about "mistralrs 0.8.1 still latest" (upstream has v0.9.5 on GitHub, none on crates.io since April).
- [x] T006 [P] `.github/workflows/ci.yml`: one Rust leg `llamacpp` (`--features llm-llamacpp`: clippy with
      `-D warnings` and the unit tests). Measure its time; the default legs must not change (SC-005).
- [x] T007 Create `src-tauri/src/llm/llamacpp/` (the shell and a link test; the submodules arrive with the stages that need them); move the spike's
      `llamacpp_spike_adapter.rs` content out of `llm/local/` into place as it is promoted in B and C, and
      remove the spike test files once their content lives in proper tests (end of C).
- [x] T008 Amend the ADR with the settled R1 decision.

## PR B – Template and formats (US2, FR-002)

- [x] T009 `llamacpp/template.rs`: the `minijinja` environment (`pycompat`, lstrip/trim blocks, `tojson`
      with `indent` and `ensure_ascii`, `raise_exception`) and the render function with the context
      variables of plan D2. `strftime_now` is not provided because none of the supported templates needs it;
      an unknown function is a render error. A render error is never a fallback to ChatML.
      Tests in `template_tests.rs`.
- [x] T010 [P] `llamacpp/fixtures/`: the real chat template text of Qwen3 4B, Qwen3.5 4B and Gemma 4 E4B
      (read from the GGUF `tokenizer.chat_template`; public model files, no secrets).
- [x] T011 Port the template rewrites a supported template needs, each with a test and a comment naming
      the template: `[::-1]` (Qwen3 and Qwen3.5). mistralrs also rewrites descending `range` calls and
      GLM4 and SmolLM3 markup; no supported template contains them, so they are left out until one does.
- [x] T012 `llamacpp/formats/mod.rs`: the `ChatFormat` trait, the fingerprint order, the refusal for
      tools plus unknown format, and the holdback buffer (text out per token, held from the first character
      of a marker). Tests: marker split across chunks, marker never completed, plain text.
- [x] T013 [P] `formats/hermes.rs` per the contract, with the case table in `hermes_tests.rs`.
- [x] T014 [P] `formats/qwen_xml.rs` per the contract (typing by schema, trimming, cut-off block), with
      `qwen_xml_tests.rs`.
- [x] T015 Message shaping round trip: holzi's flat history (assistant text, `ToolCall` rows, `ToolResult`
      rows) renders through each fixture template without error and shows the call and the result. mistralrs'
      own renderer is not public, so parity is shown by the evaluation instead: the 83 sentences through
      the new modules give the same 55 (Qwen3) and 60 (Qwen3.5) as the spike's own code.

## PR C – The adapter (US2, US3)

- [x] T016 `llamacpp/model.rs` and `memory.rs`: the backend singleton (`LlamaBackend::init` once per
      process), load from a path, the memory plan before the load (file size plus a KV estimate from the GGUF
      header against the free memory of the best GPU and of the host; refuse with a message), and the context
      size rule (catalog `context_window` capped by what the model was trained for and by what fits, default
      8192). The free memory comes from llama.cpp's own device list and the OS, not from `hardware`, whose
      VRAM figure is the card's total.
- [x] T017 `llamacpp/generate.rs`: the token loop on a blocking thread, greedy for
      `Sampling::Deterministic` and the default sampler otherwise, the cancel check after every token, and a
      per-model `tokio::sync::Mutex` so one generation runs at a time.
- [x] T018 Streaming: per-token `StreamChunk::Delta`, `reasoning` split, held-back call text, `ToolCalls`
      at the end, `Done` with token counts and the finish reason of the contract.
- [x] T019 `llamacpp/adapter.rs`: `LlamaCppAdapter` implementing `ProviderAdapter` (`list_models` empty,
      `stream_chat`, `probe_tool_template`).
- [x] T020 `probe_tool_template`: render one fixed message with and without a dummy tool and decide through
      the shared `decide` of `llm/local/probe.rs` (made reachable, not copied). Tested with the three
      fixtures and with a template that ignores tools.
- [x] T021 Error mapping: template error, memory refusal, ggml failure and cancel map onto `StreamError`
      with the right `is_transient`.
- [x] T022 Ignored model tests `tests/llamacpp_inference.rs`: load, answer, cancel mid-stream (ends within
      one second, US3 scenario 4), a tool call.
- [x] T023 Settle R2 (see `research.md`: the placement is estimated from the GGUF header instead of through
      `llama_rs_fit_params`) and apply it in T016. The spike adapter and its test files are gone; the
      evaluation harness and `tests/llamacpp_inference.rs` use `LlamaCppAdapter`.

## PR D – Engine choice and the catalog (US3, FR-003, FR-004, FR-007)

- [x] T024 `llm/engine.rs`: `Engine`, `engine_for(architecture)` (plan D1) and `load_local` (path, tokenizer repo and context in, an `Arc<dyn ProviderAdapter>` out). Table test: every architecture mistralrs knows maps to
      mistralrs; `qwen35` maps to llama.cpp when compiled and to `UnsupportedArchitecture` otherwise.
- [x] T025 Switch the call sites to `llm::engine::load_local`. There is exactly one production call site,
      `load_local_model_from_metadata` in `chat/model_loading.rs`, which the chat load, `load_local_session`
      and the model jobs all go through (grepped: no other `LocalModel::load` outside tests). The tests that
      build a `LocalAdapter` directly keep doing so on purpose.
- [x] T026 Catalog: add `architecture` to every entry of `model_catalog.json`. A build offers only the
      entries a compiled engine loads (`entries()` filters, FR-007), the gate test
      `catalog_entries_name_a_loadable_architecture` pins that, and the profile models must load with
      mistralrs so that a build without llama.cpp keeps a working default. The CI leg `llamacpp` runs the
      `catalog::` tests too.
- [x] T027 Re-add Qwen3.5 4B to the catalog (the entry removed in #640) and let the gate test pass for it.
      The desktop profile stays Qwen3 4B until T044.
- [x] T028 Run the 83-sentence evaluation for Qwen3.5 through the real adapter, write the figures into
      `specs/032-*/eval-results.md` and set `tool_use` in the catalog if the measured result supports it.
- [x] T029 Pin the reasoning rule (plan D7): a test that `qwen3.5-…` ids turn thinking off when tools are
      present, with a comment saying it is deliberate.

## PR E – Gemma 4 (US3)

- [x] T030 Settle R4: read Gemma 4's call syntax from the template in the file and from llama.cpp, and
      write every open case into the contract with an example.
- [x] T031 `formats/gemma4.rs`: `fingerprint`, `shape_messages` (tool results merged into the preceding
      assistant turn), `marker`, `parse`.
- [x] T032 `gemma4_tests.rs`: the case table of the contract and a render test against the fixture.
- [x] T033 Run the evaluation for Gemma 4 E4B; record the figures.
- [x] T034 Re-add Gemma 4 to the catalog if T033 supports it, and decide whether it becomes the desktop
      profile (it pointed there before #640 without a measurement; the answer is a measured one now).
      Result: 53 of 83 (Qwen3.5 4B: 60); Gemma 4 stays out of the catalog and is not the profile.

## PR F – Platforms and CPU (US4, FR-006)

- [ ] T035 Android: platform feature set (`android-static-stdcxx`, no `openmp`), the NDK environment for
      the cross-build (`CC_<target>`, `AR_<target>`, linker; the whole-crate check failed in `aws-lc-sys`
      without them), the build of `aarch64-linux-android` in CI's Android job.
- [ ] T036 Phone profiles: Qwen3 1.7B and 0.6B through llama.cpp on a phone; the same memory limits and
      indicators as with mistralrs. Decide whether phones use llama.cpp at all (they load nothing new).
- [ ] T037 iOS: build on a Mac with Metal; `llama-cpp-sys-2`'s `build.rs` has no iOS branch, so patch it
      upstream or in a fork. If it cannot be done, iOS keeps mistralrs and the gap is filed (FR-006).
- [ ] T038 Settle R3 (CPU kernels): measure CPU speed on a quiet machine with and without AVX2, then choose
      runtime dispatch (`dynamic-backends`) or a fixed baseline; apply it to every desktop build.
- [ ] T039 Windows and macOS: the build and a load of Qwen3 0.6B. A CI leg if one exists for them.
- [ ] T040 Android emulator e2e (spec 043 rig): load Qwen3 0.6B through the new engine and answer one
      prompt; the scenario goes into `scripts/e2e/scenarios/`.
- [ ] T041 Documentation: `docs/` page on engines, which models each runs, how to build with the
      features; the quickstart is checked on a clean checkout.

## PR G – Decision and wrap-up (US5)

- [ ] T042 Re-run the 83-sentence evaluation and the speed measurement for every catalog model through
      both engines where both load; table in `research.md`.
- [ ] T043 Decide US5: llama.cpp replaces mistralrs, or both stay. Amend the ADR with the numbers.
- [ ] T044 Apply the decision: flip `engine_for` (one line) or leave it, and set the desktop profile.
- [ ] T045 Wrap-up: remove leftovers (spike paths, the dev-profile override if still there), update
      memory notes and the spec status to Implemented.

## Dependencies

- A before everything (features and the build). B before C. C before D. D before E and F. G last.
- T004 gates T005's CUDA feature; T023 gates the final form of T016; T030 gates T031; T038 gates every
  release build with the engine; T043 needs T042.
- Inside B: T013 and T014 are independent after T012. Inside F: T035 to T039 are independent per platform.
