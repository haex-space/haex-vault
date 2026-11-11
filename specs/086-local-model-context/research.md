# Research: Context for local models

What was read in the code on 2026-11-09 (main at `76a5f17b`, after PRs #903 and #904) and what is still to be measured. A figure
marked **to measure** is not known yet; the plan fixes no default that depends on it (R9).

## R1 What a load and a generation take today

- **Finding**: the context is decided in two places.
  - At the load, `llamacpp::model::placement_for` reads the header and `memory::plan` picks the layers on the GPU and
    `placement.context`, the longest context a generation may use (`wanted = max(catalog, 131072)`, capped by the file).
  - At every generation, `generate.rs` makes a new context of `prompt + answer` tokens (`new_context`), so the cache is
    allocated per request, not per load. `speculative.rs` makes the model's and the drafter's context the same way.
- **Consequence**: everything a setting changes in the context (cache type, rope scaling) must be applied in the two places
  that build `LlamaContextParams`, and the plan must count the same cache the generation allocates.

## R2 What the binding offers

- `LlamaContextParams::with_type_k`, `with_type_v` (`KvCacheType::{F16, Q8_0, Q4_0, …}`), `with_flash_attention_policy`,
  `with_rope_scaling_type(RopeScalingType::Yarn)`, `with_rope_freq_scale`, `with_yarn_orig_ctx`, `with_yarn_ext_factor`,
  `with_yarn_attn_factor`, `with_yarn_beta_fast`, `with_yarn_beta_slow` (llama-cpp-2 0.1.160, `context/params/get_set.rs`).
- llama.cpp refuses a quantised V cache without flash attention when it creates the context (an error, not an abort), and
  flash attention depends on the head sizes and the backend. Gemma 4 has heads of 256 and 512.
- **Decision**: a non-standard cache type switches flash attention on, and the load makes one small context with the chosen
  settings (512 tokens) before it reports success. A combination the engine cannot run then fails at the load with its own
  message, not at the first message of a chat. (Cheap: the cache of 512 tokens is a few MiB.)
- **Alternatives considered**: checking per architecture in a table. Rejected: the table would be wrong for the next model.

## R3 How a cache type changes the estimate

- **Decision**: `Kv::bytes` takes the cache type. Bytes per stored value (K or V): f16 2, 8-bit `34/32` (`Q8_0`: 32 values and a
  two-byte scale in 34 bytes), 4-bit `18/32` (`Q4_0`). The fixed state of a hybrid model (Qwen3.5 recurrent layers) is not
  quantised and stays as it is. The window layers are quantised like the others. Computed in 1/32 bytes to stay integral.
- **Effect** (Qwen3 4B at 131072: 18 GiB f16, 9.6 GiB 8-bit, 5.1 GiB 4-bit; Gemma 4 12B at 131072: 2.6 GiB f16, 1.4 GiB 8-bit).
- **Alternatives considered**: keeping `KV_BYTES_PER_VALUE = 4` and halving for 8-bit. Rejected: the scale bytes matter at the
  5 % level the planner's margins are set for.

## R4 Rope scaling beyond the training length

- **Decision**: a fixed context above the trained length, up to four times, loads with `RopeScalingType::Yarn`,
  `rope_freq_scale = trained / context`, `yarn_orig_ctx = trained`. The other YaRN values stay at the engine's defaults
  (`ext_factor` from the file or `-1`, `attn_factor 1`, `beta_fast 32`, `beta_slow 1`), which are llama.cpp's own recommended
  values for this method. A file that declares its own rope scaling (`<arch>.rope.scaling.*`) is read by the header reader;
  if it already declares YaRN, its type and factor win and the setting only raises the context.
- **Decision**: the scaling is applied to a generation only when that generation's own context (`prompt + answer`) is longer
  than the trained length. A request inside the training length then runs exactly as the model was trained, and only a long
  one is stretched. The load still plans memory for the chosen length.
- **Why**: YaRN changes attention for short text too; most chats are short even when the limit is 128k. The spec says the
  setting "says so" (FR-008); this keeps the cost to the requests that need it.
- **To measure** (R9): the answer quality of Qwen3 4B at 65536 and 131072 with YaRN, short and long.
- **Alternatives considered**: always scaling when the setting is above the training length. Simpler, but short chats get
  worse for no reason.

## R5 Fixed context versus Auto

- **Decision**: two modes in the planner.
  - **Auto** is what #904 does: aim at 131072, capped by the trained length and by memory, whole model on the GPU down to 32768
    before layers move to the host. Never YaRN. The cache type comes from the setting.
  - **Fixed(n)** never shrinks: whole model on the GPU at `n` if it fits; else as many layers as fit on the GPU and the rest on
    the host at `n`; else a refusal that says what is needed and what is free. `n` above four times the trained length is
    refused before any memory is looked at. `n` below 2048 is refused (the planner's floor, `MIN_CONTEXT`).
- **Consequence**: `plan` gets a `ContextChoice { Auto, Fixed(u32) }` and a `CacheType`; `placement` records whether YaRN is on.
  The settings page needs the same decision without loading, so the planner exposes `limits(shape, free, cache)`:
  the trained length, the largest context with every layer on the GPU, the largest at all (spec Key Entities, FR-006).

## R6 Where a setting is kept

- **Decision**: a device-scoped preference, `PrefScope::Device(device_uuid)`, key `models.context.<model id>`, value
  `{"context": "auto" | <tokens>, "cache": "standard" | "q8" | "q4"}`. Missing or unreadable means Auto and standard, as the
  DNS choice (`agent_browser::choices`) does. It is a vault row, so it follows the vault but belongs to one device
  (spec FR-005), and a second device has its own.
- **Rejected**: a column on `models`. That table is a synced catalog row shared by all devices, and a CRDT migration for a
  per-device choice (the same reasoning as spec 079 R2 for the drafter hash).
- The load reads it where the metadata is resolved today (`chat::model_loading::resolve_local_model_metadata`), next to the
  drafter, and passes it to the engine as part of `LoadRequest`. The catalog's `context_window` stays an input of Auto only as
  "known for", as in #904.

## R7 What the settings page shows, and how it learns the context in effect

- **Decision**: one read command `model_context_report(model_id, setting)` answers the trained length, the two allowed lengths for
  the given cache type, the reasons in words as codes (`trained`, `memory`, `choice`, `unsized`) and, when the model is the
  loaded one, what is in effect (context, layers on the GPU, cache type, YaRN). It reads the header and the free memory, so it
  works before anything is loaded and updates as the person changes a field. "In effect" comes from `LoadedModel::placement()`,
  which `LoadedModelInfo` carries (new field `placement`) so the page and the chat header read one source.
- **Why a command and not a number on the model row**: the free memory changes between loads (spec Edge Cases).

## R8 Reloading when a setting changes

- **Decision**: the setting is saved at once; if that model is the active one, the page loads it again through the existing
  `settings.models.activate`. The adapter is an `Arc`, so an answer that is being generated keeps the old model until it ends
  and the new one takes the next request. No new lock is needed. Nothing is reloaded for a model that is not loaded.
- The spec says the setting applies "at the next load (or reloaded by the choice)"; this is the second.

## R9 What is not measured, and how it gets measured

- **Not known**: peak memory and prompt speed of a 100k-token prompt on the operator's card; the quality cost of an 8-bit
  and a 4-bit cache; the quality cost of YaRN at 2× and 4×. The GPU cannot be read in the development session (NVML driver
  mismatch), so the figures come from a probe the operator runs on the real card.
- **Decision**: a probe in `src-tauri/examples/context_probe.rs` (`--features llm-llamacpp`, release) takes a GGUF, a cache
  type and a context and writes one JSON line: free device memory before and after the load, time to fill the context with a
  synthetic prompt, tokens per second at 4k and at the end of the context, and the result of a **needle test**: a short code
  word planted at 10 %, 50 % and 90 % of the prompt, asked for at the end, three runs each, greedy. The results go into this
  file as a table.
- **What the table decides**: whether the settings page names 8-bit as "recommended for long contexts" or only offers it, and
  the defaults shown for Qwen3. It decides nothing about Auto, which stays f16 and unscaled until a measurement says otherwise.
- **Alternatives considered**: shipping q8 as the default for all models on the strength of public reports. Rejected: the
  operator asked for settings to try ("Nutzer werden … verschiedene settings testen wollen"), and a default needs our own numbers.

### R9 Result (2026-11-11)

Measured on the operator's laptop: NVIDIA RTX A2000 8 GB (7772 MiB free), nothing else on the card, release build, greedy,
`context_probe` with `--context` set to the row's value. The prompt is filler text of about the context less 1024 tokens; the
needle prompts are 10 % shorter (see the probe). Qwen3 4B and Gemma 4 12B were not measured (operator 2026-11-11: Qwen3.5 is
the minimum; the 12B does not fit beside a running app and was left out for now). "Min free" is the least free device memory
seen during the fill run.

| Model       | Cache    | Context | GPU layers | Fill prompt | First word | Tokens/s at the end | Min free | Needle 10/50/90 %     |
| ----------- | -------- | ------: | ---------: | ----------: | ---------: | ------------------: | -------: | --------------------- |
| Qwen3.5 4B  | standard |  131072 |         32 |      127890 |      134 s |                20.5 |  592 MiB | found / found / found |
| Qwen3.5 4B  | 8-bit    |  131072 |         34 |      127890 |      133 s |                18.6 | 2128 MiB | found / found / found |
| Gemma 4 E4B | standard |  131072 |   34 of 43 |      127887 |      156 s |                 7.5 | 3214 MiB | found / found / found |
| Gemma 4 E4B | 8-bit    |  131072 |         43 |      127887 |      141 s |                13.3 | 3062 MiB | found / found / found |
| Qwen3.5 4B  | standard |   65536 |         34 |       63101 |       51 s |                30.3 | 2398 MiB | found / found / found |
| Qwen3.5 4B  | 8-bit    |   65536 |         34 |       63101 |       53 s |                24.7 | 3322 MiB | found / found / found |
| Gemma 4 E4B | standard |   65536 |         43 |       63098 |       51 s |                 6.5 | 3208 MiB | found / found / found |
| Gemma 4 E4B | 8-bit    |   65536 |         43 |       63098 |       52 s |                19.5 | 3690 MiB | found / found / found |

Reading:

- **128k works on an 8 GB card** for both models with the planner's choice (SC-001 holds on a smaller card than the one it
  was written for). Qwen3.5 4B in the standard cache ends with 592 MiB free, which is close to the edge.
- **8-bit is worth recommending when memory is tight.** It found every needle like the standard cache, it freed 1.5 GiB for
  Qwen3.5 4B, and it put all 43 layers of Gemma 4 E4B on the card at 131072 (34 in the standard cache), which made it faster
  there. At 32768 tokens (first pass, same card) it cost 2 to 12 % of the speed of the short prompt and gave nothing back,
  because the cache is small there.
- **Not shown**: the needle test is one word in filler text, so it says nothing about summaries, reasoning over the whole
  context or the 4-bit cache (not measured). The note for 4-bit stays the general one (FR-009).
- **Tokens per second** come from answers of 9 to 13 tokens and vary from run to run (Gemma 4 E4B standard at 65536: 6.5 against
  19.5 with 8-bit on the same layers); read them as an order of magnitude.
- **Auto**: no model in the table is handled badly by Auto (every row loaded with the layers the planner chose and answered),
  so T028 changes no default.
- **YaRN** was not measured: the models left in the table were trained for 262144 or more, so a stretch is not needed up to 131072.

## R10 Compaction: the unit that is summarised

- **Finding**: the request is built from the thread's rows (`send.rs`: `list_messages` → `history_to_messages`) and grows within
  a turn by the rounds of the open turn (`tool_round.rs`). The rows before the latest user message do not change during a turn.
- **Decision**: a summary covers **rows up to, not including, the latest user message**. So the person's latest message and the
  open tool round (FR-015) are never in it by construction, and compaction runs once at the start of a turn, before the first
  model call, not between rounds. Growth inside one turn is what #903 handles (old results left out, the latest cut).
- **Headroom**: "would not fit" is measured against the context minus a quarter, the share one tool result may take
  (`context_budget`), so a turn that starts at 99 % does not have to cut its first result to nothing. Spec Assumptions say so.
- **Order** (FR-012): the existing `fit` stages are split: (1) `leave_out_old_results`, (2) `cut_latest`. Compaction sits
  between: leave out, and if it still does not leave the headroom, use a stored summary; if none covers enough, write a new one;
  then cut. The summary is written from the rows themselves, not from the shortened request, so a tool result that was left out
  of the request still reaches the summary (clipped, see R11).

## R11 The summary itself

- **Decision**: one call of the conversation's own adapter, no tools, a fixed instruction in the language of the interface
  (spec 083 reply-language rule): keep the person's goals and instructions, decisions, names, numbers, URLs and file names that a
  later step needs, what was tried and what failed, and what is still open; short. Input: the earlier summary, if any, then the
  rows to cover as plain text with each tool result clipped to 1500 characters and an attachment named, not included.
- **Chunks**: the input must fit the context less the instruction and 1024 tokens of answer. A longer span is summarised in
  chunks, oldest first, the result of one going in as the "earlier summary" of the next, at most four calls per turn. If four
  calls do not cover the span, the summary covers what they covered.
- **Failure** (FR-017): no answer, an empty answer, an error or a cancel leaves no row, the request falls back to stage (2)
  `cut_latest`, and the person gets one notice for the turn.
- **Where it goes into the request**: as a trailing paragraph of the system prompt ("Earlier in this conversation: …"), not as
  a message, so chat templates that require alternating roles are not disturbed, and the rows that follow stay verbatim.
- **Size**: the summary is limited to 1200 tokens (chars/3 estimate). It is asked for in that size; a longer answer is cut at a
  paragraph end.

## R12 The row

- **Decision**: a new role `summary` (`MessageRole::Summary`), and **no new column and no migration**: a message's `parent_id`
  is the message before it in the thread (`send_admission` sets it so), and a summary row names the last row it covers as its
  parent. `content` is the summary. `created_at` of the row is the `created_at` of the covered row; `insert_message` already makes a
  child sort after its parent (`MAX(created_at, parent + 1)`), so the list (`ORDER BY created_at, id`) shows the summary directly
  after what it covers and not at the end. Nothing reads `parent_id` as a conversation path: the history is the flat list of the
  thread. The validator (`ChatMessageValidationError`) gets the rule that a summary row has a parent and no tool column.
- **Why not a column** (the first plan): it would be a CRDT migration, a new field in the nineteen places that build a
  `ChatMessage`, and it would say what `parent_id` already says.
- **Which row a request uses**: among rows of role `summary` of the thread, the one whose covered row is the latest in the list;
  on a tie the newest `created_at`, then the greater `id` (spec Edge Cases, two devices). Rows after the covered one are sent.
- **Every reader of `chat_messages` must be checked** (the "every call site" rule): `history_to_messages` (summary rows are
  not messages), `thread_commands` (role to string), the frontend `useChat.ts` role union and `MessageList.vue`, the chat search,
  the export if there is one, and the peer chat, which has its own tables. The tasks list the grep.
- **Alternatives considered**: a `System` row with a marker in `content`. Rejected: `System` rows are dropped from requests today
  and shown as assistant text; a marker in free text is what the data model avoids elsewhere.

## R13 Remote models and unknown limits

- `ProviderAdapter::context_limit()` is `None` by default and `Some(placement.context)` for llama.cpp (#903). Compaction runs
  only when it is `Some` (spec FR-018). Nothing changes for providers, delegates or MCP.

## R14 Guardrail kind

- **Finding**: `src/lib/actions/settingsActions.ts` marks an action a guardrail with `scope: 'guardrails'`; such an action is
  never offered to an agent or MCP (`agentCallable: false`), and `pnpm check:agent-actions` keeps the catalog and the exported
  eval list in step.
- **Decision**: the three actions of the settings (`settings.models.context.get`, `.set`, `.report`) are guardrails. The
  context a model runs with is the person's to decide: a prompt injection that sets a 4-bit cache or 4096 tokens would
  quietly make the agent worse.
