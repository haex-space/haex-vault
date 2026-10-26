# Implementation Plan: Paper Notes to Editable Canvas

**Branch**: `feat/paper-notes-canvas` | **Date**: 2026-10-21 (überarbeitet; erste Fassung 2026-10-08) | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/046-paper-notes-canvas/spec.md`

**Implementation target**: `haex-space/haextension` at revision `6dfb015198c99018236f5a8e958a916942ab971c`, path
`apps/haex-notes/`. This holzi repository contains the design artifacts; implementation must be proposed in the target
repository's own worktree.

## Summary

Schüler importieren ein Foto handschriftlicher Notizen, eines Tafelbilds oder eines Arbeitsblatts und erhalten einen
aufgeräumten, bearbeitbaren Canvas-Entwurf. Das unveränderte Foto hängt als Anhang „Original“ an der Seite (US5, FR-022). Texte
werden zu bearbeitbarem Text in einer wählbaren Handschrift, Tabellen zu Zeilen und Zellen, Zeichnungen zu
verschiebbaren Bildausschnitten. Es gibt keinen Prüfteil: Ein Korrekturschritt setzt Rechtschreibung und eindeutige
Lesefehler direkt ein.

Die Erkennung ist ein Modellauftrag an holzi (Spec 055, vault-sdk `models.runAsync`). haex-notes beschreibt zwei
Schritte, Lesen und Korrigieren, mit eigener Anweisung und eigenem Format
([contracts/page-job.md](./contracts/page-job.md)). holzi wählt das freigegebene Modell, fragt vor dem Versand
außerhalb des Geräts und verwirft kaputte Ausgabe. haex-notes übernimmt Import, Arbeitskopie (Zuschnitt,
Perspektive, Drehung), Abbildung auf den Canvas, Neuaufbau der Seite und PDF-Export.

## Technical Context

**Language/Version**: TypeScript strict, Vue 3.5, Nuxt 4.2 SPA, haex-notes 0.1.7

**Primary Dependencies**: `@haex-space/vault-sdk` ab 4.7.1 (heute `^4.1.0`; `models.runAsync` kam mit 4.6.0, die
Wiederholung nach der Antwort des Nutzers auf eine Frage von holzi mit 4.7.1), Drizzle ORM, VueUse, perfect-freehand;
neu `pdf-lib` für den PDF-Export. Kein Anbieter- oder OCR-SDK.

**Storage**: Seitentabelle mit neuer JSON-Spalte `paper_import` (Migration `0005`); `TableElement.cells` im
vorhandenen `layers`-JSON. Original, Arbeitskopie und Bildausschnitte im Asset-Store; keine Data-URLs für Fotos.

**Testing**: Vitest im Paket, Tests in eigenen `*.test.ts`-Dateien. Die SDK-Schicht steckt hinter `runPageJob(job)`
und wird im Test durch eine Attrappe ersetzt. Ein Handtest gegen holzi mit Ollama folgt
[quickstart.md](./quickstart.md).

**Target Platform**: Alle Plattformen, auf denen holzi haex-notes ausführt. Import über die Dateiauswahl, die Kamera
zusätzlich, wo vorhanden. Das Handy nimmt nur das Foto auf; gelesen wird dort, wo der Nutzer in holzi ein Modell
freigegeben hat.

**Project Type**: Nuxt/Vue-Erweiterung mit Canvas-Editor, Datenbank und Datei-Assets.

**Performance Goals**: Erster Entwurf oder verständlicher Hinweis innerhalb von drei Minuten je Foto (SC-001).
Gemessen: `qwen3-vl:4b-instruct` über Ollama liest eine Seite auf einer 8-GB-Karte in 12–18 s (Spec 055 research R8).
Die Oberfläche blockiert während der Erkennung nicht; ein Abbruch ist jederzeit möglich (`cancelAsync`).

**Constraints**: Das Original bleibt unverändert; Fehler verhindern nie den manuellen Canvas; keine Zugangsdaten in
haex-notes; keine Geheimnisse in Git; keine persönliche Handschrift im ersten Release.

**PDF Defaults**: automatische Seitengröße und Ausrichtung, beibehaltene Proportion, hohe Qualität, sichtbarer
Canvas-Zustand, eine Notizseite je PDF-Seite. Papierformat, Ausrichtung, Ränder und Qualität sind einstellbar; Anhänge
gehören nicht in die PDF.

**Scale/Scope**: Einzelne Fotos mit mehreren Text-, Tabellen- und Bildregionen. Kein Batch- oder Mehrseitenimport.

## Constitution Check

_GATE: Must pass before Phase 0 research. Re-check after Phase 1 design._

| Regel                                                 | Stand                                                                                                                                       |
| ----------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------- |
| I. Keine Geheimnisse in Git                           | ✅ haex-notes kennt weder Anbieter noch Schlüssel; holzi hält sie (Spec 055).                                                               |
| II. Keine lokalen absoluten Pfade in Versioned Config | ✅ Nur Repository-Pfade und Laufzeitbegriffe.                                                                                               |
| III. Geräteunabhängige Projektidentität               | ✅ Zielrepository über URL und vollständige Revision.                                                                                       |
| IV. Fremde Inhalte mit unveränderlicher Revision      | ✅ haex-space/haextension mit Revision `6dfb015198c99018236f5a8e958a916942ab971c`; vault-sdk mit fester Version und Integrität im Lockfile. |
| V. Externe Quellen nur per Allowlist                  | ✅ Keine neue Quelle; Modelle und Ziele kommen über holzis Freigaben.                                                                       |
| VI. Selbständernde Anweisungen reviewen               | ✅ Keine Constitution-, Skill- oder Berechtigungsdatei wird geändert.                                                                       |
| VII. Relay blockiert lokale Arbeit nie                | ✅ Import, Speichern und manuelle Bearbeitung gehen ohne Modell.                                                                            |
| VIII. Keine Verschleierung                            | ✅ holzi fragt vor dem Versand außerhalb des Geräts; haex-notes zeigt Modell und Ort der Erkennung.                                         |
| Worktree und PR-Workflow                              | ✅ Umsetzung im Zielrepository in eigenem Worktree über dessen Review.                                                                      |
| Tests getrennt von Produktion                         | ✅ Eigene Vitest-Dateien.                                                                                                                   |

**Gate-Ergebnis vor der Recherche**: PASS.

## Project Structure

### Documentation (this feature)

```text
specs/046-paper-notes-canvas/
├── plan.md
├── research.md          # R1–R12
├── data-model.md
├── quickstart.md
├── contracts/
│   └── page-job.md      # Auftrag, Anweisungen, Formate, Abbildung, Fehler
├── checklists/requirements.md
└── tasks.md
```

### Source Code (target repository at the pinned revision)

```text
apps/haex-notes/
├── app/components/notes/
│   ├── PageCanvas.vue                  # Text, Bild und Tabellenzellen zeichnen und bearbeiten, Ebenenansicht
│   └── PaperNoteImportDialog.vue       # Kamera/Datei, Drehen, Qualitätshinweis, Lesen, Abbrechen, Herkunft
├── app/composables/
│   ├── useAssetStore.ts                # bestehend
│   └── usePaperNoteRecognition.ts      # runPageJob über sdk.models, Stand und Abbruch
├── app/database/
│   ├── schemas/index.ts                # Spalte paper_import
│   └── migrations/0005_*.sql           # neue Spalte
├── app/lib/
│   ├── paperNotes/
│   │   ├── pageJob.ts                  # Auftrag bauen (Anweisungen, Formate aus contracts/page-job.md)
│   │   ├── mapResult.ts                # Ergebnis → Elemente, Herkunft, Fehler
│   │   ├── layout.ts                   # Quellboxen → Seitenkoordinaten, Kanten, Überlappungen
│   │   ├── workingCopy.ts              # Zuschnitt, Perspektive, Drehung, Qualitätshinweis, JPEG
│   │   ├── importPage.ts               # Assets, Ebenen, paperImport
│   │   ├── closeUp.ts                  # Nahfoto einer Stelle: einpassen, erzeugte Elemente ersetzen
│   │   ├── exportPdf.ts                # sichtbarer Canvas → PDF
│   │   ├── fonts.ts                    # Handschriftstile und Fallback
│   │   └── fixtures/                   # de-identifizierte Fotos und erwartete Ergebnisse
│   ├── render/elements.ts              # bestehend: text, image, Zelltext ergänzen
│   ├── tableActions.ts                 # bestehend: cells beim Einfügen/Entfernen
│   ├── collectAssets.ts                # bestehend: neue Asset-Quellen
│   └── migratePage.ts                  # bestehend: paperImport und cells
├── app/stores/notebook.ts              # bestehende Commands und Speichern
├── app/types/document.ts               # TableElement.cells, PaperNoteImport
├── public/fonts/                       # nur frei lizenzierte Handschrift-Fonts
└── app/lib/**/*.test.ts                # Tests getrennt von Produktion
```

**Structure Decision**: Alles bleibt in haex-notes. Das vorhandene `PageDoc`-Modell wird erweitert, kein paralleles
Dokumentmodell. `paperNotes/` kapselt Auftrag, Abbildung, Layout, Arbeitskopie, Import und Export; die Oberfläche
nutzt Notebook-Store, Undo-Commands und Asset-Store.

### Delivery slices

1. **Canvas-Basis**: `text` und `image` zeichnen, Assets asynchron laden, Tabellenzellen (`cells`) zeichnen und
   bearbeiten, auswählen, verschieben, skalieren, speichern.
2. **Import**: Original-Asset, Arbeitskopie mit Drehen und Qualitätshinweis, Anhang „Original“, `paperImport`,
   manueller Weg bei jedem Fehler.
3. **Erkennung**: vault-sdk auf 4.7.1, `runPageJob`, Auftrag mit zwei Schritten, Abbildung mit Layout, Drehhinweis
   und einmaliges Nachlesen, Herkunft, Fehlerhinweise, Abbruch.
4. **Export und Durchlauf**: PDF mit Voreinstellungen, Speichern und Wiederöffnen, Fixtures, Handtest gegen holzi
   mit Ollama.

### Post-design Constitution Check

**Gate-Ergebnis nach Phase 1**: PASS. haex-notes speichert keine Zugangsdaten und keine Adresse eines Modells,
verweist auf das Zielrepository unveränderlich, trennt Tests von Produktion, bewahrt das Original und überlässt
Modellwahl und Einwilligung holzi.

## Complexity Tracking

Keine Verfassungsabweichungen.
