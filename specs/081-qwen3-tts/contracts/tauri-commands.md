# Contract: speech model commands and events

Extends spec 066's speech commands (`specs/066-voice-assistant/contracts/tauri-commands.md`). Existing commands
keep their names; only the fields below change. Wire names are camelCase.

## `tts_speech_models(language: string) -> SpeechModelOption[]` (new)

The speech models that speak `language` in this build, best `qualityRank` first.

```ts
interface SpeechModelOption {
  id: string // catalog id
  name: string
  engine: 'pocket' | 'qwen3-tts'
  languages: string[]
  license: string
  sizeBytes: number
  installed: boolean
  fit: Fit // hardware fit as for chat and STT models
  speed: 'keepsUp' | 'slow' | 'unknown' // FR-006, for the acceleration this device would use now
  ratio: number | null // seconds of computing per second of speech, null when unknown
  acceleration: 'gpu' | 'cpu' // where it would run now (ledger answer for Qwen3-TTS, always cpu for Pocket)
  chosen: boolean // named by assistant.speechModel.<language>
  isDefault: boolean // what speaks without a choice: Qwen3-TTS where the build runs it (FR-012)
}
```

- Exactly one entry has `isDefault: true` per language; at most one has `chosen: true`.
- `speed` is `unknown` with fewer than 3 measured sentences (data-model.md).
- The settings page writes the choice with the existing `set_pref` (device scope), key
  `assistant.speechModel.<language>`, saved on selection; choosing a model that is not installed starts the same
  download question as a voice today.

## `tts_voices_list() -> VoiceOption[]` (changed)

`modelId`, `installed`, `sizeBytes` and `fit` now describe the speech model that would speak the voice on this
device: the chosen one for the voice's language, or the default (`tts::choice::Resolved::wanted`). Nothing
else changes.

## `download_tts_voice(voiceId)` (unchanged name)

Accepts `qwen3-tts-1.7b` like any catalog id. For entries with `files`, each file is checked against its SHA-256
after the download; a mismatch deletes the file and fails with `TtsError::Unavailable` ("die Datei … ist
beschädigt"), shown as a toast. Progress events stay `model-download-progress`.

## `assistant_tts_preview(voice, …)` (unchanged signature)

Speaks with the speech model `tts_voices_list` names for the voice. The own-voice dialog's sample uses the same
resolution (US2).

## Event `tts-speech-model-hint` (new)

```ts
interface SpeechModelHint {
  language: string
  slowModelId: string // the chosen model that did not keep up in 3 answers in a row
  fasterModelId: string // the language's Pocket model, always installed (FR-010a)
}
```

Emitted at most once per device (FR-011), only when `assistant.speechModelHint` is not `dismissed`. The frontend
shows it as a toast with "Wechseln" (writes `assistant.speechModel.<language>`) and "Nicht mehr anzeigen" (writes
`assistant.speechModelHint = dismissed`); closing the toast also counts as dismissed. haex never switches by itself.

## Event `tts-speech-model-fallback` (new)

```ts
interface SpeechModelFallback {
  reason: 'memory' | 'notInstalled' | 'loadFailed' | 'partFailed'
  modelId: string
}
```

Emitted when an answer is spoken with Pocket TTS instead of the chosen model: the model was not installed, could
not load, lacked memory, or failed a part's speech checks. For a failed part, the reserve notice is emitted at
most once per conversation, including when another fallback reason was already reported. The frontend shows a
toast that names the reason.
