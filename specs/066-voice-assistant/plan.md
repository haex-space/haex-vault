# Implementation Plan: Sprachassistent

**Branch**: `066-voice-assistant` | **Date**: 2026-10-28 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/066-voice-assistant/spec.md`

## Summary

haex bekommt einen Sprachassistent-Modus: Im Launcher eingeschaltet, hört haex auf „Haex“ / „Hey
Haex“ (oder ein eigenes Wort), führt ein Gespräch mit dem eingebauten Agenten (003/032) und antwortet
gesprochen, mit Anschlussfragen, Dazwischenreden, mündlichen Freigaben, Rückfragen, Vorschlägen,
Aufgaben im Hintergrund und vorgelesenen Geheimnissen. Den Umfang bestimmen Spec und Klärungen vom
2026-10-28:

- Zuhören, solange haex läuft und der Vault entsperrt ist; Android per Vordergrund-Dienst, iOS nur
  im Vordergrund.
- Wake Word „Haex“ und „Hey Haex“ als trainiertes Modell; eigenes Wort per Einsprechen.
- Alles per Sprache, auch Freigaben und Geheimnisse (Bestätigung pro Abruf).
- Lokale Stimme als Standard, externer Dienst optional mit dauerhafter Anzeige.
- Freies Dazwischenreden, wo die Echo-Unterdrückung reicht, sonst nur per Wake Word und Stopp-Wort.
- Aktives Beenden bricht Aufgaben ab; sonst meldet sich haex von selbst.
- Schwebende Anzeige mit kompakter Mitschrift; eigenes Klangset.
- Antwortbeginn ≤ 3 s mit Cloud-Modell, ≤ 6 s lokal.

Technischer Ansatz (Begründungen und verworfene Alternativen in [research.md](./research.md),
Entscheidung zu Geheimnissen in [ADR-0020](../../docs/adr/0020-spoken-secrets-bypass-the-model.md)):

- **Steuerung in Rust** (R1): Modul `src-tauri/src/assistant/`; Runden über das aus `send_message`
  herausgelöste `chat::send::start_turn`, Ereignisse wie bisher an die Oberfläche und zusätzlich an
  den Assistenten.
- **Audio** (R2): dauerhafter Duplex-Strom mit `cpal` 0.18.2; Ringpuffer, Mischer, Referenzsignal.
- **Sprach-Laufzeit** (R3): `sherpa-onnx` 2.x (`shared`) für Pausenerkennung, offene
  Schlüsselwort-Erkennung und Sprachausgabe; `ort` (`load-dynamic`) auf derselben ONNX-Runtime für
  das eigene Wake-Word-Modell. Spike auf allen Zielen zuerst.
- **Wake Word** (R4, R5): eigenes Modell nach openWakeWord-Architektur, trainiert im Repo
  `haex-space/haex-wakeword`; eigenes Wort über Vorlagen aus vier Aufnahmen.
- **Sprachausgabe** (R8): Kyutai Pocket TTS (de/en) über sherpa, satzweise; Katalog wie STT.
- **Echo** (R9): AEC3 in der App (`sonora`, Ersatz `webrtc-audio-processing`), Messung je
  Plattform, sonst nur Wake Word und Stopp-Wort.
- **Freigaben, Rückfragen, Aufgaben** (R11, R12): lokal ausgewertete Antworten, Zustandsautomat
  mit injizierter Uhr.
- **Geheimnisse** (R13): Werkzeug `assistant.secret.speak`, `SpokenConsent`,
  `reveal_for_speech`, nur lokale Stimme.
- **Oberfläche** (R14): schwebende Anzeige in `Desktop.vue`, Tray-Symbol (`tray-icon`), Kachel im
  Launcher, Steuer-Eintrag im Dock.
- **Android** (R15): Kotlin-Vordergrund-Dienst im Plugin `holzi-android`.

## Technical Context

**Language/Version**: Rust (Tauri 2.12), TypeScript 6 (strict), Vue 3.5, Nuxt 4.5.2 (SPA), Kotlin
(Android-Plugin), Python 3.12 nur im externen Trainings-Repo.

**Primary Dependencies**:

- **Vorhanden**: `cpal` 0.18.2 (Feature `voice`), candle-Whisper (`stt::local`), `tokio`,
  `tokio-util`, `serde_json`, `ts-rs`, `tauri-plugin-notification`, `zeroize`, Pinia,
  `@nuxtjs/i18n`, haex-ui (`Group`, `Row`, `OptionRow`).
- **Neu**: `sherpa-onnx` 2.0.x exakt gepinnt, `default-features = false`, `shared`; `ort` 2.x mit
  `load-dynamic` (nur falls das Wake-Word-Modell nicht über sherpa läuft, R3/R4); `sonora` per
  Git-SHA (Ersatz `webrtc-audio-processing` 2.x); Tauri-Feature `tray-icon`.
- **Nix/Paketierung**: `libayatana-appindicator` (Linux-Tray); die geteilte `libonnxruntime`
  wird neben dem Binary ausgeliefert (alle Ziele, Android im `jniLibs`).
- **Keine neue Frontend-Abhängigkeit.**

**Storage**: Migration mit der nächsten freien Nummer: `chat_threads.origin`;
`HOLZI_TRIGGER_VERSION` 29 → 30. Geräteeinstellungen `assistant.*` ([data-model.md](./data-model.md)).
Modelle unter `<AppLocalData>/models/<id>/`.

**Testing**:

- **Rust, Einheit** (`*_tests.rs`): `audio/{ring,resample,mixer}`, `assistant/{conversation,
speakable,spell,sentences,phrases,approval,choice,tones,wakeword/templates,enroll}`,
  `tts/catalog`.
- **Rust, Integration**: `tests/assistant_conversation.rs` (WAV-Quelle, falsche Erkennung, falsche
  Stimme, Modell-Skript, injizierte Uhr: Runden, Nachhörzeit, Fortsetzung, Stopp, Freigaben, offene
  Fragen, aktives Ende), `tests/assistant_secret_contract.rs` (ADR-0020).
- **Rust, Messung (Release, `--ignored`)**: `tests/assistant_wakeword_eval.rs` (SC-003/SC-004 auf
  Testsatz), `tests/assistant_echo_eval.rs` (SC-005 je Plattform), Tool-Eval aus 032 mit der
  Gesprächsanweisung.
- **Prüfskripte**: `scripts/check-assistant-actions.ts` (alle `assistant.*` in `guardrails`,
  Kürzel), `scripts/check-assistant-ui.ts` (Anzeige-Zustände → Darstellung, Texte in de/en).
- **End-to-End**: Szenario `voice-assistant` mit Feature `e2e-audio` (Audio aus WAV-Skript, Ausgabe
  in eine Senke); Android-Szenario für Dienst und Benachrichtigung.
- **Von Hand**: [quickstart.md](./quickstart.md) M1–M12.
- **Regression**: `voice`-Tests aus 008, `chat`-Turn-Tests, `passwords_access`, Tool-Eval, `typecheck`,
  `lint`, `format:check`.

**Target Platform**: Linux, macOS, Windows (voll), Android (Vordergrund-Dienst), iOS (nur
Vordergrund). Echo-Verhalten je Plattform in `docs/voice-assistant-platforms.md`.

**Project Type**: desktop-app mit Mobil-Zielen (Nuxt-SPA + Rust-Backend in einem Tauri-Projekt).

**Performance Goals**: Ton „hört zu“ ≤ 0,3 s nach dem Wake Word, Quittung ≤ 0,5 s nach Sprechende
(SC-001); Sprachausgabe ≤ 1 s nach den ersten Wörtern, Antwortbeginn ≤ 3 s Cloud / ≤ 6 s lokal
(SC-002); Abbruch ≤ 0,5 s bzw. 1 s (SC-005, SC-006); „bereit“ < 5 % eines Kerns, ≤ 3 % Akku pro Stunde
(SC-010). Referenzgeräte: Desktop mit 8-Kern-x86 ohne dedizierte GPU, 16 GB; Telefon Pixel 7a
(Android 15). Festgelegt hier, gemessen in PR B, D und H.

**Constraints**:

- **Datenschutz vor dem Wake Word**: Audio nur im Ringpuffer (≤ 2 s), nie gespeichert, nie an einen
  Dienst (FR-006, SC-012). Erkennung und Wake Word laufen auf dem Gerät.
- **Geheimnisse**: nur über `reveal_for_speech` mit `SpokenConsent`; nie in Ereignis, Log, `Debug`,
  Mitschrift, Modell oder externem Dienst (ADR-0020).
- **Entscheidungen**: Freigaben und Auswahlen wertet Rust lokal aus, nie das Modell (SC-009).
- **Async**: Audio-Callbacks ohne Sperren und Speicheranforderungen; Erkennung, Wake Word und
  Sprachausgabe auf eigenen Threads bzw. `spawn_blocking`; alles über `VaultGate`, beendet in
  `begin_close`/`finish_close`.
- **Code-Regeln**: Dateien ≤ 500 Zeilen. `chat/commands.rs` (743) gibt den Rumpf von
  `send_message` an `chat/send.rs` ab; `chat/turn/tool_round.rs` (499) bekommt keine Zeile mehr — der
  Assistent hängt sich über die `emit`-Closure an.
- **Eine Implementierung pro Regel**: Freigabe-Entscheidung bleibt `permission::decide`; Schwärzung
  über `mcp_server::redact`; Stopp-Wörter über `stt::interrupt`.
- **Testcode**: in eigenen Dateien.

**Scale/Scope**: 2 neue Rust-Module (`assistant` ≈ 20 Dateien, `tts` ≈ 6), Erweiterung von `audio`
und `chat`, 1 Migration, 16 Commands, 6 Ereignisse, 5 Katalogaktionen, 1 Agent-Werkzeug, 1
Kotlin-Dienst, 1 schwebende Anzeige, 4 Einstellungsseiten, 2 Integrations- und 2 Messtests, 2
e2e-Szenarien, 1 ADR, 1 externes Trainings-Repo.

## Constitution Check

_GATE: Muss vor Phase 0 bestehen. Nach Phase 1 erneut geprüft — Ergebnis unten._

Geprüft gegen `.specify/memory/constitution.md` (v1.4.0) und die spaex-Constitution
`.spaex/constitution.md`.

| Prinzip / Vorgabe                                        | Status | Begründung                                                                                                        |
| -------------------------------------------------------- | ------ | ----------------------------------------------------------------------------------------------------------------- |
| I Keine Geheimnisse in Git                               | ✅     | Kein Schlüssel im Repo; Testaufnahmen selbst erzeugt; Zugangsdaten externer Dienste nur im Vault                  |
| II Keine lokalen absoluten Pfade                         | ✅     | Modelle über `AppLocalData`; Verträge nennen Pfade symbolisch                                                     |
| III Projektidentität geräteunabhängig                    | ✅     | Nicht berührt                                                                                                     |
| IV Cross-Repo-Referenzen gepinnt                         | ✅     | Modelle mit HF-Revision (Commit-SHA); `sonora` per Git-SHA; Trainings-Repo per SHA im Katalog                     |
| V Externe Quellen nur per Opt-in                         | ✅     | Keine neue Harness-Quelle                                                                                         |
| VI Selbstverändernde Anweisungen review-pflichtig        | ✅     | Die Gesprächsanweisung ist Code im Repo; Agenten können den Modus nicht steuern (`guardrails`)                    |
| VII Relay-Ausfall blockiert lokale Arbeit nicht          | ✅     | Alles lokal; externer Dienst fällt auf lokale Stimme zurück (FR-031)                                              |
| VIII Keine Verheimlichung                                | ✅     | Anzeige jederzeit sichtbar, Mitschrift im Verlauf, externe Dienste dauerhaft gekennzeichnet, Vorlese-Hinweis      |
| Phasendisziplin                                          | ✅     | 032 (eingebauter Agent mit Aktionen), 008 US1 und 010 im Einsatz; 008 US2 (Stopp-Wörter) wird in PR C mit gebaut  |
| Workflow: speckit, PR, Conventional Commits, kein Squash | ✅     | specify → clarify (2026-10-28) → plan → tasks → implement; Worktree je PR                                         |
| ADR bei prinzipienrelevanter Entscheidung                | ✅     | ADR-0020 (Geheimnisse vorlesen), in diesem PR „proposed“, „accepted“ mit PR F                                     |
| Test-Code in separaten Dateien                           | ✅     | `*_tests.rs`, `src-tauri/tests/`, `scripts/check-assistant-*.ts`                                                  |
| Async nicht blockieren                                   | ✅     | Siehe Constraints                                                                                                 |
| Keine neue Crate ohne Not                                | ✅     | `sherpa-onnx` ersetzt drei Eigenbauten; `sonora` nötig für SC-005; `ort` nur falls R4 es braucht                  |
| 500-LoC-Grenze                                           | ✅     | Siehe Constraints                                                                                                 |
| Graphify vor neuen benannten Artefakten                  | ✅     | Abfragen zu Aufnahme, STT, Turn, Freigaben, Passwörtern, Einstellungen, Launcher, Dock gelaufen; Kandidaten unten |
| `ponytail:`-Kommentar bei bewusster Vereinfachung        | ✅     | Geplant an: feste Pausenlänge statt Satzende-Modell (R6); Kopfhörer am Desktop per Gerätename (R9)                |
| Nicht-triviale Logik hinterlässt einen Check             | ✅     | Siehe Testing                                                                                                     |
| Jede Plattform, jedes Gerät                              | ✅     | Alle Ziele; Grenzen (iOS-Hintergrund, Echo je Plattform) sind Plattformregeln und dokumentiert, keine Lücken      |

### Kandidaten (Erweitern statt neu bauen)

| #   | Neues Artefakt                  | Vorhandener Kandidat                                                 | Entscheidung                                                                           |
| --- | ------------------------------- | -------------------------------------------------------------------- | -------------------------------------------------------------------------------------- |
| K1  | Dauerhafte Aufnahme             | `audio::Capture` (Push-to-Talk, ganzer Puffer)                       | Erweitern: Strom mit Ringpuffer; Push-to-Talk aus 008 liest künftig daraus             |
| K2  | Erkennung pro Äußerung          | `stt::SttAdapter`, `VoiceState.whisper`                              | Wiederverwenden; `VoiceState` und Assistent teilen den warmen Adapter                  |
| K3  | Stopp-Wörter                    | `stt::interrupt::match_interrupt`                                    | Wiederverwenden; 008 US2 wird damit fertig                                             |
| K4  | Runde starten                   | `chat::commands::send_message`                                       | Rumpf nach `chat::send::start_turn`; Command und Assistent rufen dieselbe Funktion     |
| K5  | Freigabe beantworten            | `respond_tool_permission`, `pending_tool_approvals`                  | Gemeinsame Funktion `chat::approvals::resolve`; Command und Assistent nutzen sie       |
| K6  | Rückfrage mit Wahlmöglichkeiten | `ask_user`, `chat-choice-request`                                    | Wiederverwenden; der Assistent beantwortet über denselben Weg                          |
| K7  | Modell-Katalog                  | `stt/stt_catalog.json`, `stt/catalog.rs`, `models::download`         | Gleiches Format und Download für `tts_catalog.json` und `assistant/assets.json`        |
| K8  | Geheimnis lesen                 | `passwords/reveal.rs` (Nutzer)                                       | Neuer Eingang `reveal_for_speech` neben `reveal`, gleiche Prüfung plus `SpokenConsent` |
| K9  | Schwärzung                      | `mcp_server/redact.rs`                                               | Wiederverwenden für die Ansage von Freigaben                                           |
| K10 | Kachel ohne App                 | Sperr-Kachel in `Launcher.vue`, `DockControlId`                      | Gleiches Muster: Kachel „Sprachassistent“, `DockControlId` `"assistant"`               |
| K11 | Ladeanimation                   | `files/Loader.vue` (Sprite-Daumenkino)                               | Gleiche Technik für die Zustände der Anzeige                                           |
| K12 | Schließen                       | `voice.cancel_for_close` / `reset_for_close` in `instances/close.rs` | Assistent hängt sich an dieselben zwei Stellen                                         |

**Ergebnis nach Phase 1**: keine Verletzung.

## Project Structure

### Documentation (this feature)

```text
specs/066-voice-assistant/
├── spec.md
├── plan.md              # diese Datei
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── tauri-commands.md
│   └── agent-and-actions.md
├── checklists/requirements.md
└── tasks.md             # /speckit-tasks
docs/adr/0020-spoken-secrets-bypass-the-model.md
docs/voice-assistant-platforms.md          # PR G: Echo-Verhalten je Plattform, Herkunft der Töne
```

### Source Code (repository root)

```text
src-tauri/src/
├── audio/
│   ├── mod.rs            # vorhandene Push-to-Talk-Aufnahme, liest künftig aus dem Strom
│   ├── stream.rs         # Duplex-Strom (cpal), Start/Stopp mit VaultGate
│   ├── ring.rs           # Ringpuffer 16 kHz Mono, Vorlauf
│   ├── resample.rs       # blockweise Umrechnung mit Zustand
│   ├── mixer.rs          # Sprachausgabe + Töne, Referenzsignal
│   └── source.rs         # Source/Sink-Traits, WAV-Quelle und Senke für Tests (e2e-audio)
├── tts/
│   ├── mod.rs, catalog.rs, tts_catalog.json, commands.rs
│   ├── local.rs          # sherpa OfflineTts, satzweise
│   └── external.rs       # PR J: OpenAI-kompatibler Endpunkt
├── assistant/
│   ├── mod.rs            # AssistantState, Start/Stopp, Schließen
│   ├── assets.rs, assets.json
│   ├── listen.rs         # VAD, Äußerungen, Vorlauf, Höchstdauer
│   ├── conversation.rs   # Zustandsautomat, Zeitgeber, offene Fragen
│   ├── turn.rs           # start_turn + emit-Brücke, Satzschnitt, Fortschrittsansage
│   ├── sentences.rs, speakable.rs, phrases.rs, spell.rs
│   ├── approval.rs       # mündliche Freigaben, SpokenConsent
│   ├── choice.rs         # ask_user per Sprache
│   ├── secret.rs         # Werkzeug assistant.secret.speak (PR F)
│   ├── tones.rs
│   ├── echo.rs           # EchoCanceller, Modus je Plattform (PR G)
│   ├── wakeword/{mod.rs, model.rs, templates.rs, enroll.rs}
│   ├── tray.rs           # Desktop-Tray
│   └── commands.rs
├── chat/send.rs, chat/approvals.rs, chat/tools/prompt.rs
├── passwords/service/speech.rs   # reveal_for_speech
└── identity/migrations*.rs       # chat_threads.origin

