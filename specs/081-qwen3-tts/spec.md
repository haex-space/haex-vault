# Feature Specification: Qwen3-TTS as haex's natural voice

**Feature Branch**: `081-qwen3-tts`

**Created**: 2026-11-04

**Status**: Draft

**Input**: User description: "Qwen3-TTS 1.7B as an additional speech output engine for the voice assistant
(066), running through haex's bundled llama.cpp. The operator judged Qwen3-TTS 1.7B the best-sounding voice,
both on the processor and on the graphics card. Pocket TTS stays for devices where Qwen is too slow (Android,
weak processors) and moves to Kyutai's retrained German model. Own voices (066 FR-049) must work with Qwen
too. The number and time normalization (066 FR-022b) applies to every speech model." (operator, 2026-11-04,
after a listening comparison of nine variants.)

## Background

- **What haex speaks with today**: Pocket TTS by Kyutai, one model per language (about 150–375 MB), run on
  the processor. Every voice, shipped or own, is a short reference recording that the model imitates (066
  FR-029, FR-049).
- **What the operator heard, 2026-11-04**: six German sentences of the kind an agent writes, spoken by nine
  variants that all imitate the shipped voice "Lena". Qwen3-TTS 1.7B sounded best, clearly ahead of every
  Pocket variant. It still sounded right when it ran through haex's own llama.cpp, which imitates a voice
  from its voice print only and does not use the transcript of the reference.
- **What it costs, measured 2026-11-04** (reference laptop: i7-12800HX, 62 GB RAM, RTX A2000 8 GB; spec 066
  research R23). "Speed" is computing time per second of speech; below 1 is faster than real time.

  | Variant                                                    | Speed | First audio after |
  | ---------------------------------------------------------- | ----- | ----------------- |
  | Pocket TTS in haex today (processor, 4 cores)              | 0.38  | not measured      |
  | Pocket TTS, Kyutai's September model, Kyutai's own runtime | 0.18  | not measured      |
  | Qwen3-TTS 1.7B in haex, processor, 4 cores                 | 1.65  | not measured      |
  | Qwen3-TTS 1.7B in haex, processor, 8 cores                 | 1.35  | not measured      |
  | Qwen3-TTS 1.7B in haex, graphics card                      | 0.36  | about 0.07 s      |

- **What it needs**: one download of about 2.3 GB covers every language haex speaks (German, English,
  French, Spanish, Italian, Portuguese). The model is licensed Apache-2.0, which allows shipping and using it
  with haex. It runs inside the llama.cpp engine that haex already ships for chat models (spec 070); no new
  engine enters the app.

## Clarifications

### Session 2026-11-04

- Operator: Qwen3-TTS 1.7B sounds best, on the processor as well as on the graphics card. → It becomes a
  local speech model next to Pocket TTS, selectable on every device where haex can run it (FR-001, FR-006).
- Operator: Pocket TTS stays for devices where Qwen is too slow and gets Kyutai's retrained German model
  (US4, FR-013).
- Operator: a sentence must never be cut off. → haex splits long sentences into parts at natural breaks
  and speaks them one after another (FR-010); a part that goes wrong is generated again before anyone
  hears it, so nothing is lost (FR-010a).

### Session 2026-11-05

- Q: What happens to a part that failed twice when no Pocket TTS model of the language is installed? → A: That
  case should not arise: while Qwen3-TTS is chosen for a language, haex keeps the language's Pocket TTS model
  installed as the reserve (about 150 MB) and downloads it with Qwen3-TTS (FR-010a).
- Operator: Pocket TTS of the language is always installed, whatever speech model the user chooses; and
  Qwen3-TTS is haex's default speech model for now. → FR-010a, FR-012, FR-014 and US5 changed: the default no
  longer depends on whether a model keeps up; the hint of FR-011 still offers Pocket TTS where Qwen3-TTS is slow.
- Operator: where Qwen3-TTS is too slow or needs too much memory for Android, Android stays with Pocket TTS. → On
  a phone's processor Qwen3-TTS is far slower than real time (1.3–1.5 s of computing per second of speech on a
  16-core laptop processor) and needs 2.3 GB beside the chat model, so FR-012 and FR-014 changed: Pocket TTS is
  the default on Android; Qwen3-TTS stays selectable there.

