# Contract: Tauri-Commands und Ereignisse

Alle Commands laufen über `VaultGate` und liefern Fehler als `HolziError` mit `kind`. Ohne Feature
`voice` gibt es Stubs, die `VoiceDisabled` liefern (Muster aus `voice.rs`). Der Aufrufer ist immer
der Nutzer im Fenster; kein Command nimmt eine Identität aus dem Frontend.

## Commands

| Command                       | Eingabe            | Ergebnis                        | Spec           |
| ----------------------------- | ------------------ | ------------------------------- | -------------- |
| `assistant_get_state`         | –                  | `AssistantStateWire`            | FR-003         |
| `assistant_readiness`         | –                  | `{missing: Missing[]}`          | FR-005         |
| `assistant_set_enabled`       | `{enabled: bool}`  | `AssistantStateWire`            | FR-001, FR-005 |
| `assistant_set_muted`         | `{muted: bool}`    | `AssistantStateWire`            | US9            |
| `assistant_talk`              | –                  | –                               | FR-011         |
| `assistant_end_conversation`  | –                  | –                               | FR-014, FR-039 |
| `assistant_enroll_start`      | `{label: string}`  | `{sessionId}`                   | FR-008         |
| `assistant_enroll_record`     | `{sessionId}`      | `EnrollStep`                    | FR-008         |
| `assistant_enroll_finish`     | `{sessionId}`      | `EnrollResult`                  | FR-008         |
| `assistant_enroll_cancel`     | `{sessionId}`      | –                               | FR-008         |
| `assistant_wakeword_probe`    | `{active: bool}`   | –                               | US7            |
| `assistant_tts_preview`       | `{voiceId, rate}`  | –                               | FR-030         |
| `list_tts_catalog`            | –                  | `TtsCatalogEntry[]` mit Eignung | FR-029         |
| `list_installed_tts_voices`   | –                  | `string[]`                      | FR-029         |
| `download_tts_voice`          | `{voiceId}`        | – (Fortschritt als Ereignis)    | FR-005, FR-029 |
| `assistant_transcript_list`   | `{threadId}`       | `TranscriptEntryWire[]`         | FR-048         |
| `assistant_transcript_search` | `{query, cursor?}` | `{threadId, seq, snippet}[]`    | FR-048         |
| `download_assistant_assets`   | –                  | – (Fortschritt als Ereignis)    | FR-005         |

Einstellungen, die nur gespeichert werden, laufen über die vorhandenen `set_pref`/`get_pref` mit
`PrefScope::Device`; der Assistent beobachtet die Schlüssel und übernimmt Änderungen ohne Neustart
(FR-030).

Typen:

```text
AssistantStateWire {
  enabled: bool, muted: bool, paused: bool,
  phase: "off"|"ready"|"listening"|"thinking"|"working"|"speaking"|"followUp"|"awaitingAnswer",
  threadId: string|null,
  externalTts: bool, externalStt: bool,           # FR-031, FR-032
  echoMode: "full"|"keywordOnly",
  openQuestion: { kind: "approval"|"choice"|"screen", text: string } | null
}
Missing = "sttModel"|"vadModel"|"wakeWordModel"|"voice"|"chatModel"|"microphone"
EnrollStep = { recorded: 1..4, ok: bool, problem: "tooShort"|"noisy"|null }
EnrollResult = { ok: true } | { ok: false, problem: "inconsistent"|"tooShort"|"confusable", similar?: string }
```

## Ereignisse (Rust → Frontend)

| Ereignis                      | Nutzlast                                  | Spec           |
| ----------------------------- | ----------------------------------------- | -------------- |
| `assistant-state`             | `AssistantStateWire`                      | FR-003, FR-041 |
| `assistant-transcript`        | `TranscriptEntryWire` (unten)             | FR-042, FR-047 |
| `assistant-tone`              | `{tone: ToneId}` (sichtbare Entsprechung) | FR-044         |
| `assistant-error`             | `{kind, message}`                         | Edge Cases     |
| `assistant-download-progress` | `{id, received, total}`                   | FR-005         |
| `assistant-wakeword-probe`    | `{detected: bool, score}`                 | US7            |

`ToneId` = `listening`, `understood`, `working`, `question`, `done`, `error`, `aborted`,
`notUnderstood`, `conversationEnd`, `attention`, `secret`, `modeOn`, `modeOff`, `muted`, `unmuted`.

`TranscriptEntryWire` = `{threadId, seq, spokenAt, speaker ("user" oder "haex"), kind, text,
spokenUntil?, outcome?, final}`; `final: false` für Text, der noch entsteht. Die gespeicherte
Mitschrift liest `assistant_transcript_list {threadId}`.

Die vorhandenen Chat-Ereignisse (`chat-token`, `chat-tool-call`, `tool-permission-request`,
`chat-choice-request`, `chat-turn-complete` …) gehen bei Sprachgesprächen unverändert an die
Oberfläche. `assistant-transcript` trägt nie ein Geheimnis.

## Android-Plugin (`holzi-android`)

| Methode                                  | Wirkung                                                         |
| ---------------------------------------- | --------------------------------------------------------------- |
| `startAssistantService`                  | Vordergrund-Dienst mit Benachrichtigung; nur aus sichtbarer App |
| `stopAssistantService`                   | Dienst beenden                                                  |
| `updateAssistantNotification`            | `{phase, muted}` in der Benachrichtigung                        |
| `audioRoute`                             | `"speaker"                                                      | "headphones" | "bluetooth"` |
| Ereignis `assistant-notification-action` | `{action: "off"                                                 | "mute"       | "unmute"}`   |
| Ereignis `audio-focus`                   | `{lost: bool}` (Telefonat, exklusive Nutzung; FR-036)           |