src-tauri/plugins/holzi-android/android/src/main/java/space/haex/holzi/android/AssistantService.kt
src-tauri/tests/assistant_conversation.rs, assistant_secret_contract.rs,
                assistant_wakeword_eval.rs, assistant_echo_eval.rs

src/
├── components/assistant/{AssistantBubble.vue, CompactTranscript.vue}
├── components/settings/assistant/{AssistantSetting.vue, WakeWordSetting.vue,
│                                  VoiceSetting.vue, TonesSetting.vue}
├── stores/assistant.ts
├── lib/actions/assistantActions.ts
├── components/wm/{Launcher.vue, Desktop.vue, DockItem.vue}, lib/wm/dock.ts
└── assets/assistant/     # Eichhörnchen-Sprites pro Zustand

scripts/check-assistant-actions.ts, scripts/check-assistant-ui.ts
scripts/e2e/scenarios/voice-assistant.test.ts, android-voice-assistant.test.ts
```

**Structure Decision**: ein neues Rust-Modul `assistant` neben `voice` und `stt`, ein Modul `tts`
nach dem Muster von `stt`; Frontend nach den vorhandenen Ordnern. Das Training des Wake Words lebt
außerhalb dieses Repos (`haex-space/haex-wakeword`), weil es Python, GPU und große Datensätze braucht;
hier landet nur das fertige Modell über den Katalog.

## Lieferung (jede ein PR, jede auslieferbar)

| PR  | Inhalt                                                                                                                                                                                                                         | Stories      | Berührt                                                         |
| --- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------ | --------------------------------------------------------------- |
| A   | Spec, Klärungen, Plan, Research, Datenmodell, Verträge, Quickstart, ADR-0020 (proposed), Tasks                                                                                                                                 | –            | `specs/066-voice-assistant/`, `docs/adr/`, `plans/README.md`    |
| B   | Spike auf allen Zielen (sherpa, ONNX-Runtime, Größe, Echtzeitfaktor), Duplex-Strom, Ringpuffer, Mischer, Source/Sink, Pausenerkennung, lokale Sprachausgabe mit Katalog und Download, Klangset, Einstellungen Stimme und Töne  | US8 (lokal)  | `audio/`, `tts/`, `assistant/{assets,tones}`, Einstellungen     |
| C   | Gesprächskern ohne Wake Word: `start_turn`, Zustandsautomat, Gespräch per Kürzel und Anzeige, Nachhörzeit, Fortsetzung, Stopp-Wörter, Schlusswendungen, Sprechtext, Gesprächsanweisung, Migration `origin`, schwebende Anzeige | US2, US5     | `assistant/`, `chat/send.rs`, Frontend Anzeige                  |
| D   | Standard-Wake-Word (Modell aus `haex-wakeword`), Empfindlichkeit, Schutz vor Selbstauslösung, Kachel im Launcher, Dock-Eintrag, Tray, Modus sichtbar; Messung SC-003/004/010                                                   | US1          | `assistant/wakeword/`, `tray.rs`, Launcher, Dock                |
| E   | Mündliche Freigaben, Rückfragen und Vorschläge, Fortschrittsansage, Aufgaben im Hintergrund, offene Fragen, aktives Ende                                                                                                       | US3, US4     | `assistant/{approval,choice,conversation}`, `chat/approvals.rs` |
| F   | Geheimnisse vorlesen: Werkzeug, `SpokenConsent`, `reveal_for_speech`, Buchstabieren, Vertragstest; ADR-0020 accepted                                                                                                           | US6          | `assistant/{secret,spell}`, `passwords/service/speech.rs`       |
| G   | Echo-Unterdrückung, freies Dazwischenreden, Kopfhörer, Messung je Plattform, `docs/voice-assistant-platforms.md`                                                                                                               | US2 (FR-016) | `assistant/echo.rs`, `audio/mixer.rs`                           |
| H   | Android-Vordergrund-Dienst, Benachrichtigung, Audio-Fokus; iOS nur Vordergrund; Android-e2e                                                                                                                                    | US9          | Plugin `holzi-android`, `assistant/mod.rs`                      |
| I   | Eigenes Wake Word: Einsprechen, Prüfung, Probephase; Spike Vorlagen gegen offene Erkennung                                                                                                                                     | US7          | `assistant/wakeword/{templates,enroll}`, Einstellungen          |
| J   | Externer Sprachdienst mit Anzeige und Rückfall                                                                                                                                                                                 | US8 (extern) | `tts/external.rs`, Anbieter-Einstellungen                       |

**Abhängigkeiten**: B → C → D. E, F, G, H, I, J bauen auf D auf und sind untereinander unabhängig;
F braucht die Freigabe-Ansage aus E. Außerhalb dieses Repos und vor D: Training und Veröffentlichung
des Standardmodells in `haex-space/haex-wakeword` (Betreiber, GPU). Die Eichhörnchen-Sprites
entstehen mit dem Bildmodell-Ablauf; bis dahin zeigt die Anzeige Symbole.

**Sichtbarkeit**: Der Schalter im Launcher erscheint erst mit PR D. Bis dahin ist das Gespräch per
Kürzel nur mit der Einstellung „Sprachassistent (Vorschau)“ in KI → Sprache erreichbar.

## Complexity Tracking

| Abweichung                               | Warum nötig                                                                | Einfachere Alternative verworfen, weil                                                      |
| ---------------------------------------- | -------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------- |
| Native C++-Abhängigkeit mit ONNX-Runtime | Pausenerkennung, Wake Word und Sprachausgabe lokal auf fünf Plattformen    | Eigenbau je Aufgabe wäre mehr Code; Plattform-APIs gibt es nicht überall (Linux, Wake Word) |
| Externes Trainings-Repo                  | Das Standardmodell braucht Python, GPU und große, lizenzsaubere Datensätze | Fertige Modelle sind nicht kommerziell lizenziert; Porcupine braucht Online-Aktivierung     |