### Session 2026-11-06

- Operator: in a conversation the pauses between the parts are far too long; why not let the chat model finish
  its work first and speak afterwards? The approach must hold on any hardware, not be tuned to one laptop. →
  Measured on the dev laptop: next to the chat model on the same processor Qwen3-TTS needed 10 s of computing per
  second of speech (1.35 s alone), Pocket TTS 1.0 (0.30 alone). FR-016 added: the local chat model computes
  first, speech output waits for its compute unit; FR-017 (playback without gaps from the measured speed) follows.
- Operator: the speech model choice lists each model once; an own "Automatisch" row next to the default model
  is redundant. → FR-006 changed: the default model is marked "Standard" in its own row, and choosing it
  removes the device's preference.
- Operator: an installed speech model shows a badge instead of a lasting download colour, and can be deleted
  again. → FR-006a added; the Pocket TTS models stay, as FR-010a keeps them installed.
- Operator (later that day): the pauses between paragraphs and sentences with Qwen3-TTS are still unbearably
  long. → Two gaps in FR-017: a model without a stored measurement played at once, and a held answer could
  start while the chat model was still writing its next paragraph, whose text cannot be computed in advance.
  FR-017 changed: a model not measured yet is held by the pace of its first computed sentence, and a model
  slower than real time waits until the answer is written to its end before the formula decides when to
  start.
- Operator: "noch nicht gemessen" says nothing worth showing. → FR-006: a model shows its speed only once it
  is measured. Delete buttons look the same everywhere (a bin that turns red on hover), and a row without one
  keeps its place, so badges line up.
- Operator: the pause between the two sentences of a voice sample is far too long. → A model slower than real
  time left the time it computed the second sentence as silence; the sample is now made whole before it plays,
  so its sentences are only the normal pause apart (`tts::preview::play`).
- Operator: no "Installiert" badge and no speed mark in the rows; a download icon while a model is missing, a
  delete icon once it is here, and nothing greyed out. → FR-006 and FR-006a changed.
- Operator: Pocket TTS is the default, and the voice assistant must work right away. Shipping Pocket in the
  installer is too big (146–375 MB per language, 1.16 GB for all six), so the onboarding offers it. → FR-012,
  FR-014 and US5 changed: Pocket TTS is the default on every platform; Qwen3-TTS is only spoken when chosen; the
  onboarding ends with a voice step that offers the Pocket model of the interface language (FR-012a).

## Terms

- **Speech model**: the local model that turns text into audio (066 "local voice" engine). After this spec
  there are two kinds: Pocket TTS (small, one model per language) and Qwen3-TTS (large, one model for all
  languages).
- **Voice**: a reference recording the speech model imitates, shipped or own (066 FR-029, FR-049). Voices
  do not belong to a speech model; every voice works with every speech model of its language.
- **Keeps up**: a speech model keeps up on a device when it needs less than 1 second of computing time per
  second of speech there, measured on that device (FR-007).
- **Graphics card acceleration**: running the speech model on a graphics card that haex's llama.cpp build
  supports on this platform (today CUDA on Linux and Windows, Metal on macOS).

## Relation to existing specs

- [`066-voice-assistant`](../066-voice-assistant/spec.md): this spec adds a speech model to 066's local
  voices. Voice choice and preview (FR-029, FR-030), own voices (FR-049, FR-050, FR-053), speaking while the
  answer is written (FR-022), the text front end (FR-022b), spelling secrets (FR-026) and the fallback rules
  (FR-031) apply unchanged to Qwen3-TTS. 066 SC-017 (no foreign accent) applies to Qwen3-TTS as well.
- [`070-llama-cpp-engine`](../070-llama-cpp-engine/spec.md): Qwen3-TTS runs in the llama.cpp engine from
  070, with its build variants (processor, CUDA, Metal) and without a second inference engine. A build
  without llama.cpp (070 FR-005) offers Pocket TTS only.
