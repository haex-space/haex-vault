# Implementation Plan: Second local inference engine (llama.cpp)

**Branch**: `070-d-plan` (plan), implementation on topic branches per PR | **Date**: 2026-10-30 |
**Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/070-llama-cpp-engine/spec.md`; the spike results in
[research.md](research.md) (PRs #654 and #666) answer US1 and most of US2 and settle the direction.

## Summary

llama.cpp joins mistralrs as a second in-process engine behind the existing `ProviderAdapter` trait. The
chat, the agents and the model jobs keep talking to an adapter; they do not learn which engine sits
behind it.

- **Engine choice from the file**: `general.architecture` (already read by `llm/local/gguf_header.rs`)
  picks the engine in one function. mistralrs keeps every architecture it knows (spec edge case: "mistralrs
  wins while it works today"), llama.cpp takes the rest. Flipping the default later (US5) is one line.
- **`LlamaCppAdapter`** in a new module `src-tauri/src/llm/llamacpp/`, grown out of the spike's prototype
  adapter. It owns what the binding does not give us: the model's own Jinja chat
  template (rendered with `minijinja`), splitting thinking from the answer, and reading tool calls out of the
  text.
- **Chat formats instead of model names**: a small `ChatFormat` per call syntax (Hermes JSON, Qwen3.5 XML,
  Gemma 4), picked by a fingerprint of the file's own template, never by the model's name
  (`contracts/tool-call-formats.md`). A template with tools that matches no format is refused with a clear
  message; that is the same decision the spec 032 template probe makes today.
- **Streaming and cancelling** are real, unlike the spike: text is emitted token by token, held back only
  where a call marker may begin, and a per-generation flag stops the loop within one token (FR in spec US3
  scenario 4).
- **Build stays opt-in** (FR-005): features `llm-llamacpp`, `llm-llamacpp-cuda`, `llm-llamacpp-metal`; the
  default build, `--no-default-features` and CI legs are unchanged except for one new leg.
- **Catalog gate** (FR-007): every catalog entry records its `architecture`; a unit test fails when an entry
  names an architecture no compiled engine loads. Qwen3.5 returns to the catalog in the PR that makes that
  test pass for it; Gemma 4 follows its own parser.

New direct dependencies: `llama-cpp-2` (pinned `=0.1.160`), `minijinja` and `minijinja-contrib` (both
already in the tree through mistralrs). Nothing else.

## Technical Context

**Language/Version**: Rust 2021 (MSRV 1.95, `src-tauri`). No frontend work is expected: the existing UI
reads the catalog.

**Primary Dependencies**:

- **Existing**: `mistralrs` 0.8.1 (stays), `async-trait`, `tokio`, `tokio-util` (`CancellationToken`),
  `serde_json`; the adapter types in `adapters/types.rs` (`ChatRequest`, `StreamChunk`, `ToolCall`,
  `ToolSpec`, `ToolTemplateProbe`); the header reader `llm/local/gguf_header.rs` (PR #640); the evaluation
  `chat/eval` and `tests/model_tool_eval.rs` (spec 032).
- **New**: `llama-cpp-2 =0.1.160` (bundles llama.cpp; needs `cmake` at build time), `minijinja` 2 with
  `builtins` and `json`, `minijinja-contrib` 2 with `pycompat`.

**Storage**: none. Catalog gets one field, `architecture`, in `model_catalog.json` (no database change).

**Testing**:

- **Rust**: `*_tests.rs` next to every module. Pure parts need no model: template rendering against
  real template texts kept as fixtures, the three parsers, the holdback buffer, engine choice, the catalog
  gate.
- **With a model** (ignored, env `HOLZI_TEST_GGUF` like `tests/local_inference.rs`): load, answer, cancel,
  tool call; the 83-sentence evaluation per catalog model (`HOLZI_EVAL_ENGINE=llamacpp`).
- **Android**: the emulator e2e (spec 043) loads Qwen3 0.6B through the new engine (T040).

**Target Platform**: Linux, Windows, macOS, Android; iOS where mistralrs runs today (FR-006). A platform
where the engine does not build keeps mistralrs and the clear error; the gap is filed as a bug.

**Project Type**: desktop and mobile app (Tauri), Rust backend.

**Performance Goals**: from the spike on an RTX A2000 (8 GB): Qwen3 4B 62.5 t/s, Qwen3.5 4B 51.7 t/s, load
under 5 s (SC-003 holds with room to spare). No regression against mistralrs on the 83-sentence evaluation
for the same model (SC-002; spike: 55 against 52).

**Constraints**: a crash in llama.cpp takes the process down (no Rust panic to catch), so memory must be
checked before the load (T016); one generation at a time per model (T017); no new network access.

**Scale/Scope**: three call formats, one new module (~1,500 lines with tests), one catalog field, three
Cargo features, one CI leg.

## Constitution Check

_GATE: passes before Phase 0; re-checked after Phase 1 below._

- **ADR for a design decision that affects a principle** (`.spaex/constitution.md`): a second engine, the
  choice of engine by architecture and the CUDA linking decision are such decisions. T001 writes the ADR
  (next free number under `docs/adr/`) in the first implementation PR and T008 amends it after R1.
- **Every device and OS** (project rule): FR-006 and T035 to T040; nothing in the new code assumes one OS's
  paths. The ggml CPU kernels must not be built for the build machine's CPU only (R3), or a release would
  crash on older CPUs.
- **Conventional Commits, no squash, topic branch via PR, no agent attribution**: followed by the PR plan
  below.
- **No key material, no OS-specific paths committed**: template fixtures are public model files without
  secrets; the spike's local paths stay out.
- **Tests with every behaviour change**: each task group carries its tests; the evaluation is the
  acceptance test for the format code.
- **One implementation per rule** (project rule): the tool-call template probe, the reasoning rules and
  the catalog gate are written once and used for both engines; the architecture check from #640 is reused,
  not copied.

Re-check after design: no conflict.

## Project Structure

### Documentation (this feature)

```text
specs/070-llama-cpp-engine/
├── spec.md
├── plan.md              # this file
├── research.md          # spike results (merged) + the open research items R1–R4 below
├── quickstart.md        # how to check it
├── contracts/
│   └── tool-call-formats.md   # normative: the three call syntaxes and what a format must provide
└── tasks.md
```

### Source code

```text
src-tauri/src/llm/
├── engine.rs                 # NEW  Engine enum, `engine_for(architecture)`, `load_local(...) -> Arc<dyn ProviderAdapter>`
├── local/                    # mistralrs path, unchanged apart from reusing gguf_header
└── llamacpp/                 # NEW  behind feature `llm-llamacpp`
    ├── mod.rs
    ├── model.rs              # backend singleton, load
    ├── memory.rs             # placement plan: GPU layers and context from the header and the free memory
    ├── generate.rs           # token loop, per-generation cancel flag, streaming
    ├── template.rs           # minijinja environment: filters, functions, message shaping
    ├── formats/              # one file per ChatFormat
    │   ├── mod.rs            # ChatFormat trait, fingerprint, holdback buffer
    │   ├── hermes.rs         # Qwen3: <tool_call>{json}</tool_call>
    │   ├── qwen_xml.rs       # Qwen3.5: <tool_call><function=…><parameter=…>
    │   └── gemma4.rs         # <|tool_call>call:name{…}<tool_call|>
    ├── adapter.rs            # LlamaCppAdapter: ProviderAdapter, probe_tool_template
    └── *_tests.rs, fixtures/ # template texts of the supported models
