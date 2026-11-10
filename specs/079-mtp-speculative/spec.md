# Feature Specification: Faster local answers with multi-token prediction

**Feature Branch**: `079-mtp-speculative`

**Created**: 2026-11-04

**Status**: Parked on 2026-11-04 (operator). The prompt cache comes first; see research R11.

**Input**: User description: "Can we add MTP for faster token generation?" (operator, 2026-11-04) After the measurement:
"You can start on it now." The measurement and the scope are in the Background below.

## Background

- **What MTP is**: multi-token prediction is a form of speculative decoding. A small helper model (the
  "drafter") proposes the next few words, and the real model checks all of them in one step. The real model keeps
  every proposal that matches what it would have written and discards the rest. So the answer comes out faster
  and the real model still decides every word.
- **Measurement, 2026-11-04**:
  - **Setup**: the reference laptop (RTX A2000 8 GB), using the inference library at exactly the version holzi
    ships. Four typical requests of 200 words each: a short spoken answer, a mail summary, a tool call and a
    longer reply.
  - **Results**:

    | Model       | Without drafter | With drafter | Faster | Proposals kept |
    | ----------- | --------------- | ------------ | ------ | -------------- |
    | Gemma 4 E2B | 88 tok/s        | 127 tok/s    | 1.44×  | about 50 %     |
    | Gemma 4 E4B | 50 tok/s        | 81 tok/s     | 1.63×  | about 53 %     |
    | Gemma 4 12B | 10.5 tok/s      | 17.4 tok/s   | 1.66×  | about 74 %     |

    The 12B model had half its layers on the processor, because it does not fit the 8 GB GPU whole.
