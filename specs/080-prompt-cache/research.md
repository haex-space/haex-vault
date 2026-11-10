# Research: Reuse the prompt a local model has already read

Facts were read from holzi's code on `main` at the start of this branch and from the binding `llama-cpp-2` 0.1.160
and its llama.cpp (`ggml-org/llama.cpp@d81235049384534c167caea52b85a694f6103d14`), the version holzi ships.

## R1 What is measured, and what is still open

- **Decision**: the baseline is spec 079 research R11 (reading 4153 prompt tokens: Gemma 4 E2B 1.46 s, E4B 2.51 s
  on the reference laptop). Two points are settled by a spike with the real files before the engine changes
  (plan, delivery step 1):
  1. A kept context of Gemma 4, with its sliding-window cache kept whole (R5), gives the same answer after a
     rewind as a fresh context.
  2. Qwen3.5 4B restores a checkpoint (R6) and then gives the same answer as a fresh context; the spike records
     the size of one checkpoint.
- **Rationale**: both are correctness questions that unit tests without a model cannot answer, and both decide
  how much of R5 and R6 is built.

## R2 Where the kept reading lives

- **Decision**: `LoadedModel` owns the model and one kept context together, built with `self_cell`: the model is
  the owner, a `Mutex<KeptContext>` the dependent. The context is created at load with the placement's context
  length (`Placement.context`) and a batch of 512, and lives until the model is dropped.
- **Rationale**:
  - A `LlamaContext<'a>` borrows its `LlamaModel`, so a struct holding both refers to itself. `self_cell` makes
    that sound without `unsafe` in holzi.
  - `self_cell` 1.3.0 is already in `Cargo.lock` as a transitive dependency; declaring it adds no new crate to
    the build.
  - `LlamaContext` is `Send` (the binding implements it), so the mutex makes it shareable. The existing `gate`
    already lets only one generation run per model; the mutex is taken inside that gate.
