# Contract: Tauri-Commands und Ereignisse

Alle Commands laufen über `VaultGate` und liefern Fehler als `HolziError` mit `kind`. Ohne Feature
`voice` gibt es Stubs, die `VoiceDisabled` liefern (Muster aus `voice.rs`). Der Aufrufer ist immer
der Nutzer im Fenster; kein Command nimmt eine Identität aus dem Frontend.

## Commands

| Command                              | Eingabe                                                         | Ergebnis                                                 | Spec            |
| ------------------------------------ | --------------------------------------------------------------- | -------------------------------------------------------- | --------------- |
| `assistant_get_state`                | –                                                               | `AssistantStateWire`                                     | FR-003          |
| `assistant_readiness`                | `{language}`                                                    | `{missing: Missing[]}`                                   | FR-005          |
| `assistant_set_enabled`              | `{enabled: bool, language, utcOffsetMinutes}`                   | `AssistantStateWire`                                     | FR-001, FR-005  |
| `assistant_set_muted`                | `{muted: bool}`                                                 | `AssistantStateWire`                                     | US9             |
| `assistant_end_conversation`         | –                                                               | –                                                        | FR-014, FR-039  |
| `assistant_enroll_start`             | `{label: string}`                                               | `{sessionId}`                                            | FR-008          |
| `assistant_enroll_record`            | `{sessionId}`                                                   | `EnrollStep`                                             | FR-008          |
| `assistant_enroll_finish`            | `{sessionId}`                                                   | `EnrollResult`                                           | FR-008          |
| `assistant_enroll_cancel`            | `{sessionId}`                                                   | –                                                        | FR-008          |
| `assistant_wakeword_probe`           | `{active: bool}`                                                | –                                                        | US7             |
| `assistant_tts_preview`              | `{voice: VoiceRef, rate}`                                       | –                                                        | FR-030          |
| `assistant_tone_preview`             | `{tone: ToneId, volume, customId?}`                             | –                                                        | FR-045, FR-045a |
| `assistant_tones_list`               | –                                                               | `{custom: CustomTone[], assigned: {ToneId: id}}`         | FR-045a         |
| `assistant_tone_import`              | `{file: PickedFile}`                                            | `CustomTone`                                             | FR-045a         |
| `assistant_tone_assign`              | `{tone: ToneId, customId: string \| null}`                      | –                                                        | FR-045a         |
| `assistant_tone_delete`              | `{customId}`                                                    | –                                                        | FR-045a         |
| `speech_credits`                     | –                                                               | `SpeechCredit[]`                                         | FR-046          |
| `tts_voices_list`                    | –                                                               | `VoiceOption[]`                                          | FR-029, FR-053  |
| `list_tts_catalog`                   | –                                                               | `TtsCatalogEntry[]` mit Eignung                          | FR-029          |
| `list_installed_tts_voices`          | –                                                               | `string[]`                                               | FR-029          |
| `download_tts_voice`                 | `{voiceId}`                                                     | – (Fortschritt als Ereignis)                             | FR-005, FR-029  |
| `tts_clone_record_start`             | –                                                               | `{sessionId}`                                            | FR-049          |
| `tts_clone_record_stop`              | `{sessionId}`                                                   | `CloneCheck`                                             | FR-049          |
| `tts_clone_from_file`                | `{file}`                                                        | `CloneCheck`                                             | FR-049          |
| `tts_clone_preview`                  | `{sessionId, language, rate?}`                                  | –                                                        | FR-049          |
| `tts_clone_save`                     | `{sessionId, name, language, consent: true}`                    | `VoiceOption`                                            | FR-050          |
| `tts_voice_delete`                   | `{voice: VoiceRef}`                                             | –                                                        | FR-050, FR-051  |
| `tts_model_inspect`                  | `{source: ModelSource}`                                         | `ModelInspection`                                        | FR-051          |
| `tts_model_import`                   | `{source, name, languages, licenseNote, rightsConfirmed: true}` | `VoiceOption[]`                                          | FR-051          |
| `assistant_transcript_list`          | `{threadId}`                                                    | `TranscriptEntryWire[]`                                  | FR-048          |
| `assistant_transcript_search`        | `{query, cursor?, limit?}`                                      | `{entries: {threadId, seq, id, snippet}[], nextCursor?}` | FR-048          |
| `download_assistant_assets`          | `{language}`                                                    | – (Fortschritt als Ereignis)                             | FR-005          |
| `assistant_wake_word_model`          | –                                                               | `{installed: bool, sizeBytes}`                           | FR-011          |
| `assistant_wake_word_model_download` | –                                                               | – (Fortschritt als Ereignis, `modelId` "wakeword-hex")   | FR-011          |