src-tauri/src/adapters/local.rs      # unchanged: wraps the mistralrs LocalModel
src-tauri/src/chat/model_loading.rs  # calls `llm::engine::load_local` instead of `LocalModel::load`
src-tauri/src/catalog/               # `architecture` field + the gate test
```

**Structure Decision**: a sibling module to `llm/local`, not a rewrite of it. Both engines end in
`Arc<dyn ProviderAdapter>`; `llm::engine` is the only place that knows both.

## Design decisions

**D1 – Engine choice.** `engine_for(architecture)`: if `mistralrs::core::GGUFArchitecture::from_value`
accepts it, mistralrs; else, if `llm-llamacpp` is compiled, llama.cpp; else `UnsupportedArchitecture` (PR
#640). One function, one test table. The rule "mistralrs wins" is kept until US5 is decided (T044).

**D2 – Template rendering.** The file's own template through `minijinja` with the same context variables
mistralrs passes (`messages`, `add_generation_prompt`, `bos_token`, `eos_token`, `tools`,
`enable_thinking`), plus `tojson` (accepting `indent`/`ensure_ascii`), `raise_exception`,
`pycompat`. The template rewrites a supported template needs are ported with a test each (`[::-1]` to
`|reverse` for Qwen3 and Qwen3.5), written down in the code with the template that needs them; the others
mistralrs applies (descending `range`, GLM4, SmolLM3 markup) are left out until a supported template does.
`strftime_now` is not provided: none of the three fixtures calls it, and an unknown function is a clear
render error. A template that fails to render is an error with the template error text, never a silent
fallback to ChatML (the spike showed the binding's own function falls back silently).

**D3 – Formats.** The trait gives each format: `fingerprint(template) -> bool`, `shape_messages`
(Gemma 4 merges tool results into the assistant turn), `marker()` (the string a call starts with, for the
holdback), `parse(text, tools) -> Parsed { reasoning, content, calls }`. Detection order: Gemma 4, Qwen XML,
Hermes. Parameter values in the XML and Gemma formats are typed by the tool's JSON schema (string stays
string; others try JSON), as in the spike.

**D4 – Streaming.** Text goes out per token; from the first character of a call marker on, output is held
back until the call block closes, then emitted as `ToolCalls`. This replaces `drain_reasoning_leak_buffer`
for the new engine only; the mistralrs path keeps its own (its bug is mistralrs'). Thinking text goes to
`reasoning`. The `Done` frame carries prompt and completion tokens and the finish reason (`stop`,
`length`, `tool_calls`) like the mistralrs path.

**D5 – Cancelling and concurrency.** Generation runs on a blocking thread; a `CancellationToken` is checked
after every decoded token. One generation at a time per model (a `tokio::sync::Mutex`); a second request
waits, as the model jobs already wait for the chat. A new context per generation, sized from the request
(prompt plus `max_new_tokens`), freed at the end; prompt-prefix reuse is a later optimisation (non-goal).

**D6 – Memory.** Before a load, `llamacpp/memory.rs` plans the placement and refuses with a message instead
of letting ggml abort. Its inputs are the file size, the layer and KV-head counts from the GGUF header and
the free memory of the best GPU (llama.cpp's device list) and of the host (the OS). All layers go on the
GPU when file, KV cache and a margin fit; the context is halved twice before layers move to the host; then
as many layers stay on the GPU as fit, and the context shrinks only when the host cannot take the rest
either. The context is the catalog `context_window`, capped by what the model was trained for, default 8192.
llama.cpp's own fit routine is not used (R2).

**D7 – Probe and reasoning.** `probe_tool_template` renders one fixed message with and without a dummy tool
through the real template and compares, exactly as `llm/local/probe.rs` does (`decide` is shared, not
copied). The chat command's reasoning rule (`model_id` containing `qwen3` with tools turns thinking off)
also matches `qwen3.5-…` ids; this is kept on purpose, since the evaluation ran that way, and a test pins it.

**D8 – Features and build.** `llm-llamacpp` pulls `llama-cpp-2` with `default-features = false` plus the
platform feature: `android-static-stdcxx` on Android, `metal` on Apple, `cuda` where asked. `openmp` was on
for desktop until US5 made the engine a default feature; it is off since, so the installers carry no OpenMP
runtime library (`vcomp140.dll`, `libgomp.so.1`). See R1 to R3 for the three things the spike left open.

## Research items (to settle before the PR that needs them)

- **R1 – CUDA linking.** The Nix CUDA toolkit has no `lib64` and no static cuBLAS, so the sys crate's default
  static link fails; the spike used `dynamic-link` plus `CUDA_LIBRARY_PATH` and a `lib64` link farm.
  Options: (a) a devshell with the static cuBLAS outputs of nixpkgs' CUDA packages, (b) `dynamic-link` and
  ship the shared llama.cpp libraries next to the binary, (c) `dynamic-backends`. Recommend trying (a) first
  because it keeps one binary; the result decides T004. Decided in the ADR.
- **R2 – GPU layer fit.** Whether `llama_rs_fit_params` gives a safe default across 4 to 24 GB cards.
  Settled in `research.md`: not used, the plan estimates from the GGUF header.
- **R3 – CPU kernels.** The sys crate builds ggml without AVX2/FMA unless `target-cpu=native`, which would
  make a release binary slow, or crash on older CPUs if native is used. Options: `dynamic-backends` with
  runtime CPU dispatch, or a fixed `x86-64-v3` baseline. Needs a measurement on CPU with a quiet machine.
- **R4 – Gemma 4 call syntax.** The spike saw one example. The parser follows the model's chat template
  (read from the file) and the llama.cpp source, not a guess; edge cases (nested objects, arrays, escapes)
  are collected into the contract before T030.

## Stages (PR cut)

Each PR is reviewed and merged before the next starts on `main`.

| PR   | Tasks           | Content                                                                           |
| ---- | --------------- | --------------------------------------------------------------------------------- |
| plan | T002            | this plan, tasks, quickstart and contract                                         |
| A    | T001, T003–T008 | ADR, build: devshell cmake, features, CI leg, CUDA decision applied, module shell |
| B    | T009–T015       | template rendering and the Hermes and Qwen XML formats, fixtures, unit tests      |
| C    | T016–T023       | `LlamaCppAdapter`: load, memory check, streaming, cancelling, probe, model test   |
| D    | T024–T029       | engine choice in the loader, catalog `architecture` and the gate, Qwen3.5 returns |
| E    | T030–T034       | Gemma 4 format and evaluation, catalog tool-use figures                           |
| F    | T035–T041       | Android, iOS, CPU kernels (R3), documentation                                     |
| G    | T042–T045       | the one-or-two-engines decision (US5) and wrap-up                                 |

## Risks

- **Parser drift**: each model family may change its call syntax with a new release. Mitigation: fixtures of
  real templates, the evaluation as the acceptance test, and the unknown-format refusal.
- **A process crash from ggml** (out of memory, bad file). Mitigation: the memory check, the header check,
  and no promise that every GGUF loads.
- **Two engines cost build time and binary size.** Mitigation: opt-in feature, one CI leg, and the US5
  decision with measured numbers.
- **Binding lag**: `llama-cpp-2` pins a llama.cpp commit; a new architecture needs a new crate release.
  Mitigation: the pin is exact and bumped deliberately, with the evaluation re-run.
