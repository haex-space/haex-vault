# Implementation Plan: Structured Agent Tasks for Extensions

> **Abgelöst:** Dieser Plan ist ein historischer Entwurf aus Spec 047 und darf
> nicht als aktueller Implementierungsplan verwendet werden. Die Nachfolge ist
> in [Spec 055](../055-extension-model-access/plan.md) beschrieben.

**Branch**: `feat/structured-agent-tasks` | **Date**: 2026-10-09 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/047-structured-agent-tasks/spec.md`

## Summary

Holzi erhält eine eng begrenzte, asynchrone Task-Schnittstelle für Erweiterungen.
Eine Erweiterung startet einen benannten Task mit Bilddaten und einem versionierten
Schema; Holzi löst das Nutzerprofil auf, prüft Modellfähigkeiten, führt die
Verarbeitung über die vorhandene Provider-/Chat-Abstraktion aus und liefert das
validierte Ergebnis über ein extension-spezifisches Ereignis zurück. Der Task
bekommt standardmäßig keine Chat-Werkzeuge, Shell oder beliebigen Dateizugriff.
Die erste Anwendung ist die Papiernotiz-Erkennung in haex-notes.

## Technical Context

**Language/Version**: Rust/Tauri 2.x, Tokio, Serde; bestehendes TypeScript-SDK konsumiert den Bridge-Vertrag

**Primary Dependencies**: vorhandene `ProviderAdapter`-/`ChatState`-Abstraktion, Extension-Bridge, bestehende Modell- und Provider-Speicher; keine neue Inferenzbibliothek in diesem Feature

**Storage**: bestehende Modell-/Provider-Zeilen und Preferences für die Task-Profilwahl; laufende Task-Zustände nur im Speicher

**Testing**: Rust Unit-/Integrationstests mit vorhandenen Test-Fixtures; Bridge-Vertragstest; kein Netzwerk und kein echtes Modell in Standardtests

**Target Platform**: Holzi Desktop und mobile Runtime mit Extension-Frames; lokale Task-Ausführung bleibt auch ohne Relay verfügbar

**Project Type**: plattformübergreifende Tauri-Desktop-/Mobile-Anwendung mit Extension-Host und lokalem Agent-Runtime

**Performance Goals**: Task-Start und Capability-Prüfung <100 ms ohne Modellstart; ein laufender Task blockiert weder Window-Manager noch Bridge; Ergebnis spätestens nach 180 s mit Timeout-Fehler

**Constraints**: keine Secrets in Anfragen/Ergebnissen; lokale Tasks ohne Remote-Fallback; schema-validierte Ergebnisse; pro Extension begrenzte laufende Tasks; Quellbild bleibt unverändert; Dateien ≤500 LoC

**Scale/Scope**: zunächst ein Task `paper-note-recognition`, wenige parallele Bildtasks pro Extension, keine allgemeine Plugin-Agent-Plattform

## Constitution Check

_GATE: Must pass before Phase 0 research. Re-check after Phase 1 design._

| Regel                               | Status | Begründung                                                                                                                               |
| ----------------------------------- | ------ | ---------------------------------------------------------------------------------------------------------------------------------------- |
| Keine Secrets                       | ✅     | Task-Payloads enthalten Bilddaten und Kennungen, aber keine Zugangsdaten; Provider-Secrets bleiben im Host.                              |
| Keine lokalen absoluten Pfade       | ✅     | Die Extension übergibt Bytes oder hostseitig autorisierte Referenzen; Verträge verwenden keine Maschinenpfade.                           |
| Cross-Repo-Referenzen gepinnt       | ✅     | Diese Spezifikation definiert den Host-Vertrag; SDK-/haex-notes-Änderungen werden in eigenen Worktrees über denselben Vertrag umgesetzt. |
| Relay blockiert lokale Arbeit nicht | ✅     | Lokale Profile benötigen weder Relay noch Cloud-Fallback.                                                                                |
| Worktree/PR/Testtrennung            | ✅     | Arbeit erfolgt auf `feat/structured-agent-tasks`; Tests liegen in separaten Rust-Testdateien; Merge nur per PR.                          |
| ADR-Pflicht                         | ✅     | Die Task-Grenze berührt keine bestehende Core-Principle-Ausnahme; Consent und Locality sind im Vertrag dokumentiert.                     |

## Project Structure

### Documentation

```text
specs/047-structured-agent-tasks/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/task.md
└── tasks.md
```

### Source Code

```text
src-tauri/src/
├── agent_tasks/
│   ├── mod.rs                 # Task-Registry, Lifecycle und Profilauflösung
│   ├── runner.rs              # Fähigkeitsprüfung, Schema-Grenze, Ausführung
│   ├── bridge.rs              # Extension-facing Start-/Cancel-Handler
│   └── *_tests.rs             # reine Lifecycle- und Validierungstests
├── adapters/types.rs          # Task-relevante Input-Fähigkeiten, falls nötig
├── model_capabilities.rs      # Bild/OCR/strukturierte Ausgabe
├── extensions/bridge/
│   ├── methods.rs             # Method-Registrierung und Eingabevalidierung
│   ├── events.rs              # Task-Completion-Event-Routing
│   └── dispatch_tests.rs      # Allowlist-/Sicherheitsregressionen
└── lib.rs                     # Modul- und Command-Registrierung

src-tauri/tests/
└── extension_agent_tasks.rs   # Bridge-Lifecycle und Frame-Isolation
```

**Structure Decision**: Die Task-Grenze bleibt neben dem bestehenden Chat, weil
ein Extension-Bridge-Aufruf keinen freien Chatverlauf und keine allgemeinen
Agent-Werkzeuge erhalten darf. `agent_tasks` kapselt die Task-spezifische
Validierung und ruft die vorhandene Provider-/Session-Abstraktion auf. Die
Extension-Bridge bleibt der einzige Eingang von Frames; das Completion-Event
wird ausschließlich an Frames derselben Extension weitergeleitet.

## Delivery Slices

1. **Vertrag und Fixture**: Task-/Status-/Fehlerformen, Capability-Prüfung und ein deterministischer Ergebnis-Runner ohne Modell.
2. **Host-Lifecycle**: Start, Abschluss, Timeout, Abbruch, Frame-Isolation und Extension-Bridge-Methoden.
3. **Provider-Ausführung**: strukturierter Prompt, Bildinput, JSON-Validierung und Nutzung des ausgewählten Holzi-Profils ohne freie Werkzeuge.
4. **haex-notes-Anbindung**: SDK-/Extension-Adapter, Consent-Anzeige, Mapping in den Canvas und manueller Fallback.

## Complexity Tracking

Keine Verfassungsabweichungen. Der asynchrone Task-Lifecycle ist erforderlich,
damit langsame lokale Vision-Modelle die Extension-Bridge nicht blockieren; die
Komplexität bleibt in einem eigenen Modul und wird mit einem Fixture-Runner
testbar gehalten.

## Post-design Constitution Check

**Ergebnis**: PASS. Keine Zugangsdaten werden über die Extension-Grenze gereicht,
lokale Verarbeitung hat keinen impliziten Remote-Fallback, und die neue
Task-Schnittstelle erhält keine allgemeinen Agent-Werkzeuge. Die laufenden
Ergebnisse bleiben flüchtig; haex-notes entscheidet selbst über die lokale
Persistenz des unveränderten Originals.
