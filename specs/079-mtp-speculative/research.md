# Research: Faster local answers with multi-token prediction

All figures were measured on 2026-11-04 on the reference laptop (RTX A2000 8 GB, 31 GB RAM, Pop!_OS).

## R1 The measurement

- **Decision**: Gemma 4 E2B, E4B and 12B get a drafter in the first delivery.
- **Measured**:
  - **Setup**: `llama-server` built with CUDA (architecture 86) from
    `ggml-org/llama.cpp@d81235049384534c167caea52b85a694f6103d14`. That is the submodule of
    `utilityai/llama-cpp-rs@0dd5814c9b061930965d144c1653cd4d05054873:llama-cpp-sys-2/llama.cpp`, so exactly the
    llama.cpp that `llama-cpp-sys-2` 0.1.160 ships. Its `src/` is byte-identical to the crate's copy.
  - **Requests**: four of 200 tokens each (a short spoken answer, a mail summary, a JSON tool call, a 120-word
    reply), greedy, context 8192.

  | Model                      | Without, flash attention on | Without, flash attention off | With drafter (FA off) | Factor | Accepted |
  | -------------------------- | --------------------------- | ---------------------------- | --------------------- | ------ | -------- |
  | Gemma 4 E2B                | 87.8 tok/s                  | 83.6                         | 126.5                 | 1.44×  | ~50 %    |
  | Gemma 4 E4B                | 49.9                        | 47.6                         | 81.3                  | 1.63×  | ~53 %    |
  | Gemma 4 12B (24/48 on GPU) | 10.5                        | 10.9                         | 17.4                  | 1.66×  | ~74 %    |

  The factor compares the drafter against the faster baseline, flash attention on.

- **Rationale**: the gain is well above the spec's 1.4× goal, on every size, including the 12B that sits half on
  the CPU.
- **Alternatives considered**: Qwen3.5 4B with bundled MTP tensors. It was not measured, and its hybrid cache needs
  a different rollback (R6), so it is left to a later delivery (spec FR-014).

## R2 The drafter files

- **Decision**: the catalog pins each drafter by repository, commit, file, size and SHA-256:

  | Model | Repository                    | Commit                                     | File                      | Bytes     | SHA-256                                                            |
  | ----- | ----------------------------- | ------------------------------------------ | ------------------------- | --------- | ------------------------------------------------------------------ |
  | E2B   | `unsloth/gemma-4-E2B-it-GGUF` | `0314792d7f1f7e229411f620751375812bb9faf2` | `mtp-gemma-4-E2B-it.gguf` | 97817664  | `9eba819938efccfd6044f8af84e3bbfddc639a2bcf32ebc36420e6a649191919` |
  | E4B   | `unsloth/gemma-4-E4B-it-GGUF` | `bfc15c382204943c3a8fff0c750b94ae2364d7a3` | `mtp-gemma-4-E4B-it.gguf` | 98653248  | `b6a723115efa510d3b3215db1e26790dae84cd08c2134a764f3d194f1f0c3376` |
  | 12B   | `unsloth/gemma-4-12b-it-GGUF` | `fc034cfff751157913579611efad8462ac1be606` | `mtp-gemma-4-12b-it.gguf` | 465109248 | `145db9094bc0f85f1701e255a2ed216dcc9800fc8bc8631ad00905b456bd451b` |

  The hashes come from the Hugging Face LFS metadata and match the files downloaded for the measurement.

- **Rationale**:
  - A pinned commit and hash make the drafter immutable (constitution IV in spirit).
  - The download can be checked against an expected hash, which the model download today cannot do (it records its
    hash and compares it on every load).
  - No new database column is needed: the expected hash lives in the catalog.
- **Alternatives considered**:
  - Resolving `main` at download time like the model. Rejected, because a drafter must match its model, and `main`
    can move.
  - A hash column in the synced `models` table. Rejected: a migration of a CRDT table for a per-device file.
