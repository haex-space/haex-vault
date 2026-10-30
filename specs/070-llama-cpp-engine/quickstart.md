# Quickstart: Checking the llama.cpp engine

Shows that the engine holds up from start to finish. Prerequisites: the worktree of this spec, the nix
devshell with `cmake` (T003), and for the model checks a GGUF file per model (the catalog downloads them).

## Automated

Always build in a targeted way (`-j 4`, only the affected target), never the whole tree:

```sh
nix develop --command scripts/with-nix-host-bridge.sh cargo test -j 4 --manifest-path src-tauri/Cargo.toml \
  --features llm-llamacpp --lib -- llm::llamacpp llm::engine catalog::
pnpm lint:rust                       # Clippy with -D warnings, both feature sets and the llamacpp leg
```

These need no model: templates against the fixtures, the three parsers, the holdback buffer, the engine
choice table and the catalog gate.

## With a model (development machine, not CI)

Run memory-limited and with the holzi dev app closed, so the GPU is free:

```sh
HOLZI_TEST_GGUF=<file> HOLZI_SPIKE_GPU_LAYERS=99 systemd-run --user --scope -p MemoryMax=12G \
  nix develop --command scripts/with-nix-host-bridge.sh cargo test -j 4 --manifest-path src-tauri/Cargo.toml \
  --features llm-llamacpp-cuda --test llamacpp_inference -- --ignored --nocapture
```

Checks: the model loads (SC-001), answers, a running stream cancels within a second (US3), a tool call
arrives as a `ToolCall` and no call text reaches the answer (US2).

The 83-sentence evaluation through the new engine (SC-002), once per catalog model:

```sh
HOLZI_EVAL_ENGINE=llamacpp HOLZI_SPIKE_GPU_LAYERS=99 HOLZI_TEST_GGUF=<file> \
  nix develop --command scripts/with-nix-host-bridge.sh cargo test -j 4 --manifest-path src-tauri/Cargo.toml \
  --features llm-llamacpp-cuda --test model_tool_eval -- --ignored --nocapture
```

The report is written to `target/eval/<model>.json`. Pass criteria: for a model both engines load, the pass
count is not below the mistralrs run of the same day; for Qwen3.5 and Gemma 4 the figure goes into
`specs/032-*/eval-results.md`.

## Manual

1. **Qwen3.5 in the app (US3)**: start the app with the new feature, open the model list, download
   Qwen3.5 4B, select it, ask a question that needs a tool ("open the settings"). The loading indicator ends,
   the answer streams, the tool runs after the usual approval.
2. **Unknown architecture (FR-004)**: select a model whose architecture no compiled engine loads; the error
   text appears at once, the indicator does not hang.
3. **Build without the feature (FR-005)**: `cargo build --no-default-features` and a default build; their
   time and binary size do not change (SC-005).
4. **Android (US4)**: the emulator e2e of T040; on a real phone the two phone profiles load and answer with
   the same limits as before.
5. **iOS and other OS (FR-006)**: the builds of T037 and T039, or the filed gap.

## Cleaning up after measurements

Delete the build cache and model copies you made for the run (`target`, any `target-cuda`, downloaded
GGUFs outside the app's model folder); the CUDA build alone takes tens of gigabytes. Never put them under
`/tmp`, which is a RAM-backed volume on this machine.
