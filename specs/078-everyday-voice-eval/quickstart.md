# Quickstart: Everyday voice agent evaluation

**Spec**: [spec.md](spec.md) | **Format**: [contracts/everyday-format.md](contracts/everyday-format.md)

Commands run from the repository root. `<models>` and `<scratch>` stand for directories of your choice.
The measurement is test code behind the cargo feature `everyday-eval`, so no app build compiles it; the
pure checks need no feature.

## Pure checks (no model)

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib chat::eval::everyday
```

They load the set, the world and the mail tools; check the twin links, the pinned manifest reference and
that every expected tool exists; and score planted traces (a `send_mail`, a `delete_messages`, a settings
change instead of a question) as safety errors with the verdict "not fit" (SC-006).

## A local model through llama.cpp

```bash
cargo test --manifest-path src-tauri/Cargo.toml --features llm-llamacpp-cuda,everyday-eval \
  --test model_tool_eval --no-run
HOLZI_EVAL_ENGINE=llamacpp HOLZI_LLAMACPP_GPU_LAYERS=999 HOLZI_EVAL_MACHINE="RTX A2000 8 GB laptop" \
HOLZI_TEST_GGUF=<models>/Qwen_Qwen3.5-4B-Q4_K_M.gguf \
  cargo test --manifest-path src-tauri/Cargo.toml --features llm-llamacpp-cuda,everyday-eval \
  --test model_tool_eval -- --ignored --nocapture measure_everyday_voice
```

`HOLZI_LLAMACPP_CONTEXT` sets the context in tokens when the model's default is too small for the offer
(the 9B model needs `8192`). The report lands in `src-tauri/target/eval/everyday-llamacpp-<file stem>.json`. Run it twice: apart from
`timing` both reports are the same (SC-003).

### Building the CUDA engine without the Nix bridge (Ubuntu or Pop!_OS)

The Nix shell sets this up; a machine with the distribution's CUDA 12 packages needs four things:

- The sherpa archive once per fresh checkout: `node scripts/fetch-sherpa-onnx.ts`.
- `CUDA_LIBRARY_PATH=<scratch>/cuda`, where `<scratch>/cuda/lib64` holds links to the distribution's
  `libcudart_static.a`, `libcublas_static.a`, `libcublasLt_static.a`, `libculibos.a` and `libcuda.so`: the
  build script only looks under `<CUDA_LIBRARY_PATH>/lib64`.
- `CUDAARCHS=<compute capability, e.g. 86>` and `CUDAFLAGS=-Xcompiler=-fPIC`: without `-fPIC` the link of
  the library fails with `relocation R_X86_64_PC32`; without `CUDAARCHS` every architecture is compiled.
  CMake reads both only on the first configure; after a change delete `target/debug/build/llama-cpp-sys-2-*`
  and `target/debug/.fingerprint/llama-cpp-sys-2-*`.
- `CMAKE_BUILD_PARALLEL_LEVEL=2`: CMake otherwise starts nvcc on every core regardless of `cargo -j`, and a
  31 GB machine runs out of memory.
- The library's unit tests with `llm-llamacpp-cuda` need about 12 GB even at `cargo -j 1`; the test binary
  and clippy fit in 10 GB.

## Claude on the subscription (reference)

```bash
mkdir -p <scratch>/claude-eval
HOLZI_EVAL_PROVIDER=claude-cli HOLZI_EVAL_MODEL=claude-opus-5-5 HOLZI_EVAL_WORKDIR=<scratch>/claude-eval \
  cargo test --manifest-path src-tauri/Cargo.toml --features everyday-eval --test model_tool_eval -- \
  --ignored --nocapture measure_everyday_voice
```

Needs a logged-in `claude` on the `PATH`. Each step is one `claude -p` in the empty work directory with
Claude Code's own tools, MCP servers, hooks and the user's CLAUDE.md switched off. The report says
`deterministic: false`.

## A local server speaking the OpenAI chat format (reference)

```bash
HOLZI_EVAL_PROVIDER=openai HOLZI_EVAL_BASE_URL=http://127.0.0.1:11434/v1 HOLZI_EVAL_MODEL=qwen3.5:4b \
  cargo test --manifest-path src-tauri/Cargo.toml --features everyday-eval --test model_tool_eval -- \
  --ignored --nocapture measure_everyday_voice
```

`HOLZI_EVAL_API_KEY` is read from the environment when the server needs one; it never appears in the
report.

## Recording the verdict

Copy `total`, `perPhrasing`, `safetyErrors`, `timing`, `speechFit` and `phoneVerdict` of each run into
`specs/078-everyday-voice-eval/eval-results.md` with the date, the set version and the commit (SC-004).
Runs since spec 082 are recorded in `specs/082-agent-harness/results.md`; their report format, with
`executedUnasked` as the gate's count, is `specs/082-agent-harness/contracts/everyday-report.md`.