- **Pairing with the model**: the drafter belongs to the base model of its catalog entry (for example Gemma 4 E4B
  it), not to one upload of its quantisation. The model file still resolves `main` at download, and the model's
  revision is not pinned:
  - a drafter works with any quantisation of its own model (spec Edge Cases);
  - the model's sampler decides every token (R6), so a pairing that drifts can only lower the share of accepted
    proposals, never change an answer;
  - the app never pairs a drafter with another model, because it takes the drafter from the model's own catalog
    entry.

  At load, two checks from the drafter's header refuse a drafter of another model:
  - its `general.architecture` is the model's with `-assistant` (`gemma4-assistant`);
  - its `embedding_length_out` equals the model's `embedding_length`: the drafter reads the model's embeddings
    (E2B 1536, E4B 2560, 12B 3840 in the three files), so the E4B drafter is refused for E2B.

  A drafter that passes both and still does not fit fails when its context is created or at the first draft,
  and the answer falls back (R6 step 5, spec FR-008).

## R3 Where the drafter lives

- **Decision**: `models/<slug>/drafter/<file>`, a subfolder of the model's folder.
- **Rationale**:
  - `paths::canonical_model_file` takes the one `*.gguf` file in `models/<slug>/` and refuses a second one. Loading,
    listing, deleting and the download pre-check all go through it.
  - It skips directories, so a subfolder is invisible to it.
  - Putting the drafter in the model's folder ties it to the model's lifecycle by location.
- **Alternatives considered**:
  - The same folder under a different extension. Rejected: it hides that the file is a GGUF and still lies next to
    the model file.
  - A separate `drafters/` root. Rejected: deleting and cleaning up would have to find it by catalog id.

## R4 Download, cancel, delete and cleanup

- **Decision**:
  - **Order**: `download_model_from_catalog` stages the drafter first, then the model, as one operation with one
    cancel token.
  - **Progress**: one `model-download-progress` stream whose `bytesTotal` is the model's total plus the drafter's
    pinned size. Bytes already done count on.
  - **Hash**: the drafter's SHA-256 is compared with the pin before it is published. On a mismatch it is discarded,
    the model download goes on, and the model is installed without the drafter (spec FR-006).
  - **Cancel**: both staging files are discarded; nothing is published.
  - **Publish**: the model first, then the drafter folder. A failed drafter publish leaves the model without a
    drafter.
  - **Delete**: `delete_installed_model` removes `drafter/` too.
  - **Startup cleanup**: `cleanup_staging_in_dir` also cleans `drafter/`.
  - **Pre-download check**: it adds the drafter's size to the model's.
- **Rationale**: the drafter is small. Staging it first means a cancel never leaves a model without its drafter
  half-way, and the model's existing publish and rollback logic stays as it is.
- **Alternatives considered**:
  - A separate download the user starts. Rejected by the spec (FR-004).
  - Two progress streams. Rejected: the UI keys progress by model id and shows one bar.

## R5 Installed state and the model list

- **Decision**:
  - **Derived from the file system**: whether the drafter is installed is read at list time: the catalog names a
    drafter for the model id, and `drafter/<file>` exists with the pinned size.
  - **Payload**: `InstalledModelPayload` gets `drafter: { installed: bool, sizeBytes }` for models whose catalog
    entry has one. Its `sizeBytes` includes an installed drafter.
  - **New command**: `download_model_drafter(modelId)` fetches the drafter of an installed model (spec US5), through
    the same cancel registry and progress events as a model download.
- **Rationale**: since migration 0012, the file system is the authority for "installed". The drafter follows that
  rule.

## R6 The generation loop with a drafter