`language` ist die Sprache des Fensters. Gilt auf diesem Gerät eine eigene Gesprächssprache
(`assistant.language`), nehmen `assistant_readiness` und `assistant_set_enabled` diese; sie bestimmt,
welche Stimme bereit sein muss (eine der sechs Sprachen, sonst Englisch). Bei `assistant_set_enabled`
ist sie auch die Sprache des Gesprächs, und
`utcOffsetMinutes` ist der Abstand der Ortszeit des Fensters zu UTC, damit Daten und Uhrzeiten in
Ortszeit gesprochen werden. `assistant_set_enabled` ist der einzige Schalter des Assistenten (Dock
und Launcher, Betreiber 2026-11-02). Einschalten lädt nichts herunter: Fehlt ein Modell, endet der
Befehl mit dem Fehler, und die Einstellungen bieten den Download an (FR-005). Einschalten öffnet
Mikrofon und Lautsprecher, Ausschalten beendet ein Gespräch, bricht eine laufende Runde ab und
schließt beide nach dem Ton „Modus aus“. Ein Gespräch hört ohne Zeitlimit zu, bis der Nutzer „Pause“
sagt. Ist das Wake-Word-Modell installiert (PR D), spielt das Einschalten den Ton „Modus an“, und
haex wartet „bereit“ auf „Hex“ oder „Hey Hex“; nach „Pause“ wartet es wieder. Ohne das Modell
beginnt das Einschalten selbst ein Gespräch, und nach „Pause“ schaltet sich der Assistent aus,
sobald haex still ist, ohne zweiten Ton. haex hört erst zu, wenn
eigene Töne und Sätze verklungen sind; bis zur Echo-Unterdrückung (PR G) unterbricht nur das
Ausschalten eine laufende Antwort (`echoMode: "keywordOnly"`).
`assistant_set_muted` und `assistant_end_conversation` antworten, nachdem der Assistent die Änderung
übernommen hat. `assistant_set_enabled` und `assistant_set_muted` speichern den neuen Zustand als
Geräteeinstellung `assistant.enabled` bzw. `assistant.muted` (Ausschalten setzt auch `assistant.muted`
zurück); nach dem Entsperren schaltet das Fenster den Assistenten damit wieder ein, wenn nichts fehlt
(FR-002). `download_assistant_assets` lädt, was haex selbst laden kann: das gewählte
Spracherkennungsmodell, das Stimmenmodell dieser Sprache und die beiden Wake-Word-Modelle.
`assistant_wake_word_model_download` lädt nur die Wake-Word-Modelle (etwa 3 MB, gepinnt), für die
Einstellungsseite.

`assistant_tts_preview` spricht zwei Beispielsätze in der Sprache der Stimme mit `rate` (0.75–1.5)
und kehrt zurück, wenn sie gespielt sind. Fehlt das Stimmenmodell der Sprache, lädt es den Download
vorher. Die Einstellungsseite lädt das Modell aber vorher selbst über `download_tts_voice` und bietet
die Probe erst danach an (2026-11-02); sie fragt vor dem Download nach (043 FR-028,
`model_download_check` mit `{kind: "tts", id: modelId}`). Der Fortschritt aller Stimmenmodell- und
Assistent-Downloads kommt als gemeinsames `model-download-progress` (`{modelId, bytesDownloaded,
bytesTotal}`) wie bei Chat- und Spracherkennungsmodellen, damit dieselben Download-Komponenten ihn
zeigen. Läuft der Assistent, spielt die
Probe über seinen Stream, sonst über einen Stream nur mit Lautsprecher, nie mit Mikrofon. Eine
neue Probe beendet die laufende. Bis PR K nimmt der Befehl nur `builtin:`-Stimmen an.
`assistant_tone_preview` spielt einen Ton einmal mit `volume` (0–100) auf demselben Weg, auch bei
ausgeschalteten Tönen: den zugeordneten eigenen Ton, sonst den mitgelieferten, mit `customId` genau
diesen eigenen Ton. `assistant_tone_import` liest die gewählte Datei (höchstens 20 MiB), schneidet
Stille ab, gleicht die Lautstärke an und lehnt ab mit `InvalidInput`: `tooLarge` (Datei über
20 MiB), `unsupported` (kein lesbares Audio), `silent`, `tooLong` (über 3 s). `assistant_tone_assign` mit `null` stellt den
mitgelieferten Ton wieder her und prüft für „Arbeitet“ die Grenze von 1 s. `assistant_tone_delete`
löscht den Ton vault-weit und nimmt ihn aus der Zuordnung dieses Geräts. `speech_credits` liefert für „Allgemein → Lizenzen“ jede mitgelieferte Stimme,
jedes Stimmenmodell und die Spracherkennungsmodelle mit Namensnennung.

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
CustomTone = { id, name, durationMs }                     # FR-045a
SpeechCredit = { kind: "voice"|"speechModel"|"recognitionModel", name, language: string|null,
                 license, attribution, source }
