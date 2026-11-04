# Feature Specification: Reuse the prompt a local model has already read

**Feature Branch**: `080-prompt-cache`

**Created**: 2026-11-04

**Status**: Draft

**Input**: User description: "Keep the context and KV cache of a loaded local model between generations, so every
agent step and every answer reads only the new part of the prompt instead of all of it." (operator, 2026-11-04,
after spec 079 was parked for this.)

## Background

- **What the model reads**: before a local model writes a single word, it reads the whole prompt: holzi's system
  prompt, the descriptions of the offered tools, the conversation so far and the new request. In a chat with
  tools that is about 4000 tokens.
- **What that costs, measured 2026-11-04** (reference laptop, RTX A2000 8 GB; spec 079 research R11):

  | Model       | Reading a 4000-token prompt | Writing a 40-token spoken answer |
  | ----------- | --------------------------- | -------------------------------- |
  | Gemma 4 E2B | 1.46 s                      | 0.5 s                            |
  | Gemma 4 E4B | 2.51 s                      | 0.9 s                            |

  Reading takes about three times as long as writing a spoken answer. On a phone, where the model runs on the
  processor, reading is slower still.

- **How often it is read again**: a request that needs tools calls the model several times in a row. Each call
  repeats the whole prompt and only appends the last tool result. A follow-up question in the same conversation
  repeats the system prompt, the tools and the history. Today every call reads everything from the start, because
  the local engine forgets what it has read once an answer is finished (spec 070 named reusing it as a later step).
- **A part that changes every minute**: the system prompt tells the model the current date and time to the minute.
  It sits before the tool descriptions, so a follow-up question one minute later differs from the previous prompt
  before most of its length.
- **Why now**: spec 079 (a helper model that writes answers faster) was parked because reading dominates a spoken
  turn. It will be measured again after this feature.

## Decisions

- **Only the local engine that reads GGUF files through llama.cpp** keeps what it has read. The other local engine
  stays as it is.
- **Same answers, only sooner**: reusing what was read changes when the answer starts, never what the model
  decides.
- **One model, one memory of what it read**: chat, voice assistant and model jobs that use the same loaded model
  take turns with it; each turn reuses what matches and reads the rest. A model job of an extension never reuses
  what another caller left.
- **Only in memory**: what the model has read holds conversation and vault content. It stays in the memory of the
  running app, is never written to a file, and ends with the model or the vault session.

## User Scenarios & Testing _(mandatory)_

### User Story 1 - The next step of a tool chain starts at once (Priority: P1)

The user asks "what did Anna write?". The model looks for the mail, gets the result and then answers. Today, the
second call reads the whole prompt again before it can use the result. With this feature it reads only the tool
result and starts answering almost at once.

**Why this priority**: tool chains are the everyday case of the voice assistant (spec 078), and every step after
the first pays the full reading time today.

**Independent Test**: run a two-step tool chain with a local model on the reference laptop and compare the time to
the first word of the second step with and without the feature.

**Acceptance Scenarios**:

1. **Given** a local model has answered the first step of a tool chain, **When** the second step starts with the
   tool result appended, **Then** the model reads only the appended part, and the second step's first word comes
   at least three times sooner than today.
2. **Given** the same chain with deterministic settings, **When** it runs with and without the feature, **Then**
   both runs name the same tool calls with the same arguments and give the same answer.

---

### User Story 2 - A follow-up question in the same conversation starts sooner (Priority: P1)

The user asks a second question a few minutes after the first. The system prompt, the tools and the earlier
conversation are the same as before; only the new question is new. The model reads only what changed.

**Why this priority**: a conversation is a sequence of turns, and each turn today pays the full reading time again.
Because the time of day in the system prompt changes every minute, this needs the changing part not to hide the
unchanged rest.

**Independent Test**: ask a question, wait two minutes, ask a follow-up in the same conversation, and compare the
time to the first word of the follow-up with and without the feature.

**Acceptance Scenarios**:

1. **Given** a finished turn in a conversation, **When** the user asks a follow-up two minutes later, **Then** the
   model reads at most the part of the prompt that changed since the last turn plus the new question, and the
   first word comes at least twice as soon as today.
2. **Given** the date and time changed since the last turn, **When** the follow-up starts, **Then** nothing before
   the new question is read again (the system prompt, the tool descriptions and the earlier turns), apart from
   what the chat template writes differently for an earlier turn (edge case "Thinking that is not shown again").
3. **Given** the change that makes this possible, **When** the model is asked about the current time, **Then** it
   still knows the date and time to the minute, and the everyday voice set of spec 078 passes as often as before.

