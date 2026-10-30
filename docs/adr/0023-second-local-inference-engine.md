# A second local inference engine (llama.cpp), chosen by the file

Status: proposed
Date: 2026-10-30

## Context

holzi runs local models in-process through mistralrs (`llm/local`, GGUF, CPU, CUDA, Metal). Qwen3.5
(`general.architecture = qwen35`) and Gemma 4 (`gemma4`) cannot be loaded this way: mistralrs 0.8.1 knows
only Llama, Qwen2, Qwen3, Qwen3MoE, Phi, Mistral3 and a few older architectures in its GGUF path, and it
`unwrap()`s the lookup, so an unknown file used to hang the load (PR #640 now refuses it with a message).
Upstream support is not in sight: the fix PR for `qwen35` (#2129) has waited without a review since May, the
`gemma4` issue (#2098) has no reply, and new models reach mistralrs through safetensors and dedicated
loaders, not through GGUF.

The spike of spec 070 (PRs #654 and #666, `specs/070-llama-cpp-engine/research.md`) tried llama.cpp through
`llama-cpp-2` 0.1.160 on an RTX A2000 (8 GB):

- It loads Qwen3.5 and Gemma 4 and answers.
- On Qwen3 4B it is faster than mistralrs (62.5 against 50.4 t/s) and uses less VRAM (2.5 against 4.6 GB).
- With a prototype that renders the file's own Jinja template and reads tool calls out of the text, the
  83-sentence tool evaluation of spec 032 gives 55 of 83 against 52 for mistralrs with the same model, and
  60 of 83 for Qwen3.5.
- The binding offers no Jinja and no tool-call parser, and three model families use three call syntaxes.
- It builds for `aarch64-linux-android`; iOS and an emulator run are not yet shown.

## Decision

- **llama.cpp becomes a second engine behind the existing `ProviderAdapter`**, in a new module
  `src-tauri/src/llm/llamacpp/`, behind opt-in Cargo features (`llm-llamacpp`, `llm-llamacpp-cuda`,
  `llm-llamacpp-metal`). The default build and `--no-default-features` are unchanged.
- **The engine is chosen from the file**, by `general.architecture`, in one function (`llm::engine`).
  mistralrs keeps every architecture it knows; llama.cpp takes the rest; an architecture neither loads is
  refused at once with a clear message. Which engine wins when both know a file is a one-line default,
  decided later with measured numbers (spec 070 US5).
- **The tool-call format is chosen from the file's own chat template**, never from the model's name. Each
  supported syntax is a `ChatFormat` (contract: `specs/070-llama-cpp-engine/contracts/tool-call-formats.md`).
  A template with tools that matches no format is refused.
- **The catalog lists a model only if a compiled engine loads its architecture**: every entry records its
  `architecture`, and a unit test fails otherwise.
- **CUDA is linked statically** (settled by R1, 2026-10-30). The binary carries cudart and cuBLAS and needs
  only the NVIDIA driver (`libcuda.so.1`) at run time, so no extra libraries ship next to it. The Nix devShell
  provides what the sys crate looks for: static cuBLAS from `cudaPackages.libcublas.static` and a directory
  `$CUDA_LIBRARY_PATH/lib64` holding `libcudart_static.a`, `libculibos.a`, `libcublas_static.a`,
  `libcublasLt_static.a` and `stubs`. llama.cpp and mistralrs, each with its own CUDA runtime, run in one
  process (checked on one card, one OS; see `research.md`).
- **The CPU kernel strategy is not decided here.** ggml is built without AVX2 by default. Spec 070 R3 settles
  it and this ADR is amended with the outcome (task T038).

## Consequences

- holzi owns a chat-template renderer and one tool-call parser per model family. A new family needs a new
  format and fixtures, and a model release may change its syntax; the evaluation is the acceptance test.
- Two engines cost build time, binary size and maintenance. They stay opt-in, with one CI leg, until the
  one-or-two-engines decision.
- A crash inside llama.cpp takes the process down, so memory is checked before a load and no promise is made
  that every GGUF loads.
- The devshell needs `cmake`. `.devshell/packages.nix` is delivered by the atoms package, so the change goes
  there first.
- The crate pins a llama.cpp commit; a new architecture needs a new crate release. The pin is exact and bumped
  on purpose, with the evaluation re-run.
- `mistralrs` stays for everything it runs today (chat, Qwen3-VL, model jobs).

## Alternatives

- **Wait for mistralrs.** No upstream movement for months on the open fixes; Qwen3.5 would stay out.
- **Run mistralrs from a fork with PR #2129.** Less code, but a foreign fork to maintain, without maintainer
  review, and it would not help Gemma 4.
- **mistralrs with safetensors and in-situ quantisation.** Gigabyte downloads and long loads, desktop only,
  and Gemma 4 has open bugs there.
- **A `llama-server` sidecar, Ollama or LM Studio.** One binary per platform, impractical on a phone; the
  external servers already work as an API provider and stay an option for users who run them.
- **candle, ONNX Runtime or Burn directly.** No better GGUF coverage than mistralrs.