- **Decision**: a new module `llm/llamacpp/speculative.rs` follows llama.cpp's own reference loop
  (`examples/speculative-simple`, `common_speculative_*`), through `llama_cpp_2::speculative::MtpSpeculative`.
  That module of the binding needs its `common` feature (plan, Primary Dependencies), so the loop exists on
  desktops only.
  1. **Contexts**: the target context as today, plus flash attention off (`with_flash_attention_policy`). The
     drafter context comes from the drafter model, with `LlamaContextType::Mtp`, the target's `n_ctx`, and the
     target as `ctx_other` (`new_context_with_ctx_other`, which Gemma 4's assistant needs).
  2. **Prompt**: decode all prompt tokens but the last on the target in batches of 512. Each batch goes through
     `MtpSpeculative::process`. Then call `begin(prompt_without_last)`.
  3. **Each round**:
     - `draft(n_past, id_last, prompt)` returns up to 4 tokens.
     - Decode `[id_last, draft…]` on the target with logits for every position, then `process(batch)`.
     - Sample position by position with the normal sampler, and stop at the first position where it disagrees with
       the draft. That position's sample is the next real token.
     - `accept(n_accepted)`; remove the target's and drafter's cache from `n_past`.
     - Emit the text of every accepted token through the same UTF-8 buffer as today.
  4. **Ending**: an end-of-generation token, the limit, a cancel and `on_text` returning false end the loop exactly
     as in `generate.rs`.
  5. **Failures**: any `MtpSpeculativeError`, or a failed decode of a draft batch, switches the rest of the answer to
     the plain loop on the same target context, after the cache is trimmed to `n_past` (spec FR-008). The reason is
     logged once.
     - **Flash attention**: the rest of that answer runs on the context created for the drafter, so with flash
       attention off. That answer is at most about 5 % slower (R1).
     - **Later answers**: the loaded model stops using the drafter until it is loaded again, so every later answer
       runs with today's settings (spec FR-012).
- **Rationale**:
  - **The decision stays with the model**: the sampler decides every token, and a draft token only saves a decode
    step when the sampler would have picked it anyway (spec FR-002).
  - **Rounding**: with greedy settings the answer can differ only where batched arithmetic rounds differently
    (spec SC-003).
- **Settled by the spike (`mtp_spike`, 2026-11-04)**: Gemma 4's sliding-window cache drops a rejected tail as it
  is; every partial removal succeeded without `with_swa_full`. The drafter context is created with
  `new_context_with_ctx_other`, and greedy answers matched the plain loop in three of four requests (the fourth
  parted after 207 characters, a rounding difference).
- **Changes against the reference loop**, both measured:
  - **First token**: the whole prompt is decoded and the first token sampled as in the plain loop, before the first
    draft. The reference drafts from the prompt's last token, which made the first token about 15 ms later.
  - **Drafter batch**: the drafter context gets a batch of 8 instead of 512. It shares the model's cache and never
    decodes the prompt; the batch of 512 cost about 1 GiB of compute buffers on the GPU.
- **Alternatives considered**:
  - The C++ server's loop through FFI. Rejected: the binding already wraps `common_speculative`.
  - Checkpoint-based rollback (`ckpt.update_tgt`). Not exposed by the binding; needed only for caches that cannot
    remove a tail, which is the Qwen3.5 case and out of scope.

## R7 The memory planner

- **Decision**: `ModelShape` gets an optional drafter, with its file size and its cache per token. `plan` returns a
  `Placement` with `drafter: bool`:
  1. If the model plus drafter plus both caches fit the GPU whole, both go on the GPU and the drafter is used.
  2. If the model alone fits whole but not with the drafter, it loads without the drafter (spec FR-009: the planner
     cannot tell which is faster).
  3. If the model fits only partly in any case, the drafter is used only where the catalog's drafter says
     `measured_partly_on_gpu`. Then the drafter goes on the GPU first, and the model's layers are planned in the
     remaining memory. Without the flag, the model loads without the drafter, as in rule 2.
     - **What R1 covered**: the 12B kept its 24 GPU layers in both runs.
     - **What the planner does**: it fills the GPU, so a drafter there costs the model some layers.
     - **The flag**: whether the 12B still gains with fewer layers is measured in the app's own placement (task
       T031), and the flag is set only if it does (spec FR-009).
  4. Without a GPU, no drafter. A processor-only gain is unmeasured (spec Assumptions, FR-009).