VoiceOption = { ref: VoiceRef, kind: "builtin"|"clone"|"model", name, language,
                modelId, installed: bool, sizeBytes, fit, isDefault: bool }
# modelId, installed, sizeBytes, fit: the speech model the voice needs (tts_catalog.json);
# download_tts_voice({voiceId: modelId}) fetches it
CloneCheck = { sessionId, speechSecs, ok: bool,
               problems: ("tooShort"|"tooLong"|"tooQuiet"|"clipped"|"noisy")[] }
ModelSource = { kind: "folder", path } | { kind: "archive", path } | { kind: "hf", repo, revision? }
ModelInspection = { ok: true, engine, languages: string[], speakers: {id, label}[], sizeBytes }
                | { ok: false, reason: "unknownLayout"|"unsupportedEngine"|"forbiddenFile"|"tooLarge"|"unreachable" }
EnrollStep = { recorded: 1..4, ok: bool, problem: "tooShort"|"noisy"|null }
EnrollResult = { ok: true } | { ok: false, problem: "inconsistent"|"tooShort"|"confusable", similar?: string }
```

`tts_clone_from_file` nimmt die gewählte Datei (`PickedFile`, wie der Import eigener Töne), damit
auch Android-Inhalts-URIs gehen. `CloneCheck.problems` meldet `tooLong` nie: Sprache über 30 s wird
abgeschnitten (R20). `tts_clone_preview` spricht mit `rate` (Standard 1,0).

Stimmen nachbilden und Modelle importieren gibt es nur über die Oberfläche: Die Commands sind
keine Aktionen und für Agenten (eingebaut, 021, Erweiterungen) nicht aufrufbar, weil sie eine
Einwilligung bzw. ein Nutzungsrecht bestätigen (FR-050, FR-051). Die Aufnahme läuft über den
Duplex-Strom und nur, solange der Dialog offen ist.

Reading an answer aloud (FR-022c), every build (without `voice` it fails with `SpeechFailed`):

```text
chat_message_speak({text, uiLanguage, utcOffsetMinutes}) -> bool
# reads the answer in the language it is written in (uiLanguage when the text gives nothing to go
# on) with the speech model and voice chosen for it; resolves true when it has played, false when
# it was stopped (chat_message_speak_stop, a newer reading or voice sample, the vault closing);
# fails when the speech model of that language is not downloaded
chat_message_speak_stop() -> ()
```

Der bestehende Chat-Command `list_threads` (und `create_thread`, `rename_thread`) liefert pro
Sitzung zusätzlich `origin: "voice" | null`; daran markieren „Verlauf“ und der Sitzungskopf ein
Sprachgespräch (T051).

## Ereignisse (Rust → Frontend)

| Ereignis                   | Nutzlast                                  | Spec           |
| -------------------------- | ----------------------------------------- | -------------- |
| `assistant-state`          | `AssistantStateWire`                      | FR-003, FR-041 |
| `assistant-transcript`     | `TranscriptEntryWire` (unten)             | FR-042, FR-047 |
| `assistant-tone`           | `{tone: ToneId}` (sichtbare Entsprechung) | FR-044         |
| `assistant-error`          | `{kind, message}`                         | Edge Cases     |
| `assistant-wakeword-probe` | `{detected: bool, score}`                 | US7            |
| `tts-clone-level`          | `{sessionId, level, speechSecs}`          | FR-049         |
| `tts-voices-changed`       | –                                         | FR-050         |

`ToneId` = `listening`, `understood`, `working`, `question`, `done`, `error`, `aborted`,
`notUnderstood`, `conversationEnd`, `attention`, `secret`, `modeOn`, `modeOff`, `muted`, `unmuted`.

`assistant_transcript_search` defaults to 50 entries and accepts `limit` from 1 to 100. Results
have a stable ascending order by `(threadId, seq, id)`; the opaque `nextCursor` carries the last
tuple and resumes strictly after it. The server applies the limit before returning entries.
A `snippet` is the matching part of the entry with up to 40 characters on each side, "…" where
the text goes on. Query and text are compared after `fold_for_search` (case and accents do not
matter); a blank query returns no entries.

`TranscriptEntryWire` = `{threadId, seq, spokenAt, speaker ("user" oder "haex"), kind, text,
spokenUntil?, outcome?, final}`; `final: false` für Text, der noch entsteht. Die gespeicherte
Mitschrift liest `assistant_transcript_list {threadId}`; den Befehl gibt es in jedem Build, auch ohne
`voice`, weil Mitschriften auf alle Geräte synchronisiert werden. Vorerst sendet
`assistant-transcript` jeden Eintrag erst, wenn er gespeichert ist (`final: true`); eine Antwort
erscheint, wenn sie vorbei ist. Einträge eines Gesprächs ohne Thread werden nie gespeichert.

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
