# Feature Specification: Second local inference engine (llama.cpp)

**Feature Branch**: `070-llama-cpp-engine`

**Created**: 2026-10-29

**Status**: Draft (sketch, spike first)

**Input**: User description: "Are there alternatives to mistral.rs that would make sense here?" – after
Qwen3.5 4B failed to load as GGUF (operator, 2026-10-29)

## Background

- holzi loads local models only through mistralrs (`llm/local/loader.rs`, GGUF, CPU/CUDA/Metal).
- Qwen3.5 (`general.architecture = qwen35`) and Gemma 4 (`gemma4`) cannot be loaded this way. In its GGUF
  path mistralrs knows only Llama, Qwen2, Qwen3, Qwen3MoE, Phi, Mistral3 and a few older architectures.
- Upstream this is open and not in sight: issue #2125 (qwen35) open since May, its finished fix PR #2129
  without any review; issue #2098 (gemma4) without a reply; PR #2425 (error instead of panic) open.
  New models reach mistralrs through safetensors and dedicated loaders, not through GGUF. State:
  2026-10-29, v0.9.5 on GitHub.
- Until then holzi refuses unknown architectures with a clear message (PR #640) and does not list
  Qwen3.5 and Gemma 4 in the catalog.

## Idea

llama.cpp (Rust binding `llama-cpp-2`) as a second engine behind the existing adapter. It reads the same
GGUF files and usually supports new architectures within days. mistralrs stays for everything that runs
on it today (chat, Qwen3-VL, model jobs).

Not part of this spec: an `llama-server` sidecar (one binary per platform, impractical on a phone),
Ollama or LM Studio (already usable today as an external API provider), a switch to safetensors with
in-situ quantisation.

## User Scenarios & Testing _(mandatory)_

### User Story 1 – Spike: Qwen3.5 loads and answers (Priority: P1)

A throwaway test (like `multimodal_spike_tests.rs`, ignored, manual only) loads the existing
`Qwen_Qwen3.5-4B-Q4_K_M.gguf` through `llama-cpp-2` and generates an answer.

**Why this priority**: It decides whether the path holds before anything is built.

**Independent Test**: The spike runs on the development machine (CUDA, 8 GB) and on CPU and reports load
time, tokens per second, peak VRAM and peak RAM, each compared with Qwen3 4B through mistralrs.

**Acceptance Scenarios**:

1. **Given** the Qwen3.5 GGUF, **When** the spike loads it, **Then** a coherent answer comes back.
2. **Given** the same hardware, **When** Qwen3 4B runs through both engines, **Then** tokens per second
   and memory are in the same order of magnitude.
3. **Given** a build with mistralrs and `llama-cpp-2` together, **When** CUDA is active, **Then** both
   build and run in the same process (candle Whisper, ONNX speech recognition and llama.cpp do not
   interfere).
4. **Given** the Android and iOS targets, **When** `llama-cpp-sys-2` is cross-built for
   `aarch64-linux-android` and `aarch64-apple-ios`, **Then** both builds succeed. If one fails, that
   platform is reported as a blocker before any further work (see US4).

---

### User Story 2 – Tool calls and thinking mode (Priority: P1)

The spike shows that a tool call in the model output arrives cleanly as a `ToolCall` and that thinking
mode (`reasoning_requested`) is emitted separately from the answer text.

**Why this priority**: Chat and agents (specs 032, 021) depend on tool calls. Without them the engine is
worthless for holzi. mistralrs delivers this ready-made today; llama.cpp provides chat template and
parser only in part through the Rust binding.

**Independent Test**: The 30 sentences of the tool evaluation (spec 032, `eval-results.md`) run through
the new engine with Qwen3 4B; the result is not below the one measured through mistralrs.

**Acceptance Scenarios**:

1. **Given** a prompt with tools, **When** the model produces a call, **Then** it appears as a `ToolCall`
   with valid JSON, and no `<tool_call>` text ends up in the answer text.
2. **Given** `reasoning_requested = false`, **When** Qwen3.5 answers, **Then** no thinking text appears.
3. **Given** the chat-template probe (spec 032 R8), **When** it runs with the new engine, **Then** it
   reaches the same decision as through mistralrs.

---

### User Story 3 – Engine behind the adapter, catalog re-enables models (Priority: P2)

After a successful spike there is an `LlamaCppAdapter` next to `LocalAdapter`. The loader picks the
engine by `general.architecture`: known architectures keep running through mistralrs, the rest through
llama.cpp when its support is built in. Qwen3.5 (and Gemma 4, if the spike supports it) return to the
catalog.

**Independent Test**: Load Qwen3.5, hold a conversation with a tool call, cancel in the middle of a stream.

**Acceptance Scenarios**:

1. **Given** an installed Qwen3 4B, **When** it is loaded, **Then** it runs unchanged through mistralrs.
2. **Given** an installed Qwen3.5, **When** it is loaded, **Then** it runs through llama.cpp.
3. **Given** an architecture no engine knows, **When** it is loaded, **Then** the clear error from PR
   #640 appears, never a hanging loading indicator.
4. **Given** a running stream, **When** the user cancels, **Then** generation ends within one second (as
   today through `AbortHandle`).

---

### User Story 4 – Android and iOS (Priority: P2)

The engine builds and runs on the phone (ADR 0010), at least for the two phone profiles. Android comes
first, iOS follows.

**Why this priority**: FR-006 requires every platform that offers local models today. llama.cpp supports
both (Android via the NDK on CPU, iOS with Metal), but the Rust binding `llama-cpp-2` is untested for us:
its `build.rs` has an Android branch and only a generic Apple branch (no iOS-specific one), and its issue
tracker holds no iOS reports either way. iOS is therefore the larger risk.

**Independent Test**: Android build in the devshell, emulator run with Qwen3 0.6B, then a check on a real
phone. For iOS, a cross-build and, on a Mac, a run on a device or simulator.

**Acceptance Scenarios**:

1. **Given** the Android build, **When** a model is loaded, **Then** the same memory limits and
   indicators apply as with mistralrs.
2. **Given** the iOS build, **When** a model is loaded, **Then** it runs through Metal or, if that fails,
   on CPU, with the same memory limits and indicators.
3. **Given** a platform where the engine does not build or run, **When** a model needs it, **Then** that
   platform keeps using mistralrs for the models it knows and shows the clear error from PR #640 for the
   rest; the gap is recorded as a bug (FR-006).

### User Story 5 – One engine or two (Priority: P3)

The measurements lead to a decision on whether llama.cpp eventually replaces mistralrs entirely.

**Why this priority**: Two engines cost build time, binary size and maintenance. The answer depends on
what the spike shows for tool calls, Qwen3-VL (spec 055) and speed. Until then: coexist.

## Edge Cases

- The same file is known to both mistralrs and llama.cpp: which one wins? Proposal: mistralrs, as long as
  it works today.
- A GGUF with a quantisation type one engine does not know (see mistralrs issue #2100, IQ4_XS): clear
  error message, no crash.
- Two engines in memory at the same time (chat and model job): shared memory budget from spec 055.
- A crash in the C++ code takes the process down; unlike a Rust panic it cannot be caught.

## Requirements _(mandatory)_

- **FR-001**: The spike MUST show that Qwen3.5 loads, answers and delivers tool calls before any
  production work (US1, US2).
- **FR-002**: The new engine MUST run behind the same adapter types (`AdapterStream`, `ToolCall`,
  `StreamChunk`) as `LocalAdapter`; chat code and frontend do not change.
- **FR-003**: The choice of engine MUST follow from `general.architecture`, not from a list in the catalog.
- **FR-004**: An architecture that no engine loads MUST fail immediately with a clear message.
- **FR-005**: The build MUST stay possible without llama.cpp (its own feature, like `llm-cuda`), so
  `--no-default-features` builds and CI do not get slower.
- **FR-006**: The engine MUST run on every platform where holzi offers local models (Linux, Windows,
  macOS, Android; iOS where mistralrs runs today). A gap is a bug, not a decision. Android is built
  first, iOS second.
- **FR-007**: The catalog MUST list a model only once it demonstrably loads and answers through the
  engine (checked in a test, not by hand).

### Key Entities

- **Engine**: mistralrs or llama.cpp; determined by the architecture of the file, never chosen by the user.
- **LlamaCppAdapter**: counterpart to `LocalAdapter`, same interface upward.

## Success Criteria _(mandatory)_

- **SC-001**: Qwen3.5 4B loads and answers on the development machine through llama.cpp.
- **SC-002**: The 30 sentences of the tool evaluation reach, with Qwen3 4B through the new engine, at
  least the value measured through mistralrs.
- **SC-003**: Tokens per second for Qwen3 4B are no more than 20 % below mistralrs. (Provisional
  threshold; the spike fixes it.)
- **SC-004**: The loading indicator never hangs on any file; each one fails after at most 60 seconds
  with a message.
- **SC-005**: The build time of the default build without the new feature does not change.
- **SC-006**: The Android emulator run with Qwen3 0.6B and the iOS cross-build both succeed, or the
  spike names the exact blocker per platform.

## Assumptions

- `llama-cpp-2` (0.1.160, last updated 2026-10-15) supports `qwen35` and `gemma4`. This is not
  verified; it is the first question of the spike.
- The Nix devshell has cmake and libclang (`haex-crdt` needs libclang anyway).
- The phone profiles (Qwen3 1.7B and 0.6B) are the only models a phone can hold. Qwen3.5 4B (3 GB) and
  Gemma 4 (5.3 GB) are desktop models, so on phones the new engine at first loads the same models as
  today; its gain is on the desktop.
- The catalog files stay GGUFs from bartowski and lmstudio-community; download and integrity check do
  not change.

## Open Questions

- [NEEDS CLARIFICATION: Chat template and tool-call parsing – build our own (Jinja via `minijinja`, one
  parser per model family) or use llama.cpp's own function, if the Rust binding exposes it?]
- [NEEDS CLARIFICATION: After the spike, which goes into the catalog first: Qwen3.5, Gemma 4 or both?
  Gemma 4 also has open bugs through mistralrs and at 5.3 GB is much larger.]
- [NEEDS CLARIFICATION: How does llama.cpp CUDA behave together with candle CUDA in the same process
  (Whisper, Qwen3-VL through mistralrs)?]
- [NEEDS CLARIFICATION: Does `llama-cpp-sys-2` need an iOS-specific branch in its `build.rs` (it only
  distinguishes watchOS and other Apple targets), and do we patch it upstream or in a fork?]
- [NEEDS CLARIFICATION: A mistralrs fork with PR #2129 instead of a second engine? Less code, but we
  would maintain a foreign fork without maintainer review.]
