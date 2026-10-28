# Implementation Plan: Meeting-Mitschrift mit Sprecherzuordnung

**Branch**: `067-meeting-transcription` | **Date**: 2026-10-28 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/067-meeting-transcription/spec.md`

## Summary

haex zeichnet Meetings auf: nach bestätigter Einwilligung live transkribiert, Stimmen getrennt und
benennbar, die eigene Stimme als „Ich“, Audio nur auf Wunsch, auf dem Desktop auch der Ton eines
Online-Meetings; der Assistent ist live per Wake Word und danach für Auswertung und Fragen da.
Klärungen vom 2026-10-28: eigene Spec, anonym + benennen, Audio pro Meeting wählbar, Agent live und
danach, Mikrofon + Geräteton am Desktop.

Technischer Ansatz ([research.md](./research.md)):

- **Datenmodell** (R1): Meeting = Unterhaltung mit `origin = 'meeting'`; Mitschrift in
  `voice_transcript_entries` aus 066, erweitert um Kanal, Ende, Zuordnungsart, Original; eigene
  Tabellen für Meeting, Teilnehmer, Audio-Stücke, Auswertung, Stimmprofil.
- **Erkennung** (R2): pro Äußerung; Spike Whisper gegen Parakeet TDT 0.6B v3 (sherpa).
- **Sprecher** (R3, R4): WeSpeaker-Einbettung über sherpa; live schrittweises Clustering, am Ende neu,
  Nutzerkorrekturen als feste Vorgaben; Stimmprofil nur für den Nutzer.
- **Geräteton** (R5, R6): `cpal`-Loopback (Windows, macOS ≥ 14.6, Linux PipeWire/Pulse); Geräteton als
  Referenz der Echo-Unterdrückung aus 066.
- **Audio** (R7): Ogg Opus 16 kbit/s je Kanal in 1-MiB-Stücken als CRDT-BLOBs; eigene
  `StreamingSource` für den Medienserver.
- **Suche** (R8): gefaltete Teilstring-Suche in Rust, kein FTS.
- **Agent** (R9): Lese-Werkzeug `meetings.transcript.read`; Auswertung mit festem Schema über 068 bzw.
  den eingebauten Agenten.
- **App** (R10, R11): System-App `system.meetings`, Kontakt-Abschnitt und wiederverwendbarer
  Kontakt-Wähler.

## Technical Context

**Language/Version**: Rust (Tauri 2.12), TypeScript 6 (strict), Vue 3.5, Nuxt 4.5.2 (SPA), Kotlin
(nur über 066 PR H).

**Primary Dependencies**:

- **Aus 066**: `cpal` 0.18.2, `sherpa-onnx` 2.x (`shared`), `EchoCanceller` (`sonora`), Audio-Strom,
  Pausenerkennung, schwebende Anzeige, Tray, Android-Dienst.
- **Neu**: Sprecher-Einbettung WeSpeaker ResNet34-LM und optional pyannote segmentation-3.0 (Modelle
  über den Katalog aus 066 `assistant/assets.json`); `opus` (libopus, BSD-3) und `ogg` (BSD-3);
  ggf. Parakeet TDT 0.6B v3 als STT-Katalogstufe (CC-BY-4.0); unter Linux ggf. `pipewire` (MIT).
- **macOS**: `NSAudioCaptureUsageDescription` in `Info.plist`.
- **Keine neue Frontend-Abhängigkeit.**

**Storage**: eine Migration (nächste freie Nummer, nach 066): Spalten an `voice_transcript_entries`,
Tabellen `haex_meetings`, `haex_meeting_participants`, `haex_meeting_audio_chunks`,
`haex_meeting_evaluations`, `haex_voice_profile` ([data-model.md](./data-model.md));
`HOLZI_TRIGGER_VERSION` +1.

**Testing**:

- **Rust, Einheit**: `meetings/{session,cluster,reassign,participants,entries,audio/{encode,chunks,
source},search,export,evaluation,profile}` je mit `*_tests.rs`.
- **Rust, Integration**: `tests/meetings_session.rs` (WAV-Quelle mit vier Sprechern, falsche
  Erkennung, injizierte Uhr: Start mit Einwilligung, Pause, Ende, End-Durchgang mit Nutzerkorrekturen,
  Löschen kaskadiert, Vault-Schließen), `tests/meetings_sync.rs` (30 MB Audio zwischen zwei Test-Vaults).
- **Messungen (Release, `--ignored`)**: `meetings_diarization_eval.rs` (SC-003, SC-004),
  `meetings_stt_eval.rs` (SC-002, SC-005, Rückstand bei 4 h).
- **Prüfskripte**: `scripts/check-meetings-actions.ts` (Aufzeichnen nur `guardrails`, Lese-Aktionen),
  `scripts/check-meetings-ui.ts` (Texte de/en, Routen).
- **End-to-End**: `meeting-record` (WAV-Skript über `e2e-audio`), `meeting-online` (WAV über Geräteton,
  Linux-CI mit PipeWire-Null-Sink).
- **Von Hand**: [quickstart.md](./quickstart.md) M1–M11.

**Target Platform**: Linux, macOS (≥ 14.6 für Geräteton), Windows; Android nur Mikrofon (Hintergrund über
066 PR H); iOS nur Vordergrund und Mikrofon.

**Project Type**: desktop-app mit Mobil-Zielen.

**Performance Goals**: Mitschrift ≤ 5 s nach Satzende (SC-001); 4 h ohne Rückstand (SC-002);
≥ 85 % / ≥ 75 % richtige Sprechzeit (SC-003, Risiko siehe research R3); Audio ≤ 30 MB/h, Akku
≤ 15 %/h (SC-007); Suche über 100 h Mitschrift < 1 s.

**Constraints**:

- **Einwilligung**: Ohne `consent_at` startet nichts; Agenten können nicht aufzeichnen (`guardrails`).
- **Fremde Stimmen**: Einbettungen fremder Sprecher nur bis zum Ende des End-Durchgangs, dann geleert;
  einzig `haex_voice_profile` bleibt.
- **Audio**: nur bei `keep_audio`; sonst wie 008 FR-020 verworfen.
- **Async**: Erkennung, Einbettung und Opus auf eigenen Threads bzw. `spawn_blocking`; alles über
  `VaultGate`; Vault-Schließen beendet das Meeting (FR-007).
- **Code-Regeln**: Dateien ≤ 500 Zeilen; Testcode getrennt; keine Mitschrift, kein Audio und keine
  Einbettung in Logs oder Fehlertexten.
- **Eine Implementierung pro Regel**: Mitschrift-Tabelle aus 066, Faltung aus `passwords::ids`,
  `EchoCanceller` aus 066, Medienserver aus 044.

**Scale/Scope**: 1 Rust-Modul `meetings` (≈ 18 Dateien), 1 Migration (5 Tabellen, 6 Spalten), ≈ 22
Commands, 4 Ereignisse, 5 Katalogaktionen, 1 Agent-Werkzeug, 1 App mit 3 Ansichten, 1 Kontakt-Abschnitt,
1 Kontakt-Wähler, 3 Integrations- und 2 Messtests, 2 e2e-Szenarien.

## Constitution Check

_GATE: Muss vor Phase 0 bestehen. Nach Phase 1 erneut geprüft._

| Prinzip / Vorgabe                                 | Status | Begründung                                                                                                |
| ------------------------------------------------- | ------ | --------------------------------------------------------------------------------------------------------- |
| I Keine Geheimnisse in Git                        | ✅     | Testaufnahmen selbst erzeugt (CC0); keine Schlüssel                                                       |
| II Keine lokalen absoluten Pfade                  | ✅     | Modelle über `AppLocalData`                                                                               |
| III Projektidentität geräteunabhängig             | ✅     | Nicht berührt                                                                                             |
| IV Cross-Repo-Referenzen gepinnt                  | ✅     | Modelle mit Revision und Digest (WeSpeaker-Tags tragen auch nicht kommerzielle Modelle)                   |
| V Externe Quellen nur per Opt-in                  | ✅     | Keine neue Harness-Quelle                                                                                 |
| VI Selbstverändernde Anweisungen review-pflichtig | ✅     | Nicht berührt                                                                                             |
| VII Relay-Ausfall blockiert lokale Arbeit nicht   | ✅     | Alles lokal; Sync großer Audio-Daten im Hintergrund                                                       |
| VIII Keine Verheimlichung                         | ✅     | Laufende Aufnahme dauerhaft sichtbar; Einwilligung gespeichert; Agent kann nicht heimlich aufzeichnen     |
| Phasendisziplin                                   | ✅     | Umsetzung nach 066 PR B–D (für E nach 066 PR G)                                                           |
| ADR bei prinzipienrelevanter Entscheidung         | ✅     | Keine nötig: Datenmodell folgt ADR-0001 und dem BLOB-Muster der Passwörter; keine neue Sicherheitsgrenze  |
| Async nicht blockieren, 500-LoC-Grenze, Tests     | ✅     | Siehe Constraints und Testing                                                                             |
| Keine neue Crate ohne Not                         | ✅     | `opus`/`ogg` nur für „Audio behalten“; `pipewire` nur falls cpal den Monitor nicht anbietet               |
| Jede Plattform, jedes Gerät                       | ✅     | Geräteton fehlt auf Android/iOS und macOS < 14.6 aus Plattformgründen; dokumentiert und in der UI erklärt |

### Kandidaten (Erweitern statt neu bauen)

| #   | Neues Artefakt          | Vorhandener Kandidat                                      | Entscheidung                                                   |
| --- | ----------------------- | --------------------------------------------------------- | -------------------------------------------------------------- |
| K1  | Mitschrift              | `voice_transcript_entries` (066)                          | Erweitern um Spalten                                           |
| K2  | Audio-BLOBs             | `passwords/binaries.rs`                                   | Muster, aber Stücke statt Hash-Zeilen (kein Dedup nötig)       |
| K3  | Abspielen               | `files/media/server.rs`, `StreamingSource`                | Neue Quelle `ChunkedAudioSource`                               |
| K4  | Suche                   | `passwords::ids::fold_for_search`, `contacts/search.rs`   | Gleiche Faltung                                                |
| K5  | Export                  | `usePickedFile`, `files::picked::PickedFile`              | Wiederverwenden                                                |
| K6  | Kontakt-Rückverweis     | `FileSharesSection.vue`, `files.shares.ofContact`         | Gleiches Muster `MeetingsSection.vue`, `meetings.ofContact`    |
| K7  | Kontakt-Wähler          | `ShareDialog.vue` (eigene Auswahl), `ReferencePicker.vue` | Neue `ContactPicker.vue`, später von `ShareDialog` übernehmbar |
| K8  | Aufnahme, Anzeige, Tray | 066 `audio/stream.rs`, `AssistantBubble.vue`, `tray.rs`   | Zustand „Meeting“ ergänzen                                     |
| K9  | Echo                    | 066 `assistant/echo.rs`                                   | Geräteton als zweite Referenzquelle                            |

## Project Structure

```text
specs/067-meeting-transcription/
├── spec.md, plan.md, research.md, data-model.md, quickstart.md
├── contracts/commands-and-actions.md
├── checklists/requirements.md
└── tasks.md