- [`010-stt-model-choice`](../010-stt-model-choice/spec.md): the choice of speech model follows the same
  pattern as the choice of speech recognition model: size, fit for the device, download on demand.
- [`080-prompt-cache`](../080-prompt-cache/spec.md) and 079 (parked, PR #807): both make the local chat model
  answer faster and both change how much graphics memory it holds. 080 keeps the model's reading in memory
  between answers and counts it in the memory planner (080 FR-008); 079 adds a drafter that only runs when it
  fits next to the model. Qwen3-TTS is a third occupant of the same graphics card. There MUST be one memory
  plan for all of them, not a second one for speech (FR-009a). FR-009 governs who gets the card.

## User Scenarios & Testing _(mandatory)_

### User Story 1 - The assistant speaks with the natural voice (Priority: P1)

Anna has a laptop with a graphics card. On the "Sprachassistent" page she sees Qwen3-TTS as a speech model,
marked "klingt am natürlichsten" with its size of about 2.3 GB and "hält auf diesem Gerät mit". She selects
it, haex downloads it, and from the next answer on the assistant speaks with Lena's voice through Qwen3-TTS.
Clock times and amounts sound as natural as with Pocket, and the first words come as quickly as before.

**Why this priority**: the better voice is the point of this spec. Without it nothing else here has value.

**Independent Test**: on the reference laptop, select Qwen3-TTS, let the download finish, ask the assistant
a question with a time and an amount in the answer. The answer is spoken by Qwen3-TTS with the selected
voice, numbers are spoken as words, and the first audio arrives no later than with Pocket TTS.

**Acceptance Scenarios**:

1. **Given** the speech model choice, **When** the user opens it, **Then** Qwen3-TTS is listed once for all
   languages, with size, licence, the languages it speaks and whether it keeps up on this device.
2. **Given** Qwen3-TTS is not downloaded, **When** the user selects it, **Then** haex downloads it with a
   progress bar, can be used with the previous speech model during the download, and switches to Qwen3-TTS
   once it is complete.
3. **Given** Qwen3-TTS is selected, **When** the assistant answers, **Then** every spoken sentence comes
   from Qwen3-TTS with the selected voice, and haex starts speaking while the answer is still being written
   (066 FR-022).
4. **Given** Qwen3-TTS is selected, **When** the answer contains numbers, times, dates, amounts or
   abbreviations, **Then** haex speaks them as words in the voice's language (066 FR-022b).
5. **Given** Qwen3-TTS is selected, **When** the user presses "Probe" on any voice, **Then** the sample is
   spoken by Qwen3-TTS.
6. **Given** a sentence longer than one part (FR-010), **When** Qwen3-TTS speaks it, **Then** the listener
   hears it in full, with breaks no longer than at a comma, and the first part starts as soon as it is
   ready instead of waiting for the whole sentence.
7. **Given** the assistant speaks a secret, **When** Qwen3-TTS is selected, **Then** haex spells it as 066
   FR-026 requires, and the secret reaches nothing but the local speech model.

---

### User Story 2 - Own voices with Qwen3-TTS (Priority: P1)

Anna recorded her own voice with 066. When she switches to Qwen3-TTS, her own voice stays selected and now
sounds more like her. Her colleague creates a new own voice from an audio file while Qwen3-TTS is selected;
the sample before saving already comes from Qwen3-TTS.

**Why this priority**: own voices are part of 066 and the operator's main reason to compare speech models.
A speech model that cannot imitate them would split the voice list.

**Independent Test**: create an own voice with Pocket TTS, switch to Qwen3-TTS and listen to a sample;
create a second own voice while Qwen3-TTS is selected. Both play with Qwen3-TTS, without re-recording.

**Acceptance Scenarios**:

1. **Given** an own voice created before this spec, **When** the user switches to Qwen3-TTS, **Then** the
   voice works without a new recording.
2. **Given** Qwen3-TTS is selected, **When** the user creates an own voice from a recording or a file,
   **Then** the sample before saving is spoken by Qwen3-TTS, and the checks of 066 FR-049 apply unchanged.
3. **Given** an own voice synced from another device, **When** this device uses Qwen3-TTS, **Then** the
   voice plays with Qwen3-TTS here, whatever speech model the other device uses.

---

### User Story 3 - On a processor without a graphics card (Priority: P2)

Ben's desktop has no supported graphics card, so Qwen3-TTS is slower than real time there. Ben keeps
Qwen3-TTS because he likes the sound. haex speaks the first sentence as soon as it is ready and the following ones with short pauses in
between. After a few answers haex tells him once that Pocket TTS would speak without pauses here.

**Why this priority**: the operator found Qwen3-TTS good on the processor too, but it is slower than real
time there. Users need an honest choice, not a silent stutter.

**Independent Test**: on a device without graphics card acceleration, select Qwen3-TTS and ask for a long
answer: the answer is spoken in full, without lost
words, with pauses between sentences, and the hint appears once.

**Acceptance Scenarios**:

1. **Given** a device where Qwen3-TTS does not keep up, **When** the user opens the speech model choice,
   **Then** Qwen3-TTS is still selectable; the choice shows no speed mark (operator 2026-11-06).
2. **Given** Qwen3-TTS is selected on such a device, **When** an answer of several sentences is spoken,
   **Then** every sentence is spoken completely and in order; between sentences there may be pauses, but a
   sentence is never cut or skipped.
3. **Given** Qwen3-TTS did not keep up in 3 answers in a row, **When** the third answer ends, **Then** haex
   shows once per device that a faster speech model is available, with a button to switch; the hint does
   not come back after the user dismisses it.
4. **Given** the device has a supported graphics card, **When** haex measures the speed, **Then** it uses
   the graphics card, and the hint of scenario 3 does not come while the measurement says it keeps up.

---

### User Story 4 - Pocket TTS with Kyutai's new German model (Priority: P2)

On her phone Anna keeps Pocket TTS, because Qwen3-TTS is too slow there. After an update the German
Pocket voice sounds clearer and speaks faster; she does not have to do anything except confirm the download.

**Why this priority**: phones and weak processors keep Pocket TTS for a long time. Kyutai retrained German
in September 2026; haex still uses the April model.

**Independent Test**: on a device with the old German Pocket model, update haex; haex offers the new model,
downloads it on confirmation, and the same German sample sentence is spoken with the new model, at least as
fast as before.

**Acceptance Scenarios**:

1. **Given** the old German Pocket model is installed, **When** haex starts after the update, **Then** the
   German Pocket entry points to the new model and haex offers the download; until it is done, the old
   model keeps speaking.
2. **Given** the new model is downloaded, **When** it works, **Then** haex removes the old model files.
3. **Given** the new German Pocket model, **When** it speaks the comparison sentences of research R23,
   **Then** it needs no more computing time per second of speech than the old model.

---

### User Story 5 - Pocket TTS is the default, offered in the onboarding (Priority: P3)

Clara sets up haex for the first time. The onboarding's last step offers the Pocket TTS model of her language;
she downloads it, and the voice assistant speaks as soon as she turns it on. Qwen3-TTS is there in the voice
settings for whoever wants the more natural sound and has the hardware for it.

**Why this priority**: most users never open the speech model choice. The default decides what they hear, and
it has to work right away on any device (operator 2026-11-06; the default was Qwen3-TTS from 2026-11-05).

**Independent Test**: on a fresh vault, go through the onboarding and download the voice; turn on the
assistant: the first answer is spoken by Pocket TTS without a further download.

**Acceptance Scenarios**:

1. **Given** no speech model was chosen on this device, **When** the user turns on the assistant or opens
   the voice settings, **Then** the language's Pocket TTS model is the default on every platform, and
   Qwen3-TTS can be chosen where the build runs it.
2. **Given** the onboarding reaches its voice step and the Pocket model of the interface language is not
   installed, **When** the user downloads it, **Then** the assistant speaks right after the onboarding; when
   the user skips, the settings offer the download.
3. **Given** the user chose a speech model, **When** haex later measures that another one would keep up
   better, **Then** haex keeps the user's choice and only gives the hint of US3 scenario 3.

---

### Edge Cases

- **Download interrupted**: the download resumes where it stopped; until it is complete, the previous
  speech model speaks (066 FR-031 fallback).
- **Not enough disk space**: haex refuses the download before it starts, names the missing space and keeps
  the previous speech model.
- **Not enough memory to load**: haex does not load Qwen3-TTS, speaks with Pocket TTS for this answer and
  tells the user once why.
- **Graphics card busy with a local chat model**: see FR-009. The answer is spoken in any case, at worst on
  the processor or with Pocket TTS.
- **The model does not stop talking**: an autoregressive speech model can occasionally repeat or babble
  past the end of its text, more often on long input. haex keeps every part short (FR-010), checks each
  part before it plays, and generates a part that runs far longer than its text allows again (FR-010a).
  Nothing of the failed attempt is heard, and no word is dropped.
- **The model produces silence or noise only**: haex treats it like a part that ran too long (FR-010a).
  If Pocket speaks that failed part instead, haex tells the user once why the selected model fell back.
- **A sentence without any natural break** (a long list of names, a long compound word chain): haex
  splits it between words, never inside a word, a number written out by FR-022b or a spelled secret.
- **Language Qwen3-TTS does not speak**: all six languages of 066 FR-052 are covered. If 066 adds a
  language Qwen3-TTS does not speak, the choice for that language offers only the models that do.
- **Speech rate**: the user's speech rate (066 FR-030) applies to Qwen3-TTS as well (FR-005).
- **Interruption**: when the user interrupts (066 barge-in), Qwen3-TTS stops generating the remaining
  sentences at once; no audio of the interrupted answer plays afterwards.
- **Build without llama.cpp** (070 FR-005) or a platform where llama.cpp is missing: Qwen3-TTS does not
  appear in the choice; a voice preference for it falls back to Pocket TTS.
- **Vault locked while speaking**: speaking stops, and the speech model is unloaded like other local models
  (013).
- **iOS**: not on the roadmap (no Apple developer licence); Metal support is kept possible but not tested.

## Requirements _(mandatory)_

### Functional Requirements

#### Speech model

- **FR-001**: haex MUST offer Qwen3-TTS 1.7B as a local speech model on every platform where its build
  contains the llama.cpp engine (070), as one entry that speaks all six languages of 066 FR-052.
- **FR-002**: Qwen3-TTS MUST run inside haex's existing llama.cpp engine. It MUST NOT add a second
  inference engine, a separate process or a runtime that executes code shipped with the model.
- **FR-003**: Qwen3-TTS MUST be downloaded on demand from a pinned, immutable revision, with its size shown
  before the download, a progress bar, resume after interruption and a check of every file before use, as
  for other local models. Its licence (Apache-2.0) and origin MUST appear in haex's credits.
- **FR-004**: Every shipped voice and every own voice (066 FR-029, FR-049) MUST work with Qwen3-TTS without
  a new recording. The voice preference MUST stay independent of the speech model (066 FR-029).
- **FR-005**: Everything 066 requires of a local voice MUST hold for Qwen3-TTS: speaking while the answer
  is written (FR-022), the text front end (FR-022b), the speech rate setting (FR-030), spelling secrets
  (FR-026, FR-027), the fallback when no voice is available (FR-031), and the voice sample in the voice
  settings and in the own-voice dialog (FR-030, FR-049).

#### Speed and choice

- **FR-006**: The speech model choice MUST show for each model: name, size, licence, languages and how it
  sounds in one plain phrase. Whether a model keeps up on this device is measured (FR-007) but not shown in
  the choice; only the hint of FR-011 uses it (operator 2026-11-06). Models that do not keep up MUST stay
  selectable. Each model MUST appear once; the default (FR-012) is marked "Standard" in its own row, with no
  separate "Automatisch" row, and choosing it removes the preference so the device keeps following the
  default.
- **FR-006a**: A speech model that is not on this device MUST show a download icon; an installed one MUST show a
  delete icon, except Pocket TTS models (FR-010a), which show neither and keep the icon's place so the rows
  line up. Deleting asks first, removes the model's files and its speed measurement; a preference naming it
  stays, and the language's Pocket TTS model speaks until the model is downloaded again (US5).
- **FR-007**: haex MUST measure the speed of the selected speech model on this device from the answers it
  actually speaks (computing time per second of speech, over the last 10 sentences) and keep the result per
  device and per model. A model keeps up when that value is below 1.0.
- **FR-008**: haex MUST use graphics card acceleration for Qwen3-TTS where the build supports it on this
  device, and the processor otherwise. On the processor it MUST use as many cores as the local chat model
  is allowed to use, and never all cores of the device.
- **FR-009**: When a local chat model and Qwen3-TTS both want the graphics card and its memory does not
  hold both, the chat model MUST keep it, and Qwen3-TTS MUST run on the processor for that time. When
  neither fits, haex MUST speak with Pocket TTS rather than delay the answer.
- **FR-009a**: The memory planner of the local chat model (070, 080 FR-008, 079) MUST count Qwen3-TTS
  while it is loaded, and Qwen3-TTS MUST be placed on the graphics card only through that planner. No load
  of a chat model or of Qwen3-TTS may fail or abort haex for lack of graphics memory; when the plan does not
  hold both, FR-009 decides.
- **FR-010**: Before text reaches Qwen3-TTS, haex MUST split every sentence longer than 20 words into
  parts of at most 20 words, at the most natural break available: first at punctuation inside the
  sentence, then before a conjunction, otherwise between words. A split MUST NOT fall inside a word, inside
  a number, time, date or amount written out by 066 FR-022b, or inside a spelled secret. haex MUST speak the
  parts in order, in the same voice, with no break longer than a comma pause between two parts once they
  are ready.
- **FR-010a**: haex MUST check each part before it plays. A part whose audio grows longer than a plausible
  length for its text, or that yields only silence or noise, MUST NOT be played; haex MUST stop generating
  it and generate it again. If the second attempt fails too, haex MUST split that part once more and, should
  that also fail, speak it with Pocket TTS in the same voice. No part may be skipped. Every failed attempt
  MUST be noted in the log with the speech model and the length of the part, never with its text. haex MUST
  keep the Pocket TTS model of the language installed as this reserve, whatever speech model is chosen, and
  download it with any other speech model when it is missing (operator 2026-11-05).
- **FR-011**: When the selected speech model did not keep up in 3 answers in a row and another installed
  or installable model would keep up, haex MUST show a hint once per device with a button to switch. It
  MUST NOT switch on its own.
- **FR-012**: When no speech model was chosen on a device, the language's Pocket TTS model MUST be the default,
  on every platform (operator 2026-11-06; until then Qwen3-TTS was the default outside Android). Qwen3-TTS MUST
  stay selectable wherever the build can run it and MUST speak only when chosen.
- **FR-012a**: The onboarding MUST end with a voice step that offers the Pocket TTS model of the interface
  language, with its size, asking before the download like the steps before (043 FR-028) and skippable; once it
  is downloaded the voice assistant speaks without a further download. The step is left out when the model is
  installed already.

#### Pocket TTS

- **FR-013**: The German Pocket TTS model MUST move to Kyutai's retrained German model (release of
  2026-09-24 or later), exported for haex's Pocket engine and pinned like the other models. Devices with the
  old model MUST keep speaking with it until the new one is downloaded, and haex MUST remove the old files
  afterwards.
