# Voice Control (Local STT) — Design

**Status**: Draft, from a brainstorming session on 2026-09-16. Input to the formal spec under
`specs/` (Spec Kit).

**Relationship to existing documents**:

- Builds directly on the agent tool loop (`specs/003-agent-tool-loop/`) — dictated text lands in
  the existing chat send path and is handled by the existing tool loop unchanged. This design adds
  no new agent tools.
- Reuses the hardware-tiering logic introduced for local chat models
  (`specs/002-onboarding-model-prefs/`) to pick a bundled STT model size per device class.
- Extends the existing `Provider`/`ProviderAdapter` abstraction (`src-tauri/src/providers/`,
  `src-tauri/src/adapters/`) with a transcription capability, rather than building a parallel
  provider system.

---

## 1. Scope

Voice control means: the user records speech locally, it is transcribed to text locally (or via a
user-configured external service), and that text drives the existing chat/agent pipeline exactly as
if it had been typed. This spec covers the **voice input pipeline** — capture, transcription,
routing, and a small fixed set of interrupt words. It explicitly does **not** cover:

- New voice-triggered UI actions (switch model, open settings, navigate) beyond the interrupt
  words. Controlling the rest of the app by voice is expected to happen later, transitively,
  through whatever tools the agent has (today's tool loop, and eventually app functionality
  exposed as MCP/ACP tools per haex-vault's stated direction) — not through new voice-specific command
  plumbing built here.
- Wake-word / always-listening activation. Activation is push-to-talk only.
- A user-facing model picker/download flow for the local STT model. It ships bundled with the app.

## 2. Activation & platforms

Push-to-talk (press/hold or toggle a mic control in the chat view). No wake word — the user
explicitly starts and stops recording. Normal dictation uses full-utterance STT after release; a
small, local interrupt recognizer runs alongside capture so the three fixed interrupt words do not
wait for the selected provider or a network round trip. It observes only the bounded recording,
requires an end-of-utterance boundary, and cancels the active turn within one second of receiving
the final local audio frame.

In scope for both desktop and mobile (iOS/Android) from the start. Voice control is a
foreground-only feature: if the app is backgrounded mid-recording (most relevant on mobile), the
in-progress recording is discarded rather than transcribed — consistent with how haex-vault already
treats backgrounding as a normal presence state, not an error. No system-wide global hotkey; the
mic control is only reachable while the app is focused.

## 3. Architecture

```
[Mic control, chat view]
      │ press/hold
      ▼
[Audio capture (cpal, in-process)] ──► canonical PCM buffer, held in memory, capped duration
      ├─ local bounded interrupt path (runs during capture)
      │     └─ exact {STOP, HALT, ABBRECHEN} after end-of-utterance
      │           → backend cancels the running agent turn immediately
      │
      └─ release → [STT provider: local (candle Whisper, bundled) or external API]
                         │
                         └─ no interrupt → write transcript into the chat input field
                       └─ auto-send setting (default: on) → existing send-message path,
                                                              same gating as typed input
```

The interrupt path is deliberately separate from full-utterance transcription. A bounded local
matcher receives the canonical capture frames while recording, confirms an exact whole-utterance
match at the local end-of-utterance boundary, and lets the backend trigger the existing
`CancellationToken` without waiting for the selected STT provider. The normal STT result still
uses the same matcher for its `interrupt` wire field and is coalesced with the fast-path result;
external-provider latency or failure can therefore affect dictation, but never delays an already
recognized interrupt. Nothing about "controlling the app" is built beyond this: everything past
"text is in the input field" is the existing, unmodified chat/agent pipeline.

## 4. STT as a provider capability

HaexVault already has a `Provider`/`ProviderAdapter` abstraction for chat models: `ProviderKind`
(`local` / `api_key` / `cli_delegate`), one adapter per vendor, credential storage, and a
"Modelle verwalten" management UI. Rather than building a separate parallel system for STT, this
design extends that abstraction with a capability dimension (`chat` | `transcription`):

- Same `Provider` storage row and CRUD commands serve both capabilities. The existing `adapter`
  discriminator string continues to distinguish vendors within a capability.
- A narrower adapter trait for transcription — `transcribe(audio: &CanonicalPcm) -> Result<String>` — sits
  alongside the existing `ProviderAdapter` (which is shaped around chat: streaming, context
  window). Forcing STT through `ProviderAdapter` as-is would mean fake/unused fields; a dedicated
  narrower trait avoids that.
- Every adapter receives the same canonical audio contract: `CanonicalPcm` contains normalized
  `f32` samples in `[-1.0, 1.0]`, mono, at exactly 16,000 Hz. The audio module converts each
  device's native sample type, channel layout, and sample rate at the capture boundary. The
  canonical value stays backend-local and in memory; the Tauri command contract refers to this
  type rather than exposing an ambiguous raw buffer. `ExternalSttAdapter` converts it once at its
  HTTP boundary to 16-kHz mono signed-16-bit PCM WAV for the multipart request; the local Whisper
  adapter consumes the canonical `f32` samples directly.
- `LocalWhisperAdapter`: candle-transformers Whisper, running on the same candle runtime already
  pulled in by `mistralrs` for local chat inference (`candle-core`/`candle-nn`, already in
  `src-tauri/Cargo.lock`). Pure Rust — no FFI, no separate cross-compilation path for iOS/Android
  the way `whisper-rs` (whisper.cpp bindings) or `sherpa-onnx` would need. Model checkpoint is
  bundled with the app, not user-downloaded; tier (tiny/base/small) is chosen via the existing
  hardware-detection logic from spec 002 (mobile/low-memory → smaller tier, desktop → larger).
- `ExternalSttAdapter`: HTTP-based, vendor discriminator + `base_url` + `api_key`, reusing the same
  credential storage path chat `api_key` providers already use.
- A default local transcription provider row always exists (bundled, not user-removable), mirroring
  how a default local chat provider already exists. The user can additionally configure an external
  transcription provider and choose which is active.

## 5. Settings & UX

- Mic control in the chat view, next to the send button. Visual states: idle, recording,
  transcribing, error.
- Settings: active STT provider (local vs. configured external), auto-send toggle (default: on).
  These likely live in the same "Modelle verwalten" area now that it serves both capabilities.
- Auto-send off means the transcript lands in the input field for review/edit, same as if typed;
  the user sends manually. Auto-send on means it goes through the existing send path immediately
  after transcription — same gating rules as manually typed + Enter (e.g. disabled while the agent
  is already generating).

## 6. Error handling & edge cases

- **Mic permission denied**: inline error, mic control shows a "permission needed" state.
- **Empty/no speech detected**: input field untouched, no auto-send, non-blocking indicator
  ("keine Sprache erkannt").
- **STT failure** (local model error, or external provider HTTP failure/timeout/invalid
  credentials): surfaced inline; nothing is written to the input field or sent. External-provider
  errors reuse the existing `AdapterError` taxonomy (e.g. `InvalidCredentials` from 401/403) rather
  than inventing new variants.
- **PTT while the agent is already generating**: normal dictation behaves exactly like typing
  during generation today — no new state machine. The three interrupt words are the sole
  exception: they fire immediately regardless of turn state, since interrupting is their entire
  purpose.
- **App backgrounded mid-recording**: recording is discarded, not transcribed.
- **Max recording length**: capped (e.g. ~60s) to bound memory on mobile; hitting the cap
  auto-stops and transcribes what was captured so far rather than growing unbounded.

## 7. Testing

- Interrupt-word matcher: pure function, deterministic unit tests (normalization, exact-match
  boundaries — e.g. German "stoppen" must not match "STOP").
- `LocalWhisperAdapter`: tests against a small fixed audio fixture with a known expected
  transcript. No real microphone, no network.
- `ExternalSttAdapter`: HTTP boundary mocked, verifying request shape and error-code mapping —
  same pattern as the existing `AnthropicAdapter` tests.
- Frontend: mic control state transitions and the auto-send toggle as component-level tests, not
  full end-to-end.
- Nothing in the test suite touches real microphone hardware or makes a real external network
  call.

## 8. Open items for the formal spec

- Exact bundled Whisper tier(s) and their size/latency numbers per device class.
- Exact schema change to `Provider`/`ProviderKind` for the capability dimension (migration
  shape).
- Whether an external transcription provider needs its own vendor catalog (e.g. OpenAI, a
  generic OpenAI-compatible `/v1/audio/transcriptions` endpoint) or starts with a single vendor.
- Concrete acceptance scenarios and priorities (P1/P2/...) per Spec Kit's user-story format.