---

### User Story 3 - Nothing gets worse (Priority: P1)

Whatever happens between two answers (another conversation, a model job, a cancelled answer, an error, a reload of
the model), the next answer is as correct as today. At worst it is read from the start, as it is today.

**Why this priority**: reusing stale reading would give wrong answers silently; this must never happen.

**Independent Test**: alternate between two conversations and a model job on the same model, cancel an answer
half-way, and check every answer against the same run without the feature.

**Acceptance Scenarios**:

1. **Given** a model just answered conversation A, **When** conversation B asks something, **Then** B's answer is
   the same as without the feature; only the common beginning (the system prompt and tools) may be reused.
2. **Given** an answer was cancelled half-way, **When** the next request comes, **Then** nothing of the cancelled
   answer is reused unless it is also part of the new prompt.
3. **Given** a generation failed with an error, **When** the next request comes, **Then** it is read from the start.
4. **Given** the model is unloaded, reloaded or replaced by another, **When** the next request comes, **Then**
   nothing from before is reused.
5. **Given** a model job runs on the model the chat uses, **When** the chat asks next, **Then** the chat's answer
   is correct; the chat may lose the time advantage, never the correctness.
6. **Given** the chat has just answered, **When** a model job of an extension runs on the same model, **Then** the
   job reads its whole prompt; nothing the chat left is reused, so the job's timing tells nothing about the chat.

---

### User Story 4 - The desktop default model gains as well (Priority: P1)

Qwen3.5 4B, holzi's default local model on desktops, keeps part of its memory in a form that cannot be cut back to
an earlier point. When a new prompt matches only the beginning of what was read, the model still reuses as much as
possible instead of falling back to reading everything.

**Why this priority**: the default model is what most users run; a gain only for Gemma 4, which holzi offers just
to try, would miss most of them.

**Independent Test**: User Stories 1 and 2 with Qwen3.5 4B.

**Acceptance Scenarios**:

1. **Given** Qwen3.5 4B, **When** the second step of a tool chain starts, **Then** its first word comes at least
   three times sooner than today.
2. **Given** Qwen3.5 4B and a follow-up question whose prompt differs from the last one before its end, **When** it
   starts, **Then** the model reuses the system prompt and the tool descriptions, apart from at most 1024 tokens
   before their end.

---

### User Story 5 - Memory stays planned (Priority: P2)

The model's reading is kept between answers, which holds memory on the GPU or in the computer's memory for as long
as the model is loaded. holzi plans that memory before loading, so no load or answer fails because of it.

**Why this priority**: the inference library aborts the whole app when GPU memory runs out; but holzi already
plans the memory of one answer, so the change is in what the plan counts.

**Independent Test**: load each catalog model on the reference laptop, and the mobile default model on an Android
phone, run ten answers in a row, and check that no load or answer fails that succeeds today.

**Acceptance Scenarios**:

1. **Given** a model that loads today, **When** it loads with the feature, **Then** it loads, with at most the
   context length it gets today.
2. **Given** ten answers in a row with long prompts, **When** they run, **Then** none fails for lack of memory.

### Edge Cases

- **A prompt longer than what was kept**: the kept reading covers the beginning; the rest is read as today.
- **A prompt that fills the context**: when the new prompt plus the answer does not fit next to what is kept, the
  kept part that does not match is dropped first; the answer never fails because something was kept.
- **The offered tools change within a chain**: when a step offers other tools than the step before, everything
  from the first changed tool description on is read again.
- **Deterministic settings**: the evaluation runs greedy. Reading a prompt in one piece and in two pieces can round
  differently in rare places, as speculative decoding can (spec 079); the tool calls are what must match.
- **Thinking that is not shown again**: some chat templates drop the model's thinking from earlier turns. The
  prompt of the next turn then differs from what the model wrote, from the dropped thinking on; that part is read
  again.
- **The app goes to the background on a phone**: what was read may be dropped with the model, as the model itself
  may be today; the next answer is read from the start.
- **Two requests at once**: holzi already lets only one answer run per model at a time; the kept reading follows
  the same rule.

## Requirements _(mandatory)_

### Functional Requirements

- **FR-001**: For a model loaded through the llama.cpp engine, holzi MUST keep what the model has read and written
  after an answer ends, for as long as the model stays loaded.
- **FR-002**: Before an answer, holzi MUST reuse the longest beginning of the kept reading that is identical to the
  beginning of the new prompt, and read only the rest.
- **FR-003**: Reusing MUST NOT change what the model decides. With deterministic settings, the tool calls and the
  answers MUST be the same as without reuse, apart from rare rounding differences in wording.