- **Alternatives considered**:
  - A worker thread per model that owns model and context. Rejected: it replaces the adapter's
    `spawn_blocking` path and the gate with a channel protocol, a larger change for the same result.
  - Casting the context's lifetime to `'static` with a careful field order. Rejected: hand-written `unsafe` for
    a problem a small, widely used crate solves.
  - A context per generation (today). Rejected by the spec.

## R3 What is kept, and how a prompt is matched

- **Decision**: the kept context remembers the tokens that are in its cache: the prompt tokens read and every
  generated token that was decoded. Before an answer, the new prompt's tokens are compared with them; the reused
  part is the longest common beginning, but at most all tokens but the last, because the last prompt token must
  be decoded to get the logits of the first answer token.
- **Rationale**:
  - Token comparison is exact, so nothing that does not match can be reused (FR-004).
  - The sampled token that ends an answer is never decoded, so it is never in the cache; tokens are recorded only
    when they are decoded.
- **Alternatives considered**: comparing the prompt text. Rejected: the same text can tokenize differently at the
  edge of the match.

## R4 Rewinding an attention cache

- **Decision**: to reuse `n` tokens, the cache from position `n` on is removed (`clear_kv_cache_seq(Some(0),
Some(n), None)`) and the tokens from `n` on are read. If the removal returns `false` or fails, the whole cache is
  cleared and the prompt is read from the start (FR-005).
- **Rationale**: spec 079's spike showed the removal working on Gemma 4 for a tail; for any other start, R5 keeps
  the cache whole so the same call is valid.

## R5 Gemma 4's sliding-window cache

- **Decision**: the kept context is created with `with_swa_full(true)` for every model. A model without
  sliding-window layers is unaffected.
- **Rationale**:
  - A sliding-window cache keeps only the last window of tokens (512 or 1024 for Gemma 4). After a rewind further
    back than one window, the tokens the next one must see are gone; llama.cpp's server either re-reads the whole
    prompt then or keeps checkpoints. With the cache kept whole, a rewind to any point is valid.
  - The memory planner already counts every layer as a full-length attention layer (spec 070 research), so the
    whole cache fits the plan as it stands (FR-008). The spike (R1) checks the cache size against the plan.
- **Alternatives considered**: checkpoints for Gemma 4 too. Rejected: a second rewind path for memory the plan
  already reserves.

## R6 Qwen3.5's hybrid cache: checkpoints

- **Decision**: for a model that is hybrid or recurrent (`LlamaModel::is_hybrid()` or `is_recurrent()`), the kept
  context saves checkpoints of the part that cannot be cut back, with `state_seq_get(0,
LlamaStateSeqFlags::PARTIAL_ONLY)`:
  - **When**: at the end of every prompt before the first answer token, and after every 1024 tokens while a prompt
    is read.
  - **How many**: at most 4 of those; the oldest goes first. A checkpoint beyond the reused point is dropped.
  - **The head**: one more checkpoint at the end of the head, the part before the first user message (system
    prompt and tool descriptions), saved when the head is read and outside the oldest-first rule; it is replaced
    only when the head changes. Its position is the longest common beginning of the prompt's tokens and the tokens
    of the rendered prompt up to the first user message's text. Without it, a long tool chain pushes all four
    checkpoints past the head, and switching to another conversation or a changed tool offer reads everything
    again (FR-006).
  - **Rewind**: the checkpoint with the most tokens that is not beyond the reused point is restored
    (`state_seq_set`), the attention cache from its position on is removed, and the tokens from there are read.
    Without such a checkpoint, the cache is cleared and the prompt read from the start.
- **Rationale**:
  - The recurrent layers hold one running state per sequence; removing tokens from it is impossible, so
    `clear_kv_cache_seq` cannot rewind it. The binding documents `PARTIAL_ONLY` state for exactly this ("save just
    the running recurrent and SWA state, then restore it to effectively rewind the sequence").
  - llama.cpp's server does the same ("context checkpoints", default 32 per slot) for models whose cache cannot be
    cut back.
  - The end of a prompt is the point a tool chain returns to: the next step repeats the previous prompt and appends
    the answer and the tool result. The 1024-token checkpoints cover a follow-up question, whose prompt departs
    from the previous one inside the history (R8).
  - The end of the head is the point another conversation, a model job and a changed tool offer return to.
  - The checkpoints live in the computer's memory. Their size is measured by the spike; the planner counts five of
    them (four and the head) on the host side (FR-008).
- **Alternatives considered**: `with_n_rs_seq` (bounded rollback snapshots the binding offers for speculative
  decoding). Rejected: it rolls back a few tokens, not to a point thousands of tokens back.

## R7 Cancel, failure, reload and other users

- **Decision**:
  - **Cancel**: the tokens decoded until the cancel stay valid and stay kept.
  - **Failure**: any error while a prompt is read or an answer written clears the kept context (tokens, cache and
    checkpoints).
  - **Reload or another model**: the kept context belongs to the `LoadedModel` and ends with it.
  - **Other users**: chat, voice assistant and model jobs that share a `LoadedModel` take turns at the gate; each
    reuses only what matches its own prompt (R3).
  - **Model jobs**: a new `ChatRequest.reuse_prompt: bool` is `true` for the chat, the voice assistant and the
    evaluations, and `false` for a model job (`model_jobs/runner.rs`, spec 055). The llama.cpp adapter clears the
    kept context before it reads a request with `false` (FR-013); every other adapter ignores the field.
  - **Where it lives**: the kept context, its tokens and its checkpoints are memory of the process (main memory and
    graphics memory). Nothing is written to a file: no `state_save_file`, no session file. It ends with the
    `LoadedModel`, at the latest when the vault session ends the process (spec 013) (FR-012).
- **Rationale**:
  - FR-004 and FR-005; the exact comparison of R3 makes reuse across conversations safe.
  - A reused beginning answers sooner. An extension that could reuse what the chat left could learn from the
    timing of its job whether its prompt starts like the chat's last one. A model job sets its own system prompt
    (`step.instruction`), so that is not far-fetched; reading a job from the start closes it. Jobs gain little
    from reuse anyway, since every step has its own instruction.
  - Today a context lives for one generation; kept, the same content stays in memory while the model is loaded.
    The process ends with the vault (spec 013), so it never outlives the vault session.

## R8 The date and time in the prompt

- **Decision**: the line with the current date and time leaves the system prompt, for every provider. Instead,
  every user message starts with a stamp of the local date and time it was sent, from its `created_at` and the
  turn's UTC offset; the current message's stamp is the current time. The system prompt gets one fixed sentence
  saying that every user message starts with the date and time it was sent, so the last one is now.
  `chat::tools::prompt::system_prompt` no longer takes the clock; a new helper writes the stamp
  (`chat::tools::prompt::sent_line`, the format of today's `now_line`). The places that build a request's messages
  use it: `history_to_messages` in `chat/send.rs` and the everyday runner (`chat/eval/everyday/runner.rs`).
- **Rationale**:
  - The chat templates render the system prompt before the tool descriptions, so a line that changes every minute
    cuts the reuse off before the tools (spec FR-007).
  - A stamp that stays with its message keeps the prompt growing only at its end: the next turn's prompt is the
    previous one plus the answer and the new question. A line on the current message only would leave the
    previous message when it becomes history, so the next turn would differ from that message on, read the whole
    last turn again (with its tool results), and on a hybrid model often find no checkpoint before it (R6).
  - One placement for every provider keeps one prompt shape; hosted providers with their own prompt caching gain
    as well. This is the only change for them (spec FR-009 is amended accordingly).
  - The stamps tell the model when earlier messages were written, which a question like "what did I ask this
    morning?" can use.
- **Alternatives considered**:
  - The line on the current user message only (the first draft). Rejected: see the second point above.
  - A per-request field that each adapter places itself. Rejected: every adapter and every `ChatRequest` would
    change for one line.
  - The time to the hour. Rejected by FR-007 (to the minute).

## R9 The offered tools

- **Decision**: no change to the offer.
- **Rationale**: the core tools come first in every offer; the tools that match the message (`first_offer`) and the
  search results (`extend_offer`) are appended after them. A step that changes the offer is reused up to the first
  changed tool, which is the edge case the spec names.

## R10 Measuring

- **Decision**:
  - A new `StreamChunk::PromptCache { reused_tokens }`, sent once per answer before `Done` by the llama.cpp adapter
    only. A field on `Done` was rejected: every adapter builds `Done`, and two of them (`cli_delegate/claude.rs`,
    `cli_delegate/codex.rs`) are over the spaex 500-LoC boundary.
  - The everyday runner (spec 078) records per request the time to the first chunk of every step, whether that
    chunk was text or a tool call, and the reused tokens of every step. The first text chunk is FR-010's first
    word; a step that starts with a tool call sends its first chunk only when the whole call is parsed, so it
    measures reading and writing the call. The report shows the median of the later steps that start with text
    (SC-001, SC-002) apart from those that start with a tool call, and the share reused.
  - `LoadOptions.prompt_cache: bool` (default `true`); the evaluation reads `HOLZI_LLAMACPP_PROMPT_CACHE=off` to
    measure without reuse (FR-010).
  - A new ignored test measures User Stories 1 and 2 directly with the real files (quickstart).
- **Rationale**: SC-001 to SC-004 need the time per step and the comparison with reuse off.

## R11 Phones

- **Decision**: the same code on Android. The kept context and the checkpoints are counted by the same planner.
- **Rationale**: Android runs the llama.cpp engine on the processor, where reading is slowest; the spec's gain is
  largest there.

## R12 Spec 079

- **Decision**: none in this feature. When spec 079 resumes, its speculative loop must read and write through the
  kept context instead of creating its own. The kept context is created at load, so whether flash attention is
  off for the drafter (spec 079 research R8) is decided at load, too.

## R13 Spec 082

- **Decision**: none in this feature. Spec 082 (agent harness) adds a routing call before the first step of a chat
  turn: the same model, a short prompt of its own, at most 12 new tokens (spec 082 research R3). It would push the
  long chat prompt out of the single kept context. When this feature lands, the routing call of the llama.cpp
  engine runs on a second sequence of the kept context (sequence 1), so the chat's kept reading on sequence 0
  stays.