- **FR-014**: Pocket TTS MUST stay available on every platform as the small speech model, the default
  (FR-012) and the reserve of FR-010a.
- **FR-015**: The sampling temperature of Pocket TTS MUST follow Kyutai's current recommendation for the
  model in use, unless a listening test of the German comparison sentences shows audibly worse results.

#### Sharing the machine with the chat model

- **FR-016**: Speech output MUST NOT compute while the local chat model computes on the same compute unit (the
  processor, or the graphics card). Before every sentence and every part it waits until the chat model's
  current model call is over; tool calls between model calls do not count, so speech may compute while a tool
  runs. A chat model on another unit, or one of a provider, does not hold speech back; there 066 FR-022 (speak
  while the answer is written) stays as it is. Waiting MUST NOT count as computing time in the speed
  measurement (FR-007), and a stop word or a stop ends the wait at once.
- **FR-017**: Where the selected speech model does not keep up on this device (FR-007), haex MUST wait until the
  answer is written to its end and then start playing it only once, by the measured speed, the rest of it can
  be computed before the audio queued so far has played, so the answer plays without gaps. A model not
  measured yet on this device is held until its first sentence is computed and judged by that sentence's
  speed. A model that keeps up starts with its first sentence, even while the answer is still written.

### Key Entities

- **Speech model entry**: a downloadable speech model with name, engine kind (Pocket TTS or Qwen3-TTS),
  languages, size, licence, pinned origin and files. Qwen3-TTS is one entry for all languages; Pocket TTS
  stays one entry per language.
