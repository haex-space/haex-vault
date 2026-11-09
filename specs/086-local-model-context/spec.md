# Feature Specification: Context for local models

**Feature Branch**: `086-local-model-context`

**Created**: 2026-11-09

**Status**: Draft

**Input**: User description: "Local models get a context of at least 128k tokens wherever the model and the device allow it, instead of being cut to 8k (operator 2026-11-09: 128k is the minimum a useful agent works with). The context is configurable per model and per device: context length (Auto or a fixed value) and KV cache type (standard, 8-bit, 4-bit), because people will try any model and other settings. A context beyond what the model was trained for uses YaRN rope scaling and says so. The settings show what the model was trained for, what the device allows and what is in effect. When a conversation still uses up the context, it is compacted instead of failing: older messages are replaced by a summary the model itself writes, stored in the session and shown in the history; the original rows stay; the recent messages and the open tool round stay as they are; cheaper steps come first."

## Background

- **A local model had 8192 tokens, and one browser page filled them.** The engine answered "the prompt has 10522 tokens and does not fit the context of 8192 tokens", and because the same history is sent again with every message, the session stayed unusable (operator, 2026-11-09, Gemma 4 12B).
- **The 8192 came from a wrong estimate, not from the device.** The memory planner of the llama.cpp engine sized every layer of Gemma 4 12B as a layer that keeps every token, with 16 heads: about 1.5 MB of cache per token, 12 GB at 8192 tokens. The file says otherwise: 40 of its 48 layers look at a window of 1024 tokens, 8 look at everything with one head. At 128k the cache is 2.6 GiB. The catalog also said 32768 for models trained for 128k and 256k.
- **Two stages are already built** (PRs #903 and #904, before this spec): a request is fitted to the context of the model by leaving out old tool results and cutting the largest one of the latest round, and a browser page is read in a size that fits a small context; the planner reads per-layer KV heads, window layers and the attention layers of hybrid models (Qwen3.5), plans for 131072 tokens, and keeps every layer on the GPU down to 32768 before it moves layers to the host. This spec records what they do as requirements and adds the settings and the compaction.
- **What a model's file allows differs.** Gemma 4 12B and Qwen3.5 are trained for 262144, Gemma 4 E4B for 131072, Qwen3 for 32768 (the family needs YaRN rope scaling to go to 131072, and its cache is large: 18 GiB at 128k in f16, 9 GiB in 8-bit). People will install other models and want to try other settings.

## Clarifications

### Session 2026-11-09

- Q: Where does a compaction summary live? → A: Stored in the session and visible in the history ("older messages summarised"); not recomputed with every message (operator 2026-11-09).
- Q: How is more context reached when 128k does not fit the card in f16? → A: A quantised KV cache and a realistic estimate; layers stay on the GPU; measured on the operator's card (operator 2026-11-09).
- Q: Order of work? → A: First the request fitting (PR #903), then this spec (operator 2026-11-09).

## User Scenarios & Testing _(mandatory)_

### User Story 1 - A useful context without doing anything (Priority: P1)

I load any local model and it has the context the model was trained for, up to 128k tokens, as far as the memory of my device allows, without my choosing anything. A browser page, a long file or ten tool results fit.

**Why this priority**: With 8k the built-in agent cannot read a single page. This is the precondition for every agent task on a local model.

**Independent Test**: Load Gemma 4 12B on a card with 12 GiB free: the context in effect is 131072 and every layer is on the GPU. Load Qwen3.5 4B: 131072. Load Qwen3 4B (trained for 32768): 32768, and the settings page says why it is not more.

**Acceptance Scenarios**:

1. **Given** a model trained for at least 131072 and a device with the memory for it, **When** it is loaded with the setting Auto, **Then** the context in effect is 131072 and the layers placed on the GPU are as many as before the context was raised.
2. **Given** a model trained for 32768, **When** it is loaded with Auto, **Then** the context in effect is 32768 and nothing in the chat suggests it is smaller than it could be.
3. **Given** a device whose memory does not hold 131072 tokens next to the model, **When** the model is loaded with Auto, **Then** the context is the largest that fits with the model fully on the GPU, but not below 32768 while that can be had with part of the model on the host; the person is told which one applies.
4. **Given** a model whose file declares no limit of its own, **When** it is loaded with Auto, **Then** the engine plans for 131072 tokens and the memory decides.

---

### User Story 2 - Choose the context and the cache per model (Priority: P1)

For a model on this device I set the context length (Auto, or a fixed number) and the KV cache type (standard, 8-bit, 4-bit). The settings show what the model was trained for, what this device allows and what is in effect now. I try Qwen3 with 128k and an 8-bit cache, look at what it does, and set it back.

**Why this priority**: People install any model and test settings. A fixed target that cannot be changed makes holzi the wrong tool for that, and some models only reach 128k with a smaller cache.

**Independent Test**: In the settings of a local model choose 65536 and an 8-bit cache; the page shows the model's training limit, the context the device allows with that cache and, once the model is loaded again, the context in effect. Choose Auto; the default is back.

**Acceptance Scenarios**:

1. **Given** the settings of a local model, **When** I open them, **Then** they show the context the model was trained for, the context this device allows with the chosen settings, the context in effect (when the model is loaded) and the cache type.
2. **Given** a fixed context that fits, **When** the model is loaded, **Then** that context is in effect exactly.
3. **Given** a fixed context that does not fit with the model fully on the GPU but does with part of it on the host, **When** I choose it, **Then** the page says the model will run slower and by how much memory it exceeds the card, and the choice stays mine.
4. **Given** a fixed context that fits nowhere, **When** I choose it, **Then** the page says what the model would need and what is free, and the model is not loaded with it.
5. **Given** a context above what the model was trained for (up to four times), **When** I choose it, **Then** the page says that the model is stretched with YaRN rope scaling beyond its training and that answers to short texts may get worse; the model is loaded with that scaling and with the original training length declared.
6. **Given** a cache type of 8 or 4 bits, **When** the model is loaded, **Then** the memory estimate counts the smaller cache and the page says that a smaller cache can lower the quality of long contexts.
7. **Given** settings I changed, **When** the model is loaded again (or reloaded by the choice), **Then** they apply; the settings are per model and per device (another device has its own), and no save or apply button is needed.

---

### User Story 3 - A conversation that outgrows its context goes on (Priority: P1)

A conversation, its tool results and the page I just read use up the context. holzi shrinks what it can without loss of meaning first, and when that is not enough it summarises the older messages. I see a line in the history saying older messages were summarised, I can still read them, and the agent goes on with what it knew.

**Why this priority**: Whatever the size of the context, a long task reaches it. Today the next message then fails.

**Independent Test**: With a model of a small context, read ten pages one after another with the agent; no message ends with "does not fit the context", the history shows where messages were summarised, and the agent still knows the first task and the results it needs.

**Acceptance Scenarios**:

1. **Given** a request that would not fit the context, **When** it is built, **Then** the steps are tried in this order and stop at the first that makes it fit: a tool result is read in a smaller part, old tool results are left out, older messages are summarised.
2. **Given** a summary is needed, **When** the model writes it, **Then** it is stored in the session as a row of its own with the statement of which messages it covers, shown in the history as "older messages summarised", and the covered messages stay readable and unchanged.
3. **Given** a stored summary, **When** a later request is built, **Then** it uses the summary in place of the covered messages only if the request would not fit without it; a device with a larger context uses the original messages.
4. **Given** a conversation that has been summarised before, **When** it outgrows the context again, **Then** the earlier summary is part of what is summarised, and the newest messages and the open tool round stay verbatim.
5. **Given** the summary cannot be written (no answer, the text to summarise does not fit, an error), **When** the request is built, **Then** the request is fitted as before (old tool results left out, the largest result cut), the person is told that the conversation could not be summarised, and nothing of the history is lost.
6. **Given** the person's latest message and the open tool round, **When** a summary is made, **Then** they are never part of it.

---

### Edge Cases

- A model is loaded while another application holds the memory of the card: the plan is made at the load from what is free then; the context in effect can differ between loads and the page shows the one in effect.
- A model trained for less than the person chose without YaRN being possible (a model that is not stretched by rope scaling): the choice is refused with the reason.
- The imported model has a header the planner cannot size: it is loaded with the setting asked for or the default of the engine, and the page says the memory could not be estimated.
- A phone with little memory: the same rules apply; the context shrinks to what fits and the page says so.
- The summary of a conversation is made by a model that is weaker than the one that reads it later: acceptable; the original messages stay and a device with a larger context uses them.
- A conversation with images or attachments in the part to be summarised: the summary names that they were there; the originals stay in the history.
- The model that writes the summary is the same one that runs the conversation; a conversation held with a model whose context is not known (the provider does not say) is not summarised.
- A voice conversation (spec 066) reaches the limit: the compaction runs between rounds, and the assistant says briefly that it is thinking, as for a slow model.
- Two devices write a summary of the same messages at the same time: both rows are kept; each request uses the newest that covers the most.
- The context setting is changed while the model is generating: it applies at the next load; the running answer is not interrupted.

## Requirements _(mandatory)_

### Functional Requirements

**Default context**

- **FR-001**: A local model MUST be planned for a context of 131072 tokens, capped by the limit its file declares and by what the memory of the device allows. A number from the catalog MUST NOT lower it.
- **FR-002**: The estimate of the memory MUST follow the file: KV heads per layer, the size of the heads, layers that look at a window only (they stop growing at the window and a margin), and layers of a hybrid model that keep a state of fixed size instead of a cache. Where the file does not say, every layer counts as one that keeps every token.
- **FR-003**: A full placement on the GPU MUST be tried at the target and at lower contexts down to 32768 before layers move to the host.
- **FR-004**: A layer that looks at a window MUST allocate its window, not the whole context, so that the estimate and the allocation agree.

**Settings**

- **FR-005**: For every local model the person MUST be able to choose, per model and per device, the context length (Auto, or a fixed number of tokens) and the KV cache type (standard, 8-bit, 4-bit). Auto is the default. The choice is a preference of this device.
- **FR-006**: The settings of a model MUST show the context it was trained for, the context this device allows with the chosen settings, the context in effect while it is loaded, and the cache type, and MUST say in words why a context is smaller than the target (training limit, memory, a choice).
- **FR-007**: A fixed context MUST be honoured as asked if it fits. If it fits only with layers on the host, the settings MUST say it runs slower; if it fits nowhere, they MUST say what is needed and what is free, and the model MUST NOT be loaded with it.
- **FR-008**: A context above the training limit MUST be possible up to four times that limit, using YaRN rope scaling with the training length declared, and the settings MUST say that this stretches the model beyond its training. Where the model does not allow it, the choice MUST be refused with the reason.
- **FR-009**: A KV cache of 8 or 4 bits MUST be counted by the estimate as the smaller cache, and the settings MUST say that a smaller cache can lower the quality of long contexts.
- **FR-010**: The settings MUST apply at the next load of the model, with no save or apply button; an error appears as a toast and a wrong input under its field.
- **FR-011**: The settings MUST be reachable by the person only; no agent can read or change them over the tools or MCP (they are settings of the guardrail kind).

**Fitting and compaction**

- **FR-012**: A request MUST be fitted to the context of the model before it is sent, when the adapter reports a limit: first a tool result is read in a smaller part (a browser page, a listing), then old tool results are left out oldest first, the largest result of the latest round is cut, and only then older messages are summarised. The stored rows MUST NOT change by fitting.
- **FR-013**: A tool that returns pages or listings MUST take the budget of one result into account (at most a quarter of the context, never below a usable minimum), and say in its result that it was cut and how to read on.
- **FR-014**: When fitting is not enough, the older messages MUST be summarised by the model of the conversation. The summary MUST be stored as a row of the session with the messages it covers, shown in the history as "older messages summarised", and the covered messages MUST stay in the history unchanged and readable.
- **FR-015**: A stored summary MUST be used in a request only where the request would not fit without it. The newest messages, the person's latest message and the open tool round MUST stay verbatim.
- **FR-016**: A summary MUST be written once and reused. When the conversation outgrows the context again, the earlier summary MUST be part of what is summarised.
- **FR-017**: When a summary cannot be written, the request MUST be fitted as in FR-012, the person MUST be told, and no history MUST be lost.
- **FR-018**: A conversation with a model whose context is not known MUST NOT be summarised.
- **FR-019**: Summary rows MUST sync like other rows of a session, and a request on another device MUST apply FR-015 with the context of that device.

### Key Entities

- **Context setting**: The choice of a person for one local model on one device: the context length (Auto or a number) and the cache type. Held as a preference of this device.
- **Context in effect**: What the engine planned at the last load of a model: tokens, layers on the GPU, the cache type, whether YaRN is on. Read from the loaded model.
- **Model limits**: What the file says and the device allows: the training limit, the largest context that fits with every layer on the GPU, the largest that fits at all, and for each the cache type assumed.
- **Summary row**: A row of a session that says it summarises messages up to a message, with the text the model wrote. Shown in the history; used by a request only when needed.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: Gemma 4 12B and Qwen3.5 4B, loaded with Auto on a card with 12 GiB free, run with a context of 131072 tokens and every layer on the GPU.
- **SC-002**: A model with a context of 8192 tokens reads ten browser pages in one conversation with no message that ends with "does not fit the context".
- **SC-003**: After a conversation has been summarised, the agent answers a question about the first task of the conversation correctly in at least four of five runs of the same conversation.
- **SC-004**: The settings of a model show the training limit, the context the device allows and the context in effect, and a person finds out why a context is smaller than 128k without reading the log.
- **SC-005**: Choosing another context or cache type in the settings and loading the model again changes the context in effect to the one chosen, for every local model tried (Gemma 4, Qwen3, Qwen3.5) where the choice fits.
- **SC-006**: No request to a local model is refused by the engine because the prompt is longer than the context, as long as the system prompt, the tools and the person's latest message fit.

## Assumptions

- The llama.cpp engine is the only local engine (spec 070); remote models report no limit of their own here and are not touched, so FR-012 to FR-019 apply where an adapter reports a limit, today the local one.
- "Would not fit" in FR-012 and FR-015 leaves headroom: a quarter of the context stays free for the tool round of the turn, the share one tool result may take (plan R10), so a turn that starts at the limit does not have to cut its first result to nothing.
- A token is estimated as three characters for the fitting; the engine's exact count is not asked for before a request.
- The memory of a device is read from the engine's device list at the load, as the planner does today; the plan is not repeated while the model is loaded.
- YaRN is the way to stretch a model beyond its training; a factor above four is not offered.
- A summary written by the model of the conversation is good enough for the next task; a better one (a larger model, a second pass) is not part of this spec.
- The settings page of a local model is the place of the controls (spec 023, "KI" category); how they look follows the other settings (rows, reset buttons, no apply buttons).
- Cloud models and delegates keep their providers' contexts; nothing here changes them.

## Out of Scope

- Raising the context of remote models and delegates.
- A display of how full the context is while chatting (a later idea).
- Other engine settings (number of layers on the GPU, flash attention, threads, batch sizes); the settings of this spec leave room for them but add none.
- Retrieval over earlier sessions or a long-term memory of the agent.
