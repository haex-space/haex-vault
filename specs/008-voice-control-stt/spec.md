# Feature Specification: Voice Control (Local Speech-to-Text)

**Feature Branch**: `008-voice-control-stt`
**Created**: 2026-09-16
**Status**: Draft
**Input**: User description: "Voice control: local push-to-talk recording in the chat view,
transcribed locally via a bundled speech-to-text model (or a user-configured external
transcription service), transcript written into the chat input field with a configurable
auto-send (default on). Three fixed words (STOP, HALT, ABBRECHEN) are matched locally against the
transcript and immediately interrupt a running agent turn, bypassing the assistant entirely. No
new voice-triggered app-control commands beyond these three words — everything else rides the
existing chat/agent pipeline unchanged. Transcription is added as a new capability on the existing
provider abstraction (which already offers local and external options for chat), rather than a
parallel system." Full design write-up: `docs/plans/2026-09-16-voice-control-stt-design.md`.

## Clarifications

### Session 2026-09-16

- Q: Should raw audio recordings be persisted anywhere (e.g. for debugging/history), or strictly
  transient? → A: Strictly transient — audio is held only in memory during recording and
  transcription, and is discarded immediately after transcription completes, regardless of
  outcome.
- Q: When an external (non-local) transcription service is active, does the chat view need an
  ongoing visible signal that audio is leaving the device, beyond the settings screen where it was
  configured? → A: Yes — the recording control itself must visibly indicate, for as long as the
  external service is active, that voice audio is sent off-device.

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Dictate a chat message (Priority: P1)

A user in the chat view presses a microphone control to start recording and speaks a message.
While the recording runs, the microphone control gives way to two icon-only controls: cancel and
send. Pressing send ends the recording; the spoken words then appear as text in the chat message
field and — unless the user has turned automatic sending off — are sent to the assistant just as
if typed and submitted. Pressing cancel discards the recording instead, and nothing is
transcribed, written to the input field or sent.

**Why this priority**: This is the entire point of the feature. Without it, there is nothing to
ship.

**Independent Test**: Press the microphone control, speak a short sentence, press the send
control. Verify the sentence appears in the input field and (with default settings) is sent as a
chat message.

**Acceptance Scenarios**:

1. **Given** the chat view is open and idle, **When** the user records a short spoken sentence via
   the microphone control, **Then** the transcribed text appears in the chat input field.
2. **Given** automatic sending is enabled (the default), **When** a recording finishes
   transcribing, **Then** the message is sent through the same path as a manually typed and
   submitted message.
3. **Given** automatic sending is disabled, **When** a recording finishes transcribing, **Then**
   the text remains in the input field, editable, until the user sends it manually.
4. **Given** a recording captured only silence or background noise, **When** transcription
   completes, **Then** the input field is left unchanged and nothing is sent.
5. **Given** a recording is in progress, **When** the user looks at the chat controls, **Then** the
   microphone control is not shown and only an icon-only cancel control and an icon-only send
   control are offered for the recording.
6. **Given** a recording is in progress, **When** the user presses cancel, **Then** the recording
   is discarded, nothing is transcribed, and nothing is written to the input field or sent.

---

### User Story 2 - Interrupt the assistant by voice (Priority: P1)

While the assistant is generating a response, the user speaks one of a small set of fixed words
("stop", "halt", "abbrechen") to immediately stop it, without waiting for the assistant to finish
or for anything else to process first.

**Why this priority**: Being able to stop an in-progress response is a basic control expectation,
and doubly so once voice is the primary input method — a user with their hands off the keyboard
still needs a way to interrupt what's happening. This must work reliably and fast, independent of
how busy the assistant currently is.

**Independent Test**: While the assistant is generating a response, record one of the three
interrupt words. Verify the response stops immediately.

**Acceptance Scenarios**:

1. **Given** the assistant is generating a response, **When** the user speaks only "stop" (or
   "halt" / "abbrechen"), **Then** the response generation stops immediately.