- **FR-004**: Nothing that does not match the new prompt exactly MUST influence the new answer: not another
  conversation, not a cancelled or failed answer, not an earlier version of a changed prompt.
- **FR-005**: A failed or cancelled answer, an unload, a reload and a change of model MUST leave holzi in a state in
  which the next answer is correct. When in doubt, the kept reading MUST be dropped and the prompt read from the
  start.
- **FR-006**: For a model whose kept reading cannot be cut back to an arbitrary point (the desktop default Qwen3.5
  4B), holzi MUST still reuse the beginnings a conversation repeats: the whole previous prompt of a tool chain, and
  the system prompt with the tool descriptions apart from at most 1024 tokens before their end.
- **FR-007**: The date and time MUST NOT stop the unchanged system prompt, tool descriptions and earlier turns
  from being reused: the prompt of a turn MUST only grow at its end compared with the previous turn. The model
  MUST still be told the current date and time to the minute.
- **FR-008**: The memory planner MUST count the kept reading. A model that loads today MUST still load, and MUST
  NOT get a shorter context than today.
- **FR-009**: The mistralrs engine and the hosted providers MUST NOT change, apart from where the date and time
  sit in the prompt (FR-007), which is the same for every provider.
- **FR-010**: The measurement tool of spec 078 MUST record, per request, the time to the first word and how much of
  the prompt was reused, and MUST be able to run with the reuse switched off for comparison.
- **FR-011**: The user MUST see no difference other than speed: no setting, no message, no change in the chat.
- **FR-012**: The kept reading MUST live only in the memory of the running app (main memory or graphics memory),
  MUST NEVER be written to a file, and MUST end with the loaded model and at the latest with the vault session,
  which ends the process (spec 013).
- **FR-013**: A request of a model job (spec 055) MUST NOT reuse kept reading that another request left; it reads
  its prompt from the start. The chat, the voice assistant and the evaluations reuse among themselves.

### Key Entities

- **Kept reading**: what one loaded model has read and written so far, as the token sequence and the model's
  memory of it. It belongs to the loaded model, not to a conversation, lives only in memory and ends with the
  model.
- **Prompt**: the full text a request gives the model; its beginning may match the kept reading.
- **Reused part**: the longest matching beginning; its length is what the measurement records.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: On the reference laptop, the second step of a two-step tool chain starts its first word at least
  three times sooner than today, with Qwen3.5 4B, Gemma 4 E2B and Gemma 4 E4B.
- **SC-002**: On the reference laptop, a follow-up question two minutes after the first in the same conversation
  starts its first word at least twice as soon as today, with the same three models.
- **SC-003**: The everyday voice set of spec 078 passes at least as many requests as today with every catalog model,
  all of which run on the llama.cpp engine (Qwen3 0.6B, 1.7B and 4B, Qwen3.5 4B, Gemma 4 E2B, E4B and 12B), with no
  additional safety error, and takes at least 30 % less time in total.
- **SC-004**: With deterministic settings, every everyday request names the same tool calls with the same arguments
  with and without reuse. A request that differs counts as a failure unless the same prompt, read in the same
  pieces into a fresh context, differs the same way (rounding, not reuse; edge case "Deterministic settings").
- **SC-005**: In tests that alternate conversations, model jobs, cancelled answers, failed answers and reloads,
  every answer equals the answer of the same request without reuse (apart from rounding differences in wording).
- **SC-006**: Every catalog model that loads on the reference laptop today still loads, with the same context
  length, and ten answers in a row with long prompts run without a memory failure. On an Android phone, the
  mobile default model does the same in a manual check.

## Assumptions

- **The engine supports it**: the llama.cpp version holzi ships can keep a context between answers, remove a tail
  from the cache of attention models (shown for Gemma 4 in spec 079), and save and restore the state of models
  whose memory cannot be cut back. The plan confirms the last point with a spike.
- **Prompts grow at the end**: holzi builds the prompt of a turn from the system prompt, the tools and the history
  in a fixed order, and a tool chain appends to it. Every user message carries the time it was sent, so a turn
  that becomes history keeps its text (research R8). Templates that rewrite earlier turns (dropped thinking) reuse
  less, never wrongly.
- **One model, taking turns**: one answer runs at a time per loaded model (spec 070); a model job and the chat on
  the same model take turns and may each lose the other's reuse.
- **Phones**: Android runs the same engine on the processor, where reading is slower; it gains the same way. The
  memory plan there is the same rule.
- **Spec 079 afterwards**: the parked speculative decoding is measured again on top of this feature; it is not part
  of it.
