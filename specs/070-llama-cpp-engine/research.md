# Research: spike results (spec 070, US1/US2/US4)

**Date**: 2026-10-29 · **Branch**: `070-b-spike` · **Binding**: `llama-cpp-2` 0.1.160 (bundles llama.cpp)

The spike is `src-tauri/src/llm/local/llamacpp_spike_tests.rs` (ignored tests, feature `llm-llamacpp`,
CUDA through `llm-llamacpp-cuda`). Every number is one run, greedy decoding, 200 new tokens, the same
prompt, on the development machine: RTX A2000 8 GB laptop GPU, 24 threads, Linux.

## Verdict

llama.cpp through `llama-cpp-2` **loads Qwen3.5, answers coherently, is faster than mistralrs on Qwen3 4B
and needs less VRAM**. It builds for `aarch64-linux-android`. What it does not bring is a chat template
and tool-call parser; those would be ours to build. iOS could not be tested here.

## Results

| Engine / model               | Device | Load  | Generation | Prompt  | VRAM / RAM peak |
| ---------------------------- | ------ | ----- | ---------- | ------- | --------------- |
| llama.cpp, Qwen3 4B Q4_K_M   | GPU    | 3.5 s | 62.5 t/s   | 273 t/s | 2.5 GB / 2.8 GB |
| mistralrs, Qwen3 4B Q4_K_M   | GPU    | 7.3 s | 50.4 t/s   | 311 t/s | 4.6 GB / 1.3 GB |
| llama.cpp, Qwen3.5 4B Q4_K_M | GPU    | 4.3 s | 51.7 t/s   | 75 t/s  | 2.8 GB / 3.4 GB |
| llama.cpp, Qwen3.5 4B Q4_K_M | CPU    | 1.7 s | 2.0 t/s    | 4 t/s   | – / 3.1 GB      |

- mistralrs had a warm-up run before the timed one, llama.cpp did not. Its VRAM figure probably includes
  the paged-attention pool, so it is not like for like.
- The CPU row is **not representative**: the binding builds ggml without AVX2/FMA (`GGML_NATIVE=OFF`,
  its default unless `target-cpu=native`), and the machine had a load of 13 on 24 cores. SC-003 was
  checked on the GPU only.
- Qwen3.5 prompt processing is slow (75 t/s against 273 t/s for Qwen3) on a 23-token prompt; the 151-token
  tool prompt ran at several thousand t/s, so the first figure is probably start-up cost. Not investigated.
- Gemma 4: llama.cpp's `llama-arch.cpp` lists `gemma4`, but no Gemma 4 file was loaded.

## Tool calls (US2)

- With a Hermes-style prompt (JSON inside `<tool_call>`), both Qwen3 and Qwen3.5 produced a correct
  call as text, e.g. `{"name": "open_settings", "arguments": {"page": "sync"}}`. Reading it back out is
  string work, not an engine feature.
- **Qwen3.5's own template asks for another format**: XML, `<tool_call><function=name><parameter=key>value
</parameter></function></tool_call>` (read from the file's `tokenizer.chat_template`). The spike did not
  test that format, only that the model can call a tool.
- **The binding has no Jinja and no tool support.** It exposes `llama_chat_apply_template` only. For Qwen3.5
  it reported success but rendered plain ChatML, without the model's thinking handling and without tools.
  The `common` feature of the sys crate wraps grammar, fit and speculative decoding, not
  `common_chat_*`. Rendering the real template needs `minijinja`; parsing needs one parser per family.
- Not done: the 30-sentence evaluation of spec 032 (SC-002) and the chat-template probe.

## Build (US4 and FR-005)

- **cmake is missing from the Nix devshell.** The spike took it from `nixpkgs#cmake` outside the shell
  (`nix shell` inside `nix develop` fails on a glibc mismatch).
- **CUDA**: the Nix CUDA toolkit has no `lib64` and no static cuBLAS, so the crate's default static linking
  fails (`cudart_static` / `cublas_static` not found). The spike set `CUDA_LIBRARY_PATH` to a directory
  with a `lib64` link and used the `dynamic-link` feature, which needs the shared llama.cpp libraries on
  `LD_LIBRARY_PATH` at run time. `CUDAARCHS=86` limited the kernels to this card. A cold build took 15 min.
- **Android**: `llama-cpp-2` builds for `aarch64-linux-android` with NDK 28.2 in 1.5 min (isolated crate,
  NDK clang set through `CC_aarch64_linux_android`). It was not run on a device or emulator. The check
  through the whole holzi crate failed earlier, in `aws-lc-sys` (host gcc instead of the NDK compiler),
  which is unrelated to llama.cpp but shows the cross-build environment needs the same variables.
- **iOS**: not buildable on this Linux machine; the sys crate's `build.rs` has no iOS-specific branch.
- Two engines in one binary built and ran (mistralrs and llama.cpp in the CUDA test binary). Each test ran
  in its own process, so both in one process at once is not shown.

## Consequences for the spec

- US1 is answered yes; SC-001 and SC-003 hold on the development machine.
- US2 stays open: tool calls need our own template rendering and parsers (new open question in the spec).
- The devshell needs cmake, and the CUDA link needs a decision (static cuBLAS from a different CUDA
  package, or the shared libraries shipped next to the binary).
- Still to do before a plan: a Gemma 4 load, the 30-sentence evaluation, an Android run on the
  emulator, an iOS build on a Mac.
