# Implementation Plan: Paper Notes to Editable Canvas

**Branch**: `feat/paper-notes-canvas` | **Date**: 2026-10-08 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/046-paper-notes-canvas/spec.md`

**Implementation target**: `haex-space/haextension` at revision `1d24a491f24a20f5579ab16304c0a081fcd1fca8`, path `apps/haex-notes/`. This holzi repository contains the design artifacts; implementation must be proposed in the target repository's own worktree.

## Summary

Schüler können ein Foto handschriftlicher Notizen importieren und daraus einen aufgeräumten, weiterhin bearbeitbaren Canvas-Entwurf erzeugen. Das unveränderte Foto bleibt als Asset auf einer gesperrten Originalebene erhalten. Erkannter Text wird als editierbarer Text mit auswählbarer Handschrift-Darstellung angelegt; Zeichnungen und Diagramme werden als eigenständige Bildobjekte übernommen. Unsichere Erkennung und Rechtschreibvorschläge bleiben überprüfbar und dürfen nicht stillschweigend den Inhalt ändern.

Die Umsetzung erweitert das bestehende haex-notes-Dokumentmodell und bindet die Erkennung über einen anbieterneutralen Vertrag an den strukturierten Agent-Task von Holzi. haex-notes übernimmt Dateiauswahl, lokale Aufbereitung einer Arbeitskopie, Qualitätswarnungen, Originalerhalt, Canvas-Mapping und Export. Der Agent-Task übernimmt ausschließlich die strukturierte Interpretation des aufbereiteten Bildes und liefert Provenienz sowie Verarbeitungsstatus zurück. Der erste Release enthält keine persönliche Handschriftprofilierung und keinen direkten Anbieter-SDK in der Extension. Holzi wählt anhand des Nutzerprofils das Harness und das Vision-/OCR-fähige Modell; der Verarbeitungsort wird vor einer externen Übertragung ermittelt und angezeigt. Der PDF-Export rendert zunächst den konfigurierten sichtbaren Canvas als hochauflösendes Bild in eine PDF, damit Darstellung und Handschrift-Fonts mit dem Canvas übereinstimmen.

## Technical Context

**Language/Version**: TypeScript strict, Vue 3.5, Nuxt 4.2 SPA, haex-notes package 0.1.7

**Primary Dependencies**: Bestehender haex-vault 2.x (Legacy) SDK einschließlich der Holzi-Agent-Task-Brücke, Drizzle ORM, VueUse, perfect-freehand; zusätzlich `pdf-lib` für den PDF-Export. Die OCR-/Korrekturkomponente wird über den strukturierten Agent-Task angesprochen und nicht als konkretes Modell oder Anbieter-SDK in die Extension eingebaut.

**Storage**: Bestehende SQLite-Seitentabelle für strukturiertes Import- und Prüfdaten-JSON; bestehender dateibasierter Asset-Store für Originalfoto, lokale Arbeitskopie und erzeugte Ausschnitte. Große Binärdaten und Font-Dateien werden nicht in der Datenbank oder als Data-URL gespeichert. Die Datenbank speichert nur Font-ID und Version/Hash.

**Testing**: Vitest im haex-notes-Paket; reine Daten-, Mapping-, Command-, Asset- und Renderer-Tests in separaten `*.test.ts`-Dateien. Ein manueller End-to-End-Durchlauf folgt [quickstart.md](./quickstart.md), da echte Provider, Dateiauswahl und PDF-Betrachter Umgebungsgrenzen haben.

**Target Platform**: Von haex unterstützte Desktop- und Mobilumgebungen, in denen die haex-notes-Extension läuft. Der Import erfolgt innerhalb der Extension mindestens über Dateiauswahl; eine verfügbare Kamera kann zusätzlich genutzt werden. Drag-and-drop ist optional und keine MVP-Voraussetzung.

**Project Type**: Private Nuxt/Vue-Extension mit Canvas-Editor, lokaler Datenbank und Dateisystem-Assets.

**Performance Goals**: Der Editor bleibt bei einer normalen Einzelnotiz während Auswahl, Verschieben und Skalieren flüssig; Import und Erkennung liefern für die annotierten Einzelbild-Fixtures innerhalb des in SC-001 gesetzten Drei-Minuten-Ziels einen ersten Entwurf oder eine verständliche Warnung. Bilder werden einzeln verarbeitet; Verarbeitung und Asset-Laden dürfen die UI nicht blockieren.

**Constraints**: Originaldaten bleiben unverändert; Korrekturen benötigen Bestätigung; externe Verarbeitung wird vorab angezeigt; Provider-Fehler dürfen den manuellen Canvas nicht verhindern; keine Geheimnisse in Git; keine großen Data-URLs für dauerhaft gespeicherte Fotos; keine persönliche Handschrift im ersten Release.

**PDF Defaults**: Der Export startet mit automatischer Seitengröße und Ausrichtung, beibehaltener Seitenproportion, hoher Qualität, aktuellem sichtbarem Canvas-Zustand und einer Notizseite pro PDF-Seite. Papierformat, Ausrichtung, Ränder, Qualität und Originalebenen-Sichtbarkeit bleiben konfigurierbar.

**Scale/Scope**: Erste Ausbaustufe für einzelne Notizbilder mit mehreren Text- und Bildregionen. Batch- und Mehrseitenimporte sowie persönliche Handschriftprofile sind nicht Teil der ersten Implementierung.

## Constitution Check

_GATE: Must pass before Phase 0 research. Re-check after Phase 1 design._

| Regel                                                 | Stand                                                                                                                                                                                                                          |
| ----------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| I. Keine Geheimnisse in Git                           | ✅ Provider-Verträge enthalten keine Schlüssel; Zugangsdaten werden ausschließlich zur Laufzeit aus der Geräte-/Nutzerkonfiguration verwendet.                                                                                 |
| II. Keine lokalen absoluten Pfade in Versioned Config | ✅ Spezifikation, Plan und Verträge verwenden nur Repository-Pfade und Laufzeitbegriffe.                                                                                                                                       |
| III. Geräteunabhängige Projektidentität               | ✅ Das Implementierungsziel wird über Repository-URL und vollständige Revision referenziert, nicht über einen lokalen Pfad.                                                                                                    |
| IV. Fremde Inhalte mit unveränderlicher Revision      | ✅ haex-space/haextension ist mit Revision `1d24a491f24a20f5579ab16304c0a081fcd1fca8` und konkreten Pfaden referenziert.                                                                                                       |
| V. Externe Quellen nur per Allowlist                  | ✅ Es wird keine neue Harness-Quelle aktiviert.                                                                                                                                                                                |
| VI. Selbständernde Anweisungen reviewen               | ✅ Keine Constitution-, Skill- oder Berechtigungsdatei wird geändert.                                                                                                                                                          |
| VII. Relay blockiert lokale Arbeit nie                | ✅ Originalimport, Canvas-Speicherung und manueller Fallback bleiben auch bei nicht verfügbarem Provider nutzbar.                                                                                                              |
| VIII. Keine Verschleierung                            | ✅ Externe Verarbeitung und ihr Ziel werden dem Nutzer sichtbar gemacht.                                                                                                                                                       |
| Worktree und PR-Workflow                              | ✅ Spec und Plan liegen auf `feat/paper-notes-canvas` in einem eigenen Worktree; Implementierung folgt später im Zielrepository über dessen Review-Workflow.                                                                   |
| Tests getrennt von Produktion                         | ✅ Neue Tests werden in separaten Vitest-Dateien geplant.                                                                                                                                                                      |
| Graphify-first                                        | ✅ Die begrenzte Graph-Abfrage wurde ausgeführt. Im holzi-Graphen gab es keine passende haex-notes-Implementierung; im Zielrepository werden die vorhandenen Dokument-, Command-, Asset- und Renderer-Abstraktionen erweitert. |

**Gate-Ergebnis vor der Recherche**: PASS. Es gibt keine offene Verfassungsabweichung, die eine Planänderung oder ein ADR erfordert.

## Project Structure

### Documentation (this feature)

```text
specs/046-paper-notes-canvas/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   └── recognition-provider.md
├── checklists/requirements.md
└── tasks.md             # folgt in der Tasks-Phase
```

### Source Code (target repository at the pinned revision)

```text
apps/haex-notes/
├── app/components/notes/
│   ├── PageCanvas.vue                 # Text-/Bild-Rendering, Auswahl und Ebenenansicht
│   ├── PaperNoteImportDialog.vue      # Kamera, Dateiimport, Verarbeitungsort und Aufbereitung
│   └── PaperNoteReviewPanel.vue       # Unsicherheiten, Vorschläge und Originalausschnitte
├── app/composables/
│   ├── useAssetStore.ts                # bestehend: Bytes und Metadaten
│   └── usePaperNoteRecognition.ts      # Holzi-Agent-Task-Adapter und Status
├── app/database/schemas/index.ts       # paper_import-Seitenfeld
├── app/lib/
│   ├── paperNotes/
│   │   ├── importPage.ts               # Asset, Ebenen und Recognition-Mapping
│   │   ├── preprocess.ts               # lokale Arbeitskopie und Qualitätswarnungen
│   │   ├── recognition.ts              # provider-neutrale Typen/Validierung
│   │   ├── review.ts                   # Vorschlags- und Review-Zustände
│   │   ├── exportPdf.ts                # konfigurierter sichtbarer Canvas → PDF
│   │   └── fonts.ts                    # gebündelte Fonts, IDs und Fallbacks
│   │   └── fixtures/                    # annotierte, de-identifizierte Referenzbilder
│   │       └── manifest.json            # erwartete Regionen und Unsicherheiten
│   ├── render/elements.ts              # bestehend: Text- und Bild-Rendering ergänzen
│   └── migratePage.ts                  # bestehend: paper_import serialisieren
├── app/stores/notebook.ts              # bestehende Commands/Save-Flows erweitern
├── app/types/document.ts               # bestehende Element- und Seitenmodelle erweitern
├── public/fonts/                       # nur lizenzierte, gebündelte Fonts
└── app/lib/paperNotes/*.test.ts        # Tests getrennt von Produktionsdateien
```

**Structure Decision**: Die Funktion bleibt innerhalb der bestehenden haex-notes-Extension. Das vorhandene `PageDoc`-/`PageLayer`-/`PageElement`-Modell wird erweitert; es wird kein paralleles Dokumentmodell eingeführt. Ein eigener `paperNotes/`-Bereich kapselt Import, Provider-Mapping, Review und Export. Die UI nutzt den bestehenden Notebook-Store, die Undo-Kommandos und den Asset-Store.

### Delivery slices

1. **Canvas-Basis**: Text- und Bild-Elemente rendern, Assets asynchron laden, auswählen/verschieben/skaliert bearbeiten und mit bestehenden Commands speichern.
2. **Papierimport**: Original-Asset, lokale Arbeitskopie, Qualitätswarnung, gesperrte Originalebene, normalisierte Seitengröße, Importfehler und manueller Fallback.
3. **Recognition und Review**: zweistufiger Agent-Task-Vertrag, Fixture-Provider für Tests, Region-Mapping, Konfidenzmarkierungen, Vorschläge, Originalausschnitte und Font-Auswahl.
4. **Export und Durchlauf**: konfigurierbare PDF-Ausgabe mit sinnvollen Defaults, Persistenz-/Wiederöffnen-Tests, annotierte Referenz-Fixtures und manueller Quickstart.

### Post-design Constitution Check

**Gate-Ergebnis nach Phase 1**: PASS. Das Design speichert keine Zugangsdaten, referenziert das externe Zielrepository unveränderlich, trennt Tests von Produktionscode, bewahrt Originale und macht externe Verarbeitung sichtbar. Die Agent-Auswahl bleibt in Holzi; die Extension erhält nur die für den Task nötige strukturierte Antwort. Die neuen Artefakte enthalten keine lokalen absoluten Pfade und keine Änderung an Anweisungs- oder Verfassungsdateien.

## Complexity Tracking

Keine Verfassungsabweichungen. Die separate `paperNotes/`-Kapselung ist eine konkrete Domänenstruktur innerhalb der bestehenden Extension und kein paralleles Dokumentmodell.