2. **Given** the assistant is generating a response, **When** the user dictates a longer sentence
   that happens to contain one of the interrupt words as part of normal speech (e.g. "please stop
   using bullet points"), **Then** the response is **not** interrupted and the full sentence is
   instead treated as a normal chat message per User Story 1.
3. **Given** the assistant is idle (not generating), **When** the user speaks an interrupt word,
   **Then** nothing is interrupted (there is nothing running), and no message is sent.

---

### User Story 3 - Choose a transcription source (Priority: P2)

A user configures which transcription capability to use: the bundled option that works fully
offline, or an external transcription service they provide credentials for — the same way they
already choose between a local and an external option for the chat assistant itself.

**Why this priority**: Important for users who want higher accuracy or different language
coverage than the bundled option provides, but not required for the feature to deliver value on
day one — the bundled option must work well on its own.

**Independent Test**: Add an external transcription service with valid credentials, mark it
active, dictate a message, and confirm it is transcribed via that service instead of the bundled
one.

**Acceptance Scenarios**:

1. **Given** no external transcription service is configured, **When** the user records a
   message, **Then** transcription happens using the bundled, fully offline capability.
2. **Given** the user has added and activated an external transcription service, **When** the
   user records a message, **Then** transcription is performed via that service, and the recording
   control visibly indicates that audio is being sent off-device for as long as that service stays
   active.
3. **Given** an external transcription service is active but its credentials are invalid or it is
   unreachable, **When** the user records a message, **Then** the user sees a clear error and
   nothing is sent or written to the input field.
4. **Given** an external transcription service was active, **When** the user switches back to the
   bundled option, **Then** subsequent recordings are transcribed offline again without needing to
   reconfigure anything else.

---

### Edge Cases

- What happens when microphone access has not been granted, or was revoked after being granted
  earlier? The control clearly indicates that access is needed and does not silently fail.
- What happens if the user starts a new recording while a previous one is still being
  transcribed? The system does not allow overlapping recordings; the control reflects that it is
  busy.
- What happens if the app loses focus or is sent to the background while a recording is in
  progress? The in-progress recording is discarded, not transcribed.
- What happens if a recording runs unusually long (the user forgets to stop it)? The
  system stops recording automatically after a fixed maximum duration and transcribes what was
  captured.
- What happens on the very first run after installation, before the user has configured anything?
  Voice dictation works immediately using the bundled transcription capability, once microphone
  access is granted — no additional download or setup step is required.

## Requirements _(mandatory)_

### Functional Requirements

- **FR-001**: Users MUST be able to start a voice recording via a dedicated control in the chat
  view, and to end it either by sending it or by cancelling it (FR-022, FR-023).
- **FR-002**: The system MUST transcribe a completed recording into text.
- **FR-003**: Unless an external transcription service has been explicitly configured and
  activated, transcription MUST happen without the recorded audio leaving the device.
- **FR-004**: The system MUST place the transcript into the chat message input field.
- **FR-005**: The system MUST provide a setting that controls whether a transcribed message is
  sent automatically, defaulting to enabled.
- **FR-006**: When automatic sending is disabled, the transcript MUST remain in the input field,
  editable, until the user sends it manually.
- **FR-007**: The system MUST recognize an utterance consisting solely of "stop", "halt", or
  "abbrechen" (case-insensitive) as an interrupt command, distinct from normal dictated content.
- **FR-008**: Recognizing an interrupt command MUST stop an in-progress assistant response
  immediately, and this recognition MUST NOT depend on the assistant itself being available or
  idle.
- **FR-009**: An utterance that contains an interrupt word only as part of a longer sentence MUST
  be treated as normal dictated content (User Story 1), not as an interrupt command.
- **FR-010**: The system MUST offer a bundled transcription capability that requires no user setup
  or download beyond granting microphone access. **Deviation (2026-09-17, operator decision,
  see tasks.md's implementation note)**: the current implementation downloads the model on first
  use instead, to avoid committing a large binary checkpoint into git history. SC-003 below carries
  the same deviation.
- **FR-011**: The system MUST size the bundled transcription capability appropriately for the
  device it runs on (e.g., a lighter-weight version on constrained/mobile hardware than on
  desktop hardware), consistent with how locally run assistant models are already sized per
  device. **Not implemented**: every device currently gets the same `tiny` model — see tasks.md's
  implementation note.
- **FR-012**: The system MUST let users configure an external transcription service (with its own
  credentials) as an alternative to the bundled capability, and switch between them.
- **FR-013**: The system MUST request microphone access through the platform's standard permission
  mechanism and visibly indicate when access is missing or denied.
- **FR-014**: If a recording contains no detectable speech, the system MUST leave the chat input
  field and message history unchanged.
- **FR-015**: If transcription fails for any reason (local failure, or an external service being
  unreachable or rejecting credentials), the system MUST inform the user and MUST NOT send or
  write anything to the input field.
- **FR-016**: The system MUST discard, rather than transcribe, a recording that was in progress
  when the app left the foreground.
- **FR-017**: The system MUST cap the maximum duration of a single recording, automatically
  stopping and transcribing once the cap is reached.
- **FR-018**: The voice recording control MUST only be available while the chat view is active and
  focused; no system-wide or background trigger is provided.
- **FR-019**: Recognized voice input MUST NOT trigger any action beyond what is described above
  (sending a chat message, or the three interrupt commands) — no other app functionality is
  controlled directly by voice as part of this feature.
- **FR-020**: The system MUST NOT persist raw recorded audio to disk or any other storage; audio
  MUST be discarded immediately after transcription completes, whether it succeeds or fails.
- **FR-021**: Whenever an external transcription service is the active source, the recording
  control MUST visibly indicate this for as long as it remains active — not only at the point the
  service was configured.
- **FR-022**: While a recording is in progress, the microphone control MUST give way to exactly two
  controls, one to cancel and one to send, and both MUST be icon-only: they carry no text label,
  and their meaning is exposed through an accessible name and a tooltip. The chat view's other
  send and stop-response controls MUST NOT be offered while the recording runs.
- **FR-023**: Cancelling a recording MUST discard it without transcribing it, and MUST NOT send or
  write anything to the input field.
- **FR-024**: Sending a recording MUST end it and transcribe it; the transcript is then handled as
  FR-004 to FR-006 describe (sent when automatic sending is enabled, otherwise placed in the input
  field).

### Key Entities

- **Transcription Source**: Represents where speech-to-text happens — either the bundled offline
  capability or a user-configured external service. Exactly one is active at a time. An external
  source carries its own credentials, following the same handling already used for other external
  service credentials in the app.
- **Interrupt Command**: One of a small, fixed set of recognized words ("stop", "halt",
  "abbrechen") that, when spoken as the entire utterance, immediately stops an in-progress
  assistant response.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: A user can dictate and send a typical one-sentence chat message using only the
  microphone control, with the transcribed text appearing in the input field within 5 seconds of
  pressing send.
- **SC-002**: The bounded local interrupt path recognizes an exact interrupt utterance at its
  end-of-utterance boundary and cancels an in-progress assistant response within one second of the
  final local audio frame, regardless of the selected transcription provider or what the assistant
  is doing. Full-utterance transcription may complete later and must not delay that cancellation.
- **SC-003**: Voice dictation works correctly on a freshly installed instance, using only the
  bundled transcription capability, with no additional download or configuration beyond granting
  microphone access. **Deviation**: see FR-010's note — the first dictation after a fresh install
  triggers a one-time model download instead.
- **SC-004**: A short dictated utterance is transcribed correctly without any network activity
  when no external transcription service is configured.
- **SC-005**: A user can switch between the bundled and an external transcription source, and
  back, without needing to reconfigure any other voice-control setting.
- **SC-006**: An utterance that merely contains an interrupt word within a longer sentence is
  never mistakenly treated as an interrupt command.

## Assumptions

- Voice input is scoped to the chat view only; it is a new input method for existing chat and
  interrupt actions, not a general app-control mechanism. Controlling other parts of the app by
  voice is out of scope and expected to happen later, if at all, through the assistant's own
  existing tool-use capability rather than through new voice-specific commands.
- Activation is explicit only: the user starts every recording with the microphone control and
  ends it with send or cancel; there is no wake word and no always-listening mode. Recording is a
  toggle, not press-and-hold, on desktop and mobile alike.
- The feature targets both desktop and mobile platforms from the start. On all platforms, voice
  input requires the app to be in the foreground.
- The three interrupt words are fixed for this feature and are not user-customizable.
- The bundled transcription capability is shipped with the app; it is not something the user
  downloads or selects a model for, unlike the existing free-form model search available for chat
  models.
- Exact device-tier sizing of the bundled transcription capability, and the precise mechanics of
  adding it as a capability on the existing provider system, are implementation decisions for the
  planning phase, not this specification.

## Implementation amendment (2026-11-06)

The shipped local transcription engine is Parakeet through sherpa-onnx (spec 066). Whisper and the Candle implementation described in the original research and plan have been removed; external transcription remains a separate provider capability.