- **The drafter is a second file**: for Gemma 4, each model has its own drafter, published next to the model. It is
  about 100 MB for E2B and E4B and 465 MB for 12B. The Gemma 4 models come to holzi as models to try (#806).
- **A known limit**: with a drafter, one acceleration of the attention step has to stay off on the GPU, or the
  drafter aborts. On its own that acceleration brought only about 5 %, and the baseline above had it on.

## Decisions

- **Gemma 4 first**: the first delivery covers Gemma 4 E2B, E4B and 12B, whose drafters were measured.
- **Qwen3.5 4B later**: the desktop default gets the same treatment only after its own measurement shows a
  similar gain. Its drafter is not a separate file but built into a variant of the model file.
- **Same answers, only faster**: the drafter changes how fast an answer is written, never what the model decides.
  Sampling, the offered tools and the prompt stay as they are.

## User Scenarios & Testing _(mandatory)_

### User Story 1 - A local model with a drafter answers faster (Priority: P1)

The user has downloaded Gemma 4 E4B and chats with it or speaks to the voice assistant. The answer appears clearly
faster than before, and nothing else changes: the same tools, the same answer quality.

**Why this priority**: faster generation is the whole point of the feature, and the measurement shows 1.4 to 1.7
times as many tokens per second.

**Independent Test**: on the reference desktop, generate the same requests with a Gemma 4 model once without and
once with its drafter, and compare tokens per second and the answers.

**Acceptance Scenarios**:

1. **Given** Gemma 4 E4B and its drafter are installed, **When** the user sends a chat message, **Then** the answer
   is written with the drafter, and on the reference desktop it comes out at least 1.4 times as fast as without.
2. **Given** a model and its drafter, **When** the same request runs with deterministic settings once with and once
   without the drafter, **Then** both answers name the same tool calls with the same arguments.
3. **Given** a model with a drafter, **When** the everyday voice set of spec 078 runs, **Then** it passes at least
   as many requests as without the drafter and has no additional safety error.

---

### User Story 2 - The drafter comes with the model and goes with it (Priority: P1)

When the user downloads a Gemma 4 model, its drafter is part of the download. The user sees the total size before
starting and one progress for both. Cancelling, deleting the model and the size shown for an installed model treat the drafter as
part of the model.

**Why this priority**: without the drafter on the device there is no speed-up, and a second file the user cannot see
or remove would leave space behind after a deletion.

**Independent Test**: download Gemma 4 E2B, cancel once halfway, download again, then delete it; check that the
drafter appears in the size before the download, the progress and the installed model's size, and is gone after
cancel and after delete.

**Acceptance Scenarios**:

1. **Given** the model list shows Gemma 4 E4B, **When** the user looks at its size, **Then** the size includes the
   drafter.
2. **Given** a Gemma 4 download is running, **When** the user cancels it, **Then** neither the model file nor the
   drafter stays behind half-written.
3. **Given** a Gemma 4 model with its drafter is installed, **When** the user deletes the model, **Then** the
   drafter is deleted too.
4. **Given** the details of an installed model in settings, **When** the model has its drafter, **Then** the size
   shown for the
   model includes the drafter.
5. **Given** the drafter's file is damaged or does not match what the catalog expects, **When** the download ends,
   **Then** the drafter is discarded as any damaged download is, and the model stays usable without it.

---

### User Story 3 - Without a drafter everything works as today (Priority: P1)

A model without a drafter, a drafter that is missing, or a drafter that cannot be loaded never keeps the user from
chatting. The model answers as it does today, only without the speed-up.

**Why this priority**: the speed-up is a bonus; a failure of the bonus must never block the chat or the voice
assistant.

**Independent Test**: start a chat with Qwen3.5 4B (no drafter), with Gemma 4 E4B whose drafter was removed by
hand, and with a drafter that does not belong to the model; all three answer without an error in the chat.

**Acceptance Scenarios**:

1. **Given** a model without a drafter, **When** the user chats, **Then** the model answers as today.
2. **Given** a Gemma 4 model whose drafter is missing on disk, **When** the model loads, **Then** it loads without
   the drafter and answers; the model list shows that the speed-up is missing and offers to download the drafter.
3. **Given** a drafter that cannot be loaded (wrong model, not enough memory, unsupported on this device), **When**
   the model loads, **Then** it loads without the drafter, the chat shows no error, and the reason is in the log.
4. **Given** the drafter stops working in the middle of an answer, **When** generation continues, **Then** the answer
   is finished without the drafter.

---

### User Story 4 - The memory planner counts the drafter (Priority: P2)

holzi decides before loading how much of a model goes on the GPU. With a drafter, that decision includes the
drafter, so loading with a drafter never fails or crashes where loading without one would have worked.

**Why this priority**: the inference library aborts the whole process when GPU memory runs out, so a forgotten
drafter could crash the app; but it matters only on devices near their memory limit.

**Independent Test**: on the reference laptop, load Gemma 4 12B with its drafter ten times; every load succeeds. The
drafter is used for a partly placed model only where a measurement in holzi's own placement showed a gain.

**Acceptance Scenarios**:

1. **Given** a model that fits the GPU only partly and whose drafter was measured faster in that placement, **When**
   it loads with its drafter, **Then** the placement leaves room for the drafter and the load succeeds. Without such
   a measurement, it loads without the drafter (FR-009).
2. **Given** a device where the model fits but the model plus drafter does not, **When** the model loads, **Then**
   holzi chooses between fewer layers on the GPU with the drafter and the same layers without it, and takes the one
   that is faster on that device. If the planner cannot tell, it loads without the drafter.

---

### User Story 5 - Gemma 4 models installed before this feature get their drafter (Priority: P3)

A user who downloaded a Gemma 4 model before this feature has no drafter. The model list shows that a speed-up is
available, with its size, and the user can download it with one action. Nothing downloads without the user asking.

**Why this priority**: few users have Gemma 4 installed yet (it arrives as a model to try), but they should not have
to download the whole model again.

**Independent Test**: install Gemma 4 E2B without its drafter, open the model list, download the speed-up, and check
that the next answer uses it.

**Acceptance Scenarios**:

1. **Given** Gemma 4 E2B is installed without its drafter, **When** the user opens the model list, **Then** the model
   shows an action to download the speed-up with its size.
2. **Given** the user starts that download, **When** it finishes, **Then** the next load of the model uses the
   drafter without a restart of the app.

---

### Edge Cases

- **Model and drafter mismatch**: the drafter must belong to exactly that model family and size, and holzi checks
  that before using it. A drafter for E4B is never loaded with E2B.
- **Low memory on a phone**: the drafter adds about 100 MB. On a phone that has room for the model but not for both,
  the model loads without the drafter (User Story 4).
- **A different quantisation of the model**: a drafter works with any quantisation of its own model. A user who
  installs another quantisation of Gemma 4 through the Hugging Face search gets no drafter in this delivery.
- **Answers that are mostly copied text**: for a tool call with long fixed arguments, more proposals are kept; for
  free text, fewer. The speed-up varies with the content; the criteria use the measured mix of four requests.
- **Parallel requests**: holzi generates one answer at a time per model. The drafter follows the same rule.
- **The answer is cancelled**: stopping an answer stops the drafter too. The next answer starts clean.
- **Thinking models**: Gemma 4 can write a thinking part before the answer. The drafter speeds up both parts, and
  how thinking is shown stays as it is.
- **A device without a GPU**: on the processor alone the speed-up may be smaller or absent. Whether the drafter
  is used there follows the same rule as in User Story 4: only where it is faster.

## Requirements _(mandatory)_

### Functional Requirements

- **FR-001**: When a local model has a drafter installed and the device can hold both, holzi MUST generate its
  answers with the drafter.
- **FR-002**: The drafter MUST NOT change what the model decides. Sampling settings, prompt, offered tools and
  stop rules MUST stay the same with and without a drafter.
- **FR-003**: The catalog MUST be able to name a drafter for a model: where it is published, its file, its
  size, and the model it belongs to. The first delivery names drafters for Gemma 4 E2B, E4B and 12B.
- **FR-004**: Downloading a catalog model that has a drafter MUST download the drafter as part of the same action.
  The size shown before the download and the progress MUST include the drafter.
- **FR-005**: Cancelling a download, deleting a model, and the size shown for an installed model MUST treat the
  drafter as part of
  its model. After a cancel or a delete, no drafter file MUST stay behind.
- **FR-006**: A drafter download MUST be checked as any model download is. A damaged drafter MUST be discarded,
  and its model MUST stay usable.
- **FR-007**: Before loading, the memory planner MUST count the drafter. Loading with a drafter MUST NOT fail or
  crash where loading the same model without it would have succeeded.
- **FR-008**: If the drafter is missing, does not belong to the model, cannot be loaded, or stops working during an
  answer, holzi MUST continue without it. The user MUST see no error in the chat or the voice assistant, and the
  reason MUST be written to the log.
- **FR-009**: holzi MUST use the drafter only where it is faster. Where the planner must choose between fewer GPU
  layers with the drafter and the full placement without it, and cannot tell which is faster, it MUST load without
  the drafter.
- **FR-010**: For an installed model whose drafter is missing, the model list MUST offer the drafter download with
  its size. holzi MUST NOT download a drafter without the user asking.
- **FR-011**: The model list MUST show whether an installed model has its speed-up.
- **FR-012**: The rules for the GPU that a drafter needs (one attention acceleration off) MUST apply only while a
  drafter is in use. Without a drafter, the model MUST run with the settings it uses today. If the drafter stops
  working during an answer, the rest of that answer MAY keep the drafter's settings. Later answers MUST run without
  the drafter and with today's settings until the model is loaded again.
- **FR-013**: The measurement tool of spec 078 MUST be able to run a model with and without its drafter, and MUST
  record tokens per second for both in its report.
- **FR-014**: Qwen3.5 4B, or any model whose drafter is built into the model file, MUST get a drafter only in a
  later delivery, after a measurement shows a gain like Gemma 4's. This delivery MUST NOT change how Qwen3.5 4B
  loads.

### Key Entities

- **Drafter**: a small helper model that belongs to exactly one model. It has a source, a file, a size and an
  installed state on each device.
- **Catalog entry (extended)**: a downloadable model; it may name one drafter.
- **Installed model (extended)**: a model on this device; it knows whether its drafter is installed, and its size
  includes the drafter.
- **Placement**: the planner's decision of GPU layers and context; it now records whether the drafter is used.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: On the reference desktop, holzi writes answers with Gemma 4 E2B, E4B and 12B at least 1.3 times as
  many tokens per second with the drafter as without, over the four measured request types, rendered through the
  model's chat template as the app does (operator, 2026-11-04: the 1.4 of the first draft came from thinking
  answers, research R11). For a model that fits
  the GPU only partly (the 12B on the reference desktop), this is measured in holzi's own placement. If it is not
  reached there, that model loads without the drafter (FR-009), and the result records the criterion as not reached
  for it.
