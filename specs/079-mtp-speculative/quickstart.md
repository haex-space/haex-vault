# Quickstart: Faster local answers with multi-token prediction

## Unit tests (no model)

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib -- catalog models:: llm::llamacpp
```

They cover:

- the drafter pins in the catalog;
- the placement rules with and without a drafter (research R7);
- the `drafter/` folder in listing, deleting and the startup cleanup;
- the combined progress totals.

## Speed and equality with the real files (reference desktop)

The CUDA build follows the recipe of spec 078's quickstart. Put the model and its drafter in one folder: the
model file and `mtp-gemma-4-E4B-it.gguf` from `unsloth/gemma-4-E4B-it-GGUF` at the revision in the catalog.

```bash
HOLZI_TEST_GGUF=<models>/gemma-4-E4B-it-Q4_K_M.gguf \
HOLZI_LLAMACPP_DRAFTER=<models>/mtp-gemma-4-E4B-it.gguf \
  cargo test --manifest-path src-tauri/Cargo.toml --features llm-llamacpp-cuda \
  --test model_tool_eval -- --ignored --nocapture mtp_speed
```

The test generates the four requests of research R1 with and without the drafter. It prints tokens per second for
both and fails if:

- the gain is below 1.4× (SC-001);
- a greedy answer names different tool calls (SC-003).

`measure_everyday_voice` of spec 078 with `HOLZI_LLAMACPP_DRAFTER` set records tokens per second in its
report, and the everyday figures can be compared (SC-002).

## In the app

1. Settings → Local models → download Gemma 4 E4B. The size shown includes the drafter (about 100 MB more).
2. The details of the installed model say "Speed-up: on".
3. Chat with it. The answer comes noticeably faster than with the drafter folder removed by hand, and the details
   then say "Speed-up available (98 MB)" with a download button.
4. Delete the model. Neither `models/<slug>/gemma-4-E4B-it-Q4_K_M.gguf` nor `models/<slug>/drafter/` remains.
5. Start a download and cancel it during the drafter's part and during the model's part. No `*.staging` or
   `*.part` file remains, and nothing is installed.
