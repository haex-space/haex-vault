# Quickstart: Reuse the prompt a local model has already read

## Unit tests (no model)

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib -- llm::llamacpp chat::tools::prompt chat::eval::everyday
```

In the 10 GB scope with `-j 1`; if the test build is killed for memory, add
`--config 'profile.dev.package.holzi.debug=0'`.

They cover the prefix match, the choice of checkpoint, the clearing after a failure, a model job that never
reuses, the time stamps of the user messages, and the new report fields.

## With the real files (reference desktop)

The CUDA build follows spec 078's quickstart. Model files: Gemma 4 E2B and E4B and Qwen3.5 4B, Q4_K_M, under their
Hugging Face file names (`gemma-4-E2B-it-Q4_K_M.gguf`, `gemma-4-E4B-it-Q4_K_M.gguf`,
`Qwen_Qwen3.5-4B-Q4_K_M.gguf`) in one folder outside the repository.

```bash
# The spike uses all three files of the folder and downloads the missing ones into it.
HOLZI_TEST_GGUF_DIR=<models> \
  cargo test --manifest-path src-tauri/Cargo.toml --features llm-llamacpp-cuda \
  --test prompt_cache_eval -- --ignored --nocapture prompt_cache_spike

# The other tests run one model at a time.
HOLZI_TEST_GGUF=<models>/<model>.gguf \
  cargo test --manifest-path src-tauri/Cargo.toml --features llm-llamacpp-cuda \
  --test prompt_cache_eval -- --ignored --nocapture --skip prompt_cache_spike
```

- `prompt_cache_spike`: a rewind gives the same greedy answer as a fresh context; prints the checkpoint size.
- `prompt_cache_chain`: a two-step tool chain with a 4000-token prompt; prints the time to the first token of the
  second step with and without reuse (SC-001).
- `prompt_cache_follow_up`: a follow-up question with a changed time line; prints the time to its first token with
  and without reuse (SC-002).

## The everyday set

```bash
HOLZI_EVAL_ENGINE=llamacpp HOLZI_TEST_GGUF=<models>/<model>.gguf \
  cargo test … --features llm-llamacpp-cuda,everyday-eval --test model_tool_eval -- --ignored measure_everyday_voice
HOLZI_LLAMACPP_PROMPT_CACHE=off …   # the same without reuse
```

Compare pass count, safety errors, total time and `later_chunk_ms` of the steps that start with text (SC-003,
SC-004).

## In the app

1. Load Qwen3.5 4B and ask "what did Anna write?". The answer after the mail search starts noticeably sooner
   than the search step.
2. Ask a follow-up two minutes later; it starts sooner than the first question.
3. Ask "how late is it?": the answer names the current time.