- **SC-002**: On the everyday voice set of spec 078, every Gemma 4 model passes at least as many requests with the
  drafter as without, and has no additional safety error.
- **SC-003**: With deterministic settings, at least 95 % of the everyday requests name the same tool calls with the
  same arguments with and without the drafter.
- **SC-004**: In tests of the four failure cases of User Story 3 (no drafter, missing drafter, wrong drafter, drafter
  that fails during an answer), every request gets an answer and the chat shows no error.
- **SC-005**: After deleting a model with a drafter, no file of the model or the drafter stays on the device.
- **SC-006**: Loading Gemma 4 12B with its drafter on the reference laptop succeeds in 10 of 10 tries, without a
  crash.
- **SC-007**: The time from sending a voice request to the first spoken word does not get longer with the drafter.

## Assumptions

- **The library supports it**: the inference library at the version holzi ships supports the Gemma 4 drafter. The
  measurement used exactly that version.
- **The catalog entries exist**: the Gemma 4 entries come from #806. This feature extends them and does not decide
  which models are suggested.
- **No switch for the user**: the drafter is used wherever it helps. A settings switch to turn it off is out of
  scope, because the answers do not change.
- **Phones**: Android builds the same code. holzi runs models there on the processor only, and FR-009 keeps the
  drafter off without a GPU. Measuring a phone is left to a later delivery.
- **Rare differences in wording**: checking several proposed words at once uses slightly different arithmetic than
  writing one word at a time. In rare cases a deterministic answer can therefore differ in wording; SC-003 allows
  for that, and the tool calls are what matter.
- **The first word comes no sooner**: the drafter speeds up writing the answer, not reading the prompt. The time to the
  first word therefore stays about the same (SC-007), and long answers gain the most.
- **Drafters from the Hugging Face search**: models installed through the free Hugging Face search, outside the
  catalog, get no drafter in this delivery.