- **Speed measurement**: per device and per speech model, the computing time per second of speech of the
  last 10 sentences, whether graphics card acceleration was used, and when it was last measured. Device
  local, never synced.
- **Speech model preference**: per device, the chosen speech model per language, or none, so the default
  speaks (FR-012).
  Device local, like other model choices.
- **Hint state**: per device, whether the "faster model available" hint (FR-011) was dismissed.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: On the reference laptop with graphics card acceleration, Qwen3-TTS needs at most 0.5 seconds
  of computing time per second of speech, and the first audio of an answer starts no later than Pocket
  TTS speaking the same first sentence in the same voice on that laptop. Measure both from the point when
  the first sentence is complete; include Qwen3-TTS's required generation and validation of the first
  part before playback.
- **SC-002**: In a blind listening test of the six German sentences of research R23 (066), at least 4 of 5
  German native speakers prefer Qwen3-TTS over Pocket TTS for the same voice.
- **SC-003**: For each of the six languages, at least 4 of 5 native speakers judge Qwen3-TTS free of a
  foreign accent (066 SC-017).
- **SC-004**: An own voice created with 066 is recognised as the same person by at least 3 of 5 listeners
  when spoken by Qwen3-TTS, as 066 SC-016 requires for Pocket TTS.
- **SC-005**: On a device without graphics card acceleration, a 10-sentence answer spoken by Qwen3-TTS
  loses no word and no sentence, and no pause between two sentences exceeds 3 seconds on the reference
  laptop's processor.
