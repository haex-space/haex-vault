# Contract: Tauri-Commands und Ereignisse

Alle Commands laufen über `VaultGate` und liefern Fehler als `HolziError` mit `kind`. Ohne Feature
`voice` gibt es Stubs, die `VoiceDisabled` liefern (Muster aus `voice.rs`). Der Aufrufer ist immer
der Nutzer im Fenster; kein Command nimmt eine Identität aus dem Frontend.

## Commands

| Command                       | Eingabe                                                         | Ergebnis                                             | Spec           |
| ----------------------------- | --------------------------------------------------------------- | ---------------------------------------------------- | -------------- |
| `assistant_get_state`         | –                                                               | `AssistantStateWire`                                 | FR-003         |
| `assistant_readiness`         | `{language}`                                                    | `{missing: Missing[]}`                               | FR-005         |
| `assistant_set_enabled`       | `{enabled: bool}`                                               | `AssistantStateWire`                                 | FR-001, FR-005 |
| `assistant_set_muted`         | `{muted: bool}`                                                 | `AssistantStateWire`                                 | US9            |
| `assistant_talk`              | –                                                               | –                                                    | FR-011         |
| `assistant_end_conversation`  | –                                                               | –                                                    | FR-014, FR-039 |
| `assistant_enroll_start`      | `{label: string}`                                               | `{sessionId}`                                        | FR-008         |
| `assistant_enroll_record`     | `{sessionId}`                                                   | `EnrollStep`                                         | FR-008         |
| `assistant_enroll_finish`     | `{sessionId}`                                                   | `EnrollResult`                                       | FR-008         |
| `assistant_enroll_cancel`     | `{sessionId}`                                                   | –                                                    | FR-008         |
| `assistant_wakeword_probe`    | `{active: bool}`                                                | –                                                    | US7            |
| `assistant_tts_preview`       | `{voice: VoiceRef, rate}`                                       | –                                                    | FR-030         |
| `tts_voices_list`             | –                                                               | `VoiceOption[]`                                      | FR-029, FR-053 |
| `list_tts_catalog`            | –                                                               | `TtsCatalogEntry[]` mit Eignung                      | FR-029         |
| `list_installed_tts_voices`   | –                                                               | `string[]`                                           | FR-029         |
| `download_tts_voice`          | `{voiceId}`                                                     | – (Fortschritt als Ereignis)                         | FR-005, FR-029 |
| `tts_clone_record_start`      | –                                                               | `{sessionId}`                                        | FR-049         |
| `tts_clone_record_stop`       | `{sessionId}`                                                   | `CloneCheck`                                         | FR-049         |
| `tts_clone_from_file`         | `{path}`                                                        | `CloneCheck`                                         | FR-049         |
| `tts_clone_preview`           | `{sessionId, language}`                                         | –                                                    | FR-049         |
| `tts_clone_save`              | `{sessionId, name, language, consent: true}`                    | `VoiceOption`                                        | FR-050         |
| `tts_voice_delete`            | `{voice: VoiceRef}`                                             | –                                                    | FR-050, FR-051 |
| `tts_model_inspect`           | `{source: ModelSource}`                                         | `ModelInspection`                                    | FR-051         |
| `tts_model_import`            | `{source, name, languages, licenseNote, rightsConfirmed: true}` | `VoiceOption[]`                                      | FR-051         |
| `assistant_transcript_list`   | `{threadId}`                                                    | `TranscriptEntryWire[]`                              | FR-048         |
| `assistant_transcript_search` | `{query, cursor?, limit?}`                                      | `{entries: {threadId, seq, snippet}[], nextCursor?}` | FR-048         |
| `download_assistant_assets`   | `{language}`                                                    | – (Fortschritt als Ereignis)                         | FR-005         |

`language` ist die Sprache des Fensters; sie bestimmt, welche Stimme bereit sein muss (eine der sechs
Sprachen, sonst Englisch). `download_assistant_assets` lädt, was haex selbst laden kann: das gewählte
Spracherkennungsmodell und das Stimmenmodell dieser Sprache. Das Standard-Wake-Word meldet
`assistant_readiness` als fehlend, bis PR D sein Modell liefert.

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
VoiceRef = "builtin:<id>" | "clone:<id>" | "model:<id>" | "model:<id>#<sprecher>"   # data-model.md
VoiceOption = { ref: VoiceRef, kind: "builtin"|"clone"|"model", name, language,
                installed: bool, sizeBytes, fit, isDefault: bool }
CloneCheck = { sessionId, speechSecs, ok: bool,
               problems: ("tooShort"|"tooLong"|"tooQuiet"|"clipped"|"noisy")[] }
ModelSource = { kind: "folder", path } | { kind: "archive", path } | { kind: "hf", repo, revision? }
ModelInspection = { ok: true, engine, languages: string[], speakers: {id, label}[], sizeBytes }
                | { ok: false, reason: "unknownLayout"|"unsupportedEngine"|"forbiddenFile"|"tooLarge"|"unreachable" }
EnrollStep = { recorded: 1..4, ok: bool, problem: "tooShort"|"noisy"|null }
EnrollResult = { ok: true } | { ok: false, problem: "inconsistent"|"tooShort"|"confusable", similar?: string }
```

Stimmen nachbilden und Modelle importieren gibt es nur über die Oberfläche: Die Commands sind
keine Aktionen und für Agenten (eingebaut, 021, Erweiterungen) nicht aufrufbar, weil sie eine
Einwilligung bzw. ein Nutzungsrecht bestätigen (FR-050, FR-051). Die Aufnahme läuft über den
Duplex-Strom und nur, solange der Dialog offen ist.

## Ereignisse (Rust → Frontend)

| Ereignis                      | Nutzlast                                  | Spec           |
| ----------------------------- | ----------------------------------------- | -------------- |
| `assistant-state`             | `AssistantStateWire`                      | FR-003, FR-041 |
| `assistant-transcript`        | `TranscriptEntryWire` (unten)             | FR-042, FR-047 |
| `assistant-tone`              | `{tone: ToneId}` (sichtbare Entsprechung) | FR-044         |
| `assistant-error`             | `{kind, message}`                         | Edge Cases     |
| `assistant-download-progress` | `{id, received, total}`                   | FR-005         |
| `assistant-wakeword-probe`    | `{detected: bool, score}`                 | US7            |
| `tts-clone-level`             | `{sessionId, level, speechSecs}`          | FR-049         |
| `tts-voices-changed`          | –                                         | FR-050         |

`ToneId` = `listening`, `understood`, `working`, `question`, `done`, `error`, `aborted`,
`notUnderstood`, `conversationEnd`, `attention`, `secret`, `modeOn`, `modeOff`, `muted`, `unmuted`.

`assistant_transcript_search` defaults to 50 entries and accepts `limit` from 1 to 100. Results
have a stable ascending order by `(threadId, seq, id)`; the opaque `nextCursor` carries the last
tuple and resumes strictly after it. The server applies the limit before returning entries.

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
| `audioRoute`                             | `"speaker" \| "headphones" \| "bluetooth"`                      |
| Ereignis `assistant-notification-action` | `{action: "off" \| "mute" \| "unmute"}`                         |
| Ereignis `audio-focus`                   | `{lost: bool}` (Telefonat, exklusive Nutzung; FR-036)           |
