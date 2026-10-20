# Implementation Plan: Erweiterungen nutzen holzis Modelle

**Branch**: `055-extension-model-access` | **Date**: 2026-10-17 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/055-extension-model-access/spec.md`

## Summary

Eine Erweiterung schickt holzi einen Auftrag mit eigener Anweisung, Eingabe (Text, Bilder) und einem
Ergebnisformat. holzi führt ihn mit Modellen aus, die der Nutzer für diese Erweiterung freigegeben hat, prüft
die Ausgabe gegen Wächter und Schema und liefert nur ein brauchbares Ergebnis samt Herkunft. Nichts über den
Auftrag bleibt gespeichert.

Technischer Ansatz (Begründungen in [research.md](./research.md)):

- **Auftragskern** (R1, R2, R4): `agent_tasks` (Spec 047) wird zu `model_jobs` verallgemeinert. Lebenszyklus,
  Besitzer, Abbruch, Rahmen-Haken und Abschluss-Ereignis bleiben; Eingabe, Definitionen, Prüfung und Testdaten
  werden ersetzt. Aufträge haben 1 bis 4 Schritte, ein späterer Schritt liest einen Teil des früheren.
- **Berechtigungen** (R3, R5): zwei neue Arten in `extension_permissions` (`model` mit höchstem Aufwand,
  `modelDestination` für die Einwilligung), dieselbe Abfrage wie überall (Sitzung oder dauerhaft); die
  sitzungsgebundene Entscheidung wird beim Schließen der Tresorsitzung aus dem bestehenden In-Memory-Holder
  entfernt; eine kleine
  Tabelle `extension_model_order` für die Rangfolge.
- **Ziele und Ausweichweg** (R4, R6): Ort jedes Modells (`this_device`, `network`, `cloud`); alle Fragen stellt
  der Start; Ausweichmodelle ohne Einwilligung fallen weg.
- **Wächter und Schema** (R10, R11): ein allgemeiner Prüfer (leer, Schleife, Schriftwechsel, JSON, Schema), die
  eine eigene Schema-Teilmenge für Ergebnisformate.
- **Anbieter** (R7): neuer Adapter `openai_compatible` (auch Gemini, Ollama, vLLM, Schul-Server), „nimmt Bilder
  an“ je Modell als Nutzerangabe.
- **Lokale Bilder** (R8): zweiter Ladeweg über `MultimodalModelBuilder` mit Quantisierung beim Laden;
  Katalogfelder; zuerst ein Versuch, mit dem lokalen OpenAI-kompatiblen Anbieter als Rückfall.
- **Grenzen, nichts speichern** (R9): feste Obergrenzen, Dauer je Erweiterung in den Limits, Registry räumt auf.
- **SDK** (R13): `sdk.models.*` im vault-sdk, eigene Lieferung.

## Technical Context

**Language/Version**: Rust (Edition des Crates `src-tauri`), TypeScript (strict), Vue 3.5, Nuxt 4 (SPA);
vault-sdk (eigenes Repo): TypeScript

**Primary Dependencies**: vorhanden — `mistralrs 0.8.1` (`MultimodalModelBuilder`, `add_image_message`,
`set_constraint`), `reqwest`, `eventsource_stream`, `tokio`, `serde_json`, `image` (für `DynamicImage`; im
Baum über mistralrs, wird bei Bedarf direkte Abhängigkeit). Keine neue Fremdabhängigkeit geplant; ein Base64-Crate
ist schon da (Adapter Anthropic).

**Storage**: `extension_permissions` (zwei neue Arten, keine Schemaänderung); neue Tabelle
`extension_model_order` (Migration `0037`); neues Feld `model_job_timeout_ms` in `ExtensionLimits`;
`providers.adapter = openai_compatible`; Katalogfelder `format`, `vision`, `isq` ([data-model.md](./data-model.md)).
Aufträge selbst: nur Arbeitsspeicher.

**Testing**: `cargo test` (neu `model_jobs/*_tests.rs`, `adapters/openai_compatible_tests.rs`,
Erweiterungen in `extensions/permissions/*_tests.rs`, `providers/*_tests.rs`);
Integration `tests/extension_model_jobs.rs` mit einem Scheinanbieter; Vertragstests für `contracts/`; `pnpm check:*`
für neue reine TS-Module; E2E-Szenario mit der Test-Erweiterung; Versuch zum lokalen Bildmodell als Test mit
`#[ignore]` (braucht Modell und GPU).

**Target Platform**: Linux, macOS, Windows, Android, iOS. Lokale Bildmodelle nur dort, wo lokale Modelle laufen
(Desktop); auf dem Handy läuft die Analyse über einen Anbieter (Spec, Annahmen).

**Project Type**: Desktop- und Mobil-App (Tauri) plus Bibliothek (vault-sdk)

**Performance Goals**: Vorprüfung eines Auftrags < 50 ms (ohne Modell); Abbruch wirkt < 1 s; ein lokaler
Seitenauftrag auf der Zielhardware in der Größenordnung von Sekunden bis wenigen Minuten (Test: 3–130 s je Seite);
kein Messziel für Modelle (Spec: Modellwahl ist Einstellung).

**Constraints**: Aufträge ohne Werkzeuge (FR-026); nichts dauerhaft speichern (FR-028); sitzungsgebundene
Berechtigungen enden mit der Tresorsitzung (FR-016); kein Byte außerhalb des
Geräts ohne Einwilligung (FR-015); Obergrenzen (R9); Dateien < 500 Zeilen; Rust-Tests in `*_tests.rs`;
keine Zugangsdaten in Fehlern, Ereignissen oder Protokollen.

**Scale/Scope**: bis 4 Schritte je Auftrag, 2 laufende Aufträge je Rahmen, einige Erweiterungen, wenige Dutzend
Modelle.

## Constitution Check

_GATE: Must pass before Phase 0 research. Re-check after Phase 1 design._

| Grundsatz                                              | Bewertung                                                                                                                                                  |
| ------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------- |
| I. Keine Geheimnisse in Git                            | Besteht. Anbieterschlüssel liegen verschlüsselt in `providers.credentials`; Testanbieter nutzen Scheinwerte, die Testerweiterung braucht keinen Schlüssel. |
| II. Keine absoluten Pfade                              | Besteht. Modellverzeichnisse stehen als `relative_path` (Katalog, `device_downloaded_models_no_sync`); in der Spec stehen keine Pfade.                     |
| III. Identität ohne Gerätepfad                         | Nicht berührt.                                                                                                                                             |
| IV. Fremde Repos nur mit festem Stand                  | Besteht. Das vault-sdk wird mit vollem Commit oder Version gepinnt; haex-notes folgt in `haextension` mit eigenem Pin.                                     |
| V. Externe Quellen nur aus Allowlist                   | Nicht berührt.                                                                                                                                             |
| VI. Selbständernde Anweisungen nur per Review          | Besteht. Die Anweisung einer Erweiterung ist Eingabe an ein Modell, keine Änderung von holzis Anweisungen; neue Berechtigungsarten kommen per PR.          |
| VII. Relais-Ausfall blockiert nie lokale Arbeit        | Besteht. Lokale Aufträge brauchen kein Netz; Freigaben und Einwilligungen liegen lokal.                                                                    |
| VIII. Keine Verschleierungsanweisung in Agentenausgabe | Besteht. Ergebnisse gehen nur an die Erweiterung; der Wächter verwirft Ausgabe, ändert sie nicht.                                                          |

Nach dem Entwurf erneut geprüft: keine Verletzung. Offene Beobachtung: Ein Cloud-Ziel erhält Schülerfotos;
das deckt FR-015/FR-016 mit sichtbarer Einwilligung. Datenschutz bei Minderjährigen im Betrieb eines
Schul-Servers ist ausdrücklich nicht Teil dieser Spec.

## Project Structure

### Documentation (this feature)

```text
specs/055-extension-model-access/
├── plan.md              # dieses Dokument
├── research.md          # R1–R14
├── data-model.md        # gespeicherte und flüchtige Daten
├── quickstart.md        # Handtest
├── contracts/
│   ├── bridge.md        # extension_model_list / _job_start / _job_cancel, Ereignis
│   └── job-format.md    # Auftrag, Schema-Teilmenge, Aufwand, Wächter
├── checklists/requirements.md
└── tasks.md             # nicht von diesem Befehl (speckit-tasks)
```

### Source Code (repository root)

```text
src-tauri/src/
├── model_jobs/                      # ersetzt agent_tasks/ (R1)
│   ├── mod.rs                       # Konstanten, Re-Exports
│   ├── job.rs                       # Job, Step, Input, Format (Parsen, Vorprüfung)
│   ├── limits.rs                    # Obergrenzen und Dauer je Erweiterung
│   ├── state.rs                     # Registry, JobRecord, Besitzer, Aufräumen (aus agent_tasks)
│   ├── errors.rs                    # JobErrorCode (aus TaskErrorCode)
│   ├── candidates.rs                # Kandidatenliste je Schritt: Freigaben, Rangfolge, Fähigkeit, Ort
│   ├── effort.rs                    # Deckeln und Zuordnung auf Stufen
│   ├── schema.rs                    # Schema-Teilmenge für Ergebnisformate (R11)
│   ├── guard.rs                     # leer, Schleife, Schriftwechsel, JSON, Schema
│   ├── runner.rs                    # Schritte ausführen, Ausweichweg, Abbruch, Dauer
│   ├── model_source.rs              # Adapter für ein Modell beschaffen (Fern: build_adapter; lokal: Platz)
│   ├── place.rs                     # Ort eines Anbieters (this_device/network/cloud)
│   ├── bridge.rs                    # start, cancel, list (Handler)
│   └── *_tests.rs
├── adapters/openai_compatible.rs    # neuer Adapter (R7) + openai_compatible_tests.rs
├── adapters/openai_request.rs       # Rumpf der Anfrage (Nachrichten, Bilder, Aufwand, Werkzeuge)
├── extensions/
│   ├── permissions/{model,target,store}.rs      # Arten Model, ModelDestination (R3)
│   ├── bridge/{allowlist,model_jobs}.rs         # ersetzt agent_tasks.rs, neue Methoden
│   ├── registry/limits.rs                       # + model_job_timeout_ms
│   └── commands/model_grants.rs                 # Freigabe, Rangfolge, Widerruf (Einstellungen)
├── providers/mod.rs                 # validate_adapter/build_adapter/add_provider: openai_compatible
├── providers/capabilities.rs        # set_model_user_capabilities: Angaben des Nutzers je Modell
├── llm/local/{loader,stream}.rs     # Ladeweg multimodal, Bildanhänge (R8, Lieferung C)
├── catalog/model_catalog.json       # format, vision, isq
├── model_capabilities.rs            # lokale Fähigkeiten aus dem Katalog
└── identity/migrations.rs           # 0037_extension_model_access, Trigger-Version

src/
├── components/extensions/PermissionRequestDialog.vue   # Zweige model, modelDestination
├── components/settings/extensions/ExtensionModelsView.vue   # Freigaben, Rangfolge, Aufwand, Widerruf
├── components/settings/extensions/ExtensionLimitsView.vue   # + Dauer
├── components/settings/ModelServer{s,}View.vue              # Modell-Server (openai_compatible), „nimmt Bilder an“
├── lib/extensions/modelGrants.ts                       # reine Logik (Rangfolge, Aufwandsstufen)
└── i18n/…                                              # Texte de/en

src-tauri/tests/
├── extension_model_jobs.rs          # Integration mit Scheinanbieter
└── fixtures/extension_e2e/model-jobs/   # signierte Test-Erweiterung (wie agent-tools)
```

**Structure Decision**: Ein neues Modul `model_jobs` statt einer Erweiterung von `agent_tasks`, weil alles bis auf
den Lebenszyklus ersetzt wird (R1). Die Brücke bleibt im vorhandenen Erweiterungsweg. Das SDK ist ein eigenes
Repo und eine eigene Lieferung (R13).

## Lieferungen

|     | Inhalt                                                                                                                        | Reihenfolge           |
| --- | ----------------------------------------------------------------------------------------------------------------------------- | --------------------- |
| A   | Adapter `openai_compatible`, Anbieter-Dialog, „nimmt Bilder an“ je Modell                                                     | 1                     |
| B   | Auftragskern mit Fernmodellen: Brücke, Berechtigungsarten, Rangfolge, Wächter, Schema, Grenzen, Ausweichweg; Test-Erweiterung | 2                     |
| D   | Dialoge und Einstellungen (zusammen mit B für die Abfragen)                                                                   | 2                     |
| C   | Lokale Bilder: Versuch, Ladeweg, Katalog, Fähigkeiten                                                                         | 3, mit frühem Versuch |
| E   | vault-sdk `sdk.models.*`                                                                                                      | 4                     |

Jede Lieferung ist ein eigener PR mit grünen Prüfungen (Vorbild: Spec 018).

## Risiken

- **Lokales Bildmodell über mistral.rs** ist nicht erprobt (R8). Frühester Versuch der Lieferung C; Rückfall
  über einen lokalen OpenAI-kompatiblen Anbieter, ohne die Spec zu verletzen.
- **Ein lokaler Platz**: Ein Auftrag mit anderem lokalen Modell verdrängt das Chatmodell (R8). Das kann dem Nutzer
  auffallen; die Oberfläche sagt es beim Freigeben, und der Chat lädt nach.
- **Qualität**: Der Wächter erkennt nur Offensichtliches (R10). Ob ein Modell Handschrift und Tabellen gut genug
  liest, entscheidet die Messung außerhalb dieser Spec.
- **Aufräumen der Registry** (R1) und **nichts speichern** (R9) brauchen Tests auf Datenbankinhalt (SC-011).

## Complexity Tracking

Keine Verletzung der Grundsätze, daher keine Einträge.
