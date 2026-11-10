# Quickstart: Context for local models

Validation of spec 086 by hand, on a machine with a GPU and the models of the catalog installed. Each step names the spec item it
proves. The numbers of M1 and M2 go into `research.md` R9 as a table.

## Prerequisites

- A build with `llm-llamacpp` (`pnpm tauri:dev`), a vault, and these models installed: Gemma 4 12B, Gemma 4 E4B, Qwen3.5 4B,
  Qwen3 4B.
- `cargo run --release --features llm-llamacpp --example context_probe -- <file.gguf> --cache q8 --context 131072` for M1.

## M1 Measure (before the defaults are named, R9)

1. For each model and each cache type (standard, q8, q4) run the probe at 32768, 65536 and 131072 (Qwen3: 32768 and, with YaRN,
   65536 and 131072).
2. Record peak device memory, time to fill the context, tokens per second at the start and at the end, and the needle result.
3. Expected: Gemma 4 12B and Qwen3.5 4B fit at 131072 with every layer on a 12 GiB card in the standard cache (SC-001); Qwen3 does
   not without 8-bit. Anything else is written down as found and the plan's defaults are revisited before T016 (R9).

## M2 Default context (US1, SC-001)

1. Settings → AI → local models → Gemma 4 12B: the page shows the trained length 262144, the context the device allows and, after
   activating, the context in effect 131072 with every layer on the GPU. Repeat with Qwen3.5 4B.
2. Qwen3 4B: in effect 32768; the page says "trained for 32768" as the reason (US1 scenario 2).

## M3 Settings (US2, SC-004, SC-005)

1. Qwen3 4B: choose 131072 and the 8-bit cache. The page says it stretches the model with YaRN beyond its training and that a
   smaller cache can lower the quality. The model reloads; the context in effect is 131072 (scenario 5, 6).
2. Choose 524288: the page refuses it (above four times 32768).
3. Choose a context that fits only with layers on the host: the page says it runs slower and by how much it exceeds the card; the
   choice stays (scenario 3). Choose one that fits nowhere: the page says what is needed and what is free, and the model does not
   load with it (scenario 4).
4. Choose Auto and standard: the default is back, the row is gone from the vault.
5. Look at the same model on a second device: its own setting (scenario 7).
6. Run `settings.models.context.set` over MCP: refused as unknown (FR-011).

## M4 Compaction (US3, SC-002, SC-003, SC-006)

1. Gemma 4 12B with a fixed context of 8192: ask the agent to open five different pages one after another and say what each is
   about. No message ends with "does not fit the context"; earlier pages are shortened in the request (scenario 1).
2. Same, with ten pages and a long first question. The history shows the line "older messages summarised" after the first
   messages; opening it shows the summary; the covered messages are still readable above it (scenario 2).
3. Ask "what was my first task?": the answer is right in four of five repeats of the whole conversation (SC-003).
4. Open the same conversation on a device with 131072: the request uses the originals, the summary is only shown (scenario 3).
5. Make the summary fail (stop the model while it summarises): one notice, the next message still gets an answer, no row is lost
   (scenario 5).
6. A conversation of one message and a long tool result: nothing is summarised, the result is cut as before (scenario 6).

## M5 Voice and unknown limits (spec Edge Cases)

1. A voice conversation on a small context: the compaction runs between turns and the assistant says it is thinking.
2. A conversation with a provider model: nothing is summarised, nothing changes (FR-018).