- **SC-006**: In 200 spoken sentences, including 20 sentences of more than 40 words, listeners hear no
  babbling, repeated or missing words, and at most 2 parts need a second attempt (FR-010a).
- **SC-007**: While Qwen3-TTS speaks, a local chat model on the same graphics card answers at most 20 %
  slower than without speech output.
- **SC-008**: The new German Pocket TTS model needs at most as much computing time per second of speech as
  the old one on the reference laptop's processor.
- **SC-009**: Switching the speech model in the settings takes effect from the next spoken sentence,
  without a restart (066 FR-030).

## Assumptions

- The llama.cpp version haex pins (llama-cpp-sys-2 0.1.160 at the time of writing) contains the audio
  generation interface Qwen3-TTS needs; the 2026-11-04 spike ran Qwen3-TTS through exactly this version.
  If a later pin bump changes the interface, that bump updates this engine with it.
- The GGUF conversion published by ggml-org (backbone Q8_0 about 1.85 GB, audio part Q8_0 about 0.45 GB)
  is the starting point; the plan decides whether haex mirrors it in its own repository like the Pocket
  models.
- Voice imitation from the voice print alone (without the reference transcript) is good enough; the
  operator confirmed it by ear on 2026-11-04.
- Qwen3-TTS 0.6B is not part of this spec. The plan measures it through llama.cpp on the processor; if it
  keeps up there and sounds clearly better than Pocket TTS, a follow-up spec can add it.
- Replacing sherpa-onnx with llama.cpp for Pocket TTS too (llama.cpp's audio generation also runs Pocket
  TTS) is a later step, not part of this spec. It would leave one engine for chat and speech.
- Release builds may not include graphics card support on every platform. Where they do not, Qwen3-TTS runs
  on the processor and is marked accordingly (US3).
- On Android Pocket TTS is the default (FR-012); Qwen3-TTS can be chosen there and, when it does not keep up,
  the hint of FR-011 offers Pocket TTS again.
- iOS is not on the roadmap.