- **Rationale**: ggml aborts the process on a failed GPU allocation, so the drafter has to be in the plan before
  anything loads (spec FR-007). The rules keep every case the measurement covered and leave out every case it did
  not.

## R8 Flash attention

- **Decision**: flash attention is switched off on both contexts only while a drafter is used. Without one, the
  context keeps llama.cpp's default (spec FR-012).
- **After a failure**: a drafter that fails during an answer leaves the rest of that one answer with flash attention
  off (R6 step 5). Later answers run without the drafter and with the default.
- **Rationale**: the drafter's documentation states that it aborts in the CUDA flash-attention kernel. Without a
  drafter, flash attention was worth about 5 % (R1).
- **Measured 2026-11-04**: flash attention on for the model's context only, off for the drafter's, does not abort.
  Reading the prompt is then as fast as without a drafter, but writing takes twice as long (E2B 0.52×, E4B 0.56×):
  the drafter reads the model's cache. Both contexts keep flash attention off.

## R9 Measuring in the app's own code

- **Decision**: `tests/model_tool_eval.rs` reads `HOLZI_LLAMACPP_DRAFTER=<path>` and passes it through
  `LoadOptions`. Its report gets the generated tokens and the generation time per request, and the tokens per
  second over the run (spec FR-013).
- **Rationale**: SC-001 to SC-003 must hold for holzi's own loop, not only for `llama-server`.
- **Dependency**: the everyday measurement is spec 078 (#803, merged on 2026-11-04).

## R10 Phones

- **Decision**: the same code on Android. The planner's rule 4 (R7) keeps the drafter off where the model runs on
  the processor only, which is how holzi runs on Android today.
- **Rationale**: a phone gain is unmeasured, and the spec allows the drafter only where it is faster.

## R11 Measured in holzi's own loop, and why the feature is parked

- **Measured 2026-11-04** with `mtp_speed` (holzi's loader, planner and token loop, the four requests of R1 rendered
  through the model's chat template with thinking off, greedy, debug build; RTX A2000 8 GB):

  | Model | Writing, tok/s      | First token, short prompts | First token after 4153 prompt tokens | Tool call |
  | ----- | ------------------- | -------------------------- | ------------------------------------ | --------- |
  | E2B   | 80.7 → 96.9 (1.20×) | 75 → 57 ms                 | 1462 → 1716 ms (+17 %)               | same      |
  | E4B   | 45.4 → 63.3 (1.39×) | 98 → 82 ms                 | 2512 → 2810 ms (+12 %)               | same      |

- **Cross-check**: `llama-server` (R1's build) on the same four chat prompts with thinking off gives E4B
  46.1 → 62.4 tok/s, 1.35×: holzi's loop is as fast as the reference. R1's 1.44× to 1.66× came from answers with
  thinking, which the drafter predicts better; the app's short voice answers gain less.
- **The prompt**: holzi decodes the whole prompt for every answer and every tool step (one context per generation,
  spec 070 plan D5). With the drafter, that happens with flash attention off (R8), 12 to 17 % slower. For a chat
  prompt of about 4000 tokens and an answer of 40 tokens, E2B takes 2104 ms with the drafter against 1949 ms
  without, and E4B is even (3411 against 3377 ms). The drafter pays off only from about 110 answer tokens on E2B
  and 45 on E4B.
- **Decision (operator, 2026-11-04)**: the feature is parked. Keeping the context and its cache between the steps
  and answers of a conversation comes first, as its own spec: it speeds up every local model, and afterwards the
  drafter's slower prompt reading only applies to the new part of a prompt. Then MTP is measured again.
- **State of the code at parking**: tasks T001 to T016 (and the shared token writer of T026) are implemented and
  tested; the catalog pins, the `drafter/` folder, the planner rules and the speculative loop work. Downloading the
  drafter (US2), the fallback (US3), partial placement (US4) and the settings line (US5) are not built.