src-tauri/src/meetings/
├── mod.rs, commands.rs, migrations (identity/migrations_meetings.rs)
├── session.rs          # laufendes Meeting: Start, Pause, Ende, Kanäle, Warteschlange
├── device_audio.rs     # Geräteton je Betriebssystem
├── cluster.rs          # live schrittweise Zuordnung
├── reassign.rs         # End-Durchgang, Abbildung auf Live-Bezeichnungen
├── profile.rs          # eigenes Stimmprofil
├── participants.rs, entries.rs
├── audio/{encode.rs, chunks.rs, source.rs}
├── search.rs, export.rs, evaluation.rs, transcript_tool.rs
└── fixtures/
src-tauri/tests/meetings_session.rs, meetings_sync.rs,
                meetings_diarization_eval.rs, meetings_stt_eval.rs

src/
├── components/apps/MeetingsApp.vue
├── components/meetings/{List.vue, Detail.vue, Transcript.vue, Participants.vue,
│                        Player.vue, Evaluation.vue, StartDialog.vue, Toolbar.vue}
├── components/contacts/{ContactPicker.vue, MeetingsSection.vue}
├── components/settings/meetings/VoiceProfileSetting.vue
├── lib/meetings/registry.ts, lib/actions/meetingsActions.ts
└── stores/meetings.ts, stores/meetingsActionHandlers.ts
```

## Lieferung (jede ein PR, jede auslieferbar)

| PR  | Inhalt                                                                                                                                                                    | Stories        |
| --- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------- |
| A   | Spec, Plan, Research, Datenmodell, Vertrag, Quickstart, Tasks                                                                                                             | –              |
| B   | Spike (STT, Sprechertrennung auf deutschen Aufnahmen, Sync 30 MB, Suche); Migration; Aufzeichnen mit Einwilligung, Pause, Ende; Mikrofon; live „Sprecher N“; Meetings-App | US1 (Mikrofon) |
| C   | End-Durchgang, Benennen, Kontakt-Wähler, Umhängen, Teilen, Zusammenführen, Bearbeiten; Stimmprofil                                                                        | US2, US3       |
| D   | Audio behalten: Opus-Stücke, Player, Löschen, Neuberechnung mit Audio, Überlappung                                                                                        | US4            |
| E   | Geräteton (Windows, macOS, Linux), Echo-Abzug, getrennte Kanäle, „Ich“ ohne Profil                                                                                        | US1 (online)   |
| F   | Assistent im Meeting, Lese-Werkzeug, Auswertung, Meeting-Chat, Aufgaben übernehmen                                                                                        | US5, US6       |
| G   | Suche, Export, Kontakt-Abschnitt                                                                                                                                          | US7            |

**Abhängigkeiten**: 066 PR B–D vor B; 066 PR G vor E; 066 PR H für Android im Hintergrund; 068 für die
Stufenwahl der Auswertung (ohne 068 läuft sie mit dem Standardmodell). C–G hängen nur von B ab.

## Complexity Tracking

| Abweichung         | Warum nötig                                 | Einfachere Alternative verworfen, weil                          |
| ------------------ | ------------------------------------------- | --------------------------------------------------------------- |
| Native libopus     | SC-007 (≤ 30 MB/h) mit verschlüsseltem Sync | Unkomprimiert ≈ 115 MB/h; reine Rust-Encoder noch nicht erprobt |
| Eigenes Clustering | Fertige Pipeline erreicht SC-003 nicht (R3) | sherpa-Standard: 35 % DER auf AMI                               |
