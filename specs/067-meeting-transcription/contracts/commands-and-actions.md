# Contract: Commands, Ereignisse, Aktionen, Werkzeug

## Tauri-Commands (Aufrufer immer der Nutzer)

| Command                        | Eingabe                                                         | Ergebnis                | Spec           |
| ------------------------------ | --------------------------------------------------------------- | ----------------------- | -------------- |
| `meeting_start`                | `{title?, consent: true, announce, keepAudio, source, answers}` | `MeetingWire`           | FR-001–003     |
| `meeting_pause` / `_resume`    | `{id}`                                                          | `MeetingWire`           | FR-006         |
| `meeting_end`                  | `{id}`                                                          | `MeetingWire`           | FR-006, FR-011 |
| `meeting_get` / `meeting_list` | `{id}` / `{query?, cursor?}`                                    | `MeetingWire[]`         | FR-022, FR-023 |
| `meeting_transcript`           | `{id, fromSeq?, limit?}`                                        | `TranscriptEntryWire[]` | US1            |
| `meeting_rename`               | `{id, title}`                                                   | –                       | FR-022         |
| `meeting_participant_update`   | `{participantId, label?, contactId?, isMe?}`                    | –                       | FR-012         |
| `meeting_participants_merge`   | `{fromId, intoId}`                                              | –                       | FR-012         |
| `meeting_entry_assign`         | `{entryId, participantId}`                                      | –                       | FR-012         |
| `meeting_entry_split`          | `{entryId, atChar}`                                             | `{firstId, secondId}`   | FR-012         |
| `meeting_entry_edit`           | `{entryId, text}`                                               | –                       | FR-012         |
| `meeting_reassign`             | `{id}`                                                          | –                       | US4 Szen. 6    |
| `meeting_audio_open`           | `{id, atMs?}`                                                   | `{url}` (Medienserver)  | FR-015         |
| `meeting_audio_delete`         | `{id}`                                                          | –                       | FR-015         |
| `meeting_evaluate`             | `{id}`                                                          | – (Aufgabe startet)     | FR-019         |
| `meeting_export`               | `{id, target: PickedFile}`                                      | –                       | FR-024         |
| `meeting_delete`               | `{id}`                                                          | –                       | FR-025         |
| `meeting_search`               | `{query, cursor?}`                                              | `SearchHit[]`           | FR-023         |
| `voice_profile_enroll_*`       | `start` / `record {sessionId, sentence}` / `finish` / `cancel`  | `EnrollStep`            | FR-013         |
| `voice_profile_delete`         | –                                                               | –                       | FR-013         |
| `meetings_of_contact`          | `{contactId}`                                                   | `MeetingWire[]`         | FR-026         |

`MeetingWire` = Zeilen aus `haex_meetings` plus `participants`, `hasAudio`, `durationMs`,
`backlogSecs`. `TranscriptEntryWire` aus 066 erweitert um `endedAt`, `channel`, `overlapping`,
`assignment`, `edited`. `SearchHit` = `{meetingId, entryId?, field, snippet}`.

## Ereignisse

| Ereignis                  | Nutzlast                                                                                | Spec               |
| ------------------------- | --------------------------------------------------------------------------------------- | ------------------ |
| `meeting-state`           | `MeetingWire`                                                                           | FR-005             |
| `meeting-entry`           | `TranscriptEntryWire` (neu oder geändert)                                               | US1, FR-011        |
| `meeting-assignment-done` | `{id, changedEntries}`                                                                  | US1 Szen. 9        |
| `meeting-warning`         | `{kind, ...}` mit `kind` = `storage`, `longRunning`, `deviceAudioDenied` oder `backlog` | FR-016, Edge Cases |

## Katalogaktionen (Spec 020)

| Aktion                  | Bereich      | Agent | Spec   |
| ----------------------- | ------------ | ----- | ------ |
| `meetings.record.start` | `guardrails` | nein  | FR-001 |
| `meetings.record.stop`  | `guardrails` | nein  | FR-006 |
| `meetings.list`         | `meetings`   | lesen | FR-022 |
| `meetings.search`       | `meetings`   | lesen | FR-023 |
| `meetings.ofContact`    | `meetings`   | lesen | FR-026 |

Aufzeichnen starten und beenden ist Nutzersache (Einwilligung); Agenten lesen nur.

## Agent-Werkzeug `meetings.transcript.read` (nativ, nur lesend)

Angeboten nur im Meeting-Chat und in Assistent-Gesprächen während eines Meetings (FR-021).

```text
input:  { meetingId, fromMs?, toMs?, speaker?: participantId, query?: string, limit?: int ≤ 200 }
result: { entries: [{ id, at, speaker: label, text }], more: bool }
```

## Auswertung

`meeting_evaluate` startet eine Aufgabe (068) bzw. einen Turn des eingebauten Agenten mit festem
Schema `Evaluation` ([data-model.md](./data-model.md)); das Ergebnis wird geprüft (Verweise) und als neue
Version gespeichert.
