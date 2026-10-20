# Research: Paper Notes to Editable Canvas

**Stand**: 2026-10-08

**Zielrepository**: `https://github.com/haex-space/haextension.git`

**Unveränderliche Revision**: `1d24a491f24a20f5579ab16304c0a081fcd1fca8`

## R1 — Implementierungsgrenze und vorhandene Abstraktionen

- **Decision**: Die Spezifikation und Planung liegen in holzi; die eigentliche Implementierung gehört in `haextension/apps/haex-notes/` bei der oben genannten Revision. Im Zielrepository werden vorhandene Dokument-, Command-, Asset- und Renderer-Abstraktionen erweitert.
- **Rationale**: Der aktuelle haex-notes-Code lebt in einem separaten Repository. Ein paralleles Modell in holzi würde die Extension nicht verändern und könnte zu auseinanderlaufenden Datenformaten führen.
- **Alternatives considered**: Implementierung direkt in holzi (verworfen: dort existiert keine haex-notes-Quelle); separates Importmodell neben `PageDoc` (verworfen: würde Persistenz, Undo und Teilen doppeln).
- **Evidence**: `apps/haex-notes/app/types/document.ts`, `app/lib/commands.ts`, `app/composables/useAssetStore.ts`, `app/lib/collectAssets.ts`.

## R2 — Originalfoto als gesperrte Ebene statt Hintergrund-Overlay

- **Decision**: Das Originalfoto wird als Asset gespeichert und als `ImageElement` in einer eigenen sichtbaren, gesperrten Ebene angelegt. `PageBackground` bleibt für Papierfarbe und Lineatur zuständig.
- **Rationale**: `PageLayer` besitzt bereits `visible` und `locked`; `ImageElement` besitzt Position, Größe, Rotation, Quelle und Deckkraft. Dadurch kann das Original ein- und ausgeblendet werden und bleibt unabhängig von bereinigtem Text und Zeichnungen. Das bestehende Hintergrund-Overlay wird in `PageCanvas.vue` aktuell nur für Inline-Bilder geladen und ist nicht als Layer auswählbar.
- **Alternatives considered**: Original als `PageBackground.overlay` speichern (verworfen: eingeschränkte Asset-Lade- und Layer-Semantik); Original in Text-/Recognition-JSON einbetten (verworfen: große Binärdaten und keine direkte Canvas-Darstellung).
- **Evidence**: `app/types/document.ts:33-35,68-77,123-129`, `app/components/notes/PageCanvas.vue:107-142`.

## R3 — Provenienz in einem optionalen Seitenfeld

- **Decision**: Die Seite erhält ein optionales `paperImport`-Objekt. Es verweist auf das Original-Asset, hält Verarbeitungsstatus und Provider-Herkunft und speichert pro Recognition-Region Quellbox, Erkennung, Vorschläge, Review-Zustand und erzeugte Element-IDs.
- **Rationale**: Element-Geometrie und darstellbarer Inhalt bleiben im bestehenden `PageElement`-Modell. Review- und Provenienzdaten sind dagegen keine visuellen Elemente und sollten nicht in `TextElement` oder `ImageElement` vervielfacht werden. Ein optionales JSON-Feld ist mit der vorhandenen Seitenserialisierung kompatibel und lässt ältere Seiten unverändert.
- **Alternatives considered**: Eigene normalisierte Datenbanktabellen (verworfen für den ersten Release: zusätzlicher Synchronisations- und Migrationsaufwand); Provenienz nur im UI-State (verworfen: nach Speichern/Öffnen und Teilen verloren).
- **Evidence**: `app/database/schemas/index.ts:42-72`, `app/lib/migratePage.ts:149-201`.

## R4 — Provider-neutraler Recognition-Vertrag

- **Decision**: Die Extension kennt nur einen kleinen Recognition-Vertrag. Ein Adapter kann lokal oder extern arbeiten und liefert normalisierte Regionen, Text, Konfidenz, Vorschläge und sichtbare Verarbeitungsmetadaten. Ein Fixture-Provider wird für Tests und UI-Entwicklung verwendet.
- **Rationale**: Der Nutzer möchte keine Bindung an einen Hersteller oder ein bestimmtes Modell. Ein Provider-SDK im Canvas würde diese Bindung erzwingen und erschwert lokalen Betrieb. Die Anwendung kann die Übertragung vor dem Start anzeigen und bei einem Provider-Ausfall den Import trotzdem als manuellen Canvas speichern.
- **Alternatives considered**: Direkter SDK-Aufruf eines einzelnen KI-Anbieters (verworfen: Bindung und Geheimnisverwaltung); generatives Bild-zu-Bild-„Schönmachen“ (verworfen: nicht nachvollziehbare Textänderungen und schlechte Editierbarkeit); OCR ohne Region-/Konfidenzdaten (verworfen: keine sinnvolle Review-UI).
- **Evidence**: Der Vertrag ist in [contracts/recognition-provider.md](./contracts/recognition-provider.md) festgelegt; die Spezifikation fordert Bestätigung für Korrekturen und einen manuellen Fallback.

## R9 — Holzi als Modell-/Auftragsvermittler

- **Decision**: haex-notes ruft für die automatische Aufbereitung `models.runAsync` aus Spec 055 mit dem Auftrag „Seitenerkennung“ auf. Die Extension kennt weder Qwen, einen Cloud-Anbieter noch ein Harness direkt.
- **Rationale**: Holzi besitzt bereits die gemeinsame Provider-/Modellabstraktion und Nutzerkonfiguration. Dadurch kann derselbe Auftrag lokal oder extern ausgeführt werden, ohne dass haex-notes Zugangsdaten, Modell-Ladecode oder Anbieterlogik dupliziert.
- **Important boundary**: Der bestehende Holzi-Agent-Chat und seine freien Antworten sind nicht das Datenformat des Canvas. Der Auftrag muss Bildinput, Fähigkeitsprüfung und JSON-Schema-Validierung kapseln.
- **Model implication**: Das vorhandene textbasierte Qwen3-Profil reicht für eine Fotoseite nicht aus. Ein lokales Vision-/OCR-Modell, zum Beispiel ein Qwen-VL-Modell, oder ein spezialisiertes OCR-Modell muss als für diesen Task geeignet registriert sein. Qwen3-VL dokumentiert OCR, Layout-/Positionsinformationen und mehrere Modellgrößen; die konkrete Modellwahl bleibt Holzi-Konfiguration.
- **Alternatives considered**: Direkter HTTP-Aufruf aus haex-notes (verworfen: doppelte Provider-/Credentiallogik); generischer Chat-Aufruf ohne Schema (verworfen: nicht zuverlässig in Canvas-Regionen überführbar); nur die vorhandene SDK-Aktion `actions.register` (verworfen als Primärpfad: sie beschreibt aktuell Agent→Extension-Aktionsaufrufe, während der Button Extension→Agent startet).

## R5 — Text bleibt semantisch bearbeitbar, Handschrift ist Darstellung

- **Decision**: Recognition-Text wird in `TextElement.text` gespeichert; `fontFamily` referenziert einen registrierten Handschriftstil. Roh-Erkennung und Vorschläge bleiben in `paperImport.regions`. Zeichnungen werden als lokal zugeschnittene `ImageElement`s gespeichert.
- **Rationale**: Der Schüler kann Wörter nachträglich korrigieren, ohne ein Bild neu generieren zu müssen. Ein Font liefert im ersten Release ein kontrollierbares und reversibles Ergebnis; persönliche Handschriftprofile bleiben vollständig außerhalb des Umfangs.
- **Alternatives considered**: Text ausschließlich als generiertes Bild speichern (verworfen: nicht editierbar); persönliche Stilgenerierung im ersten Release (verworfen: Datenschutz-, Qualitäts- und Modellrisiko); automatische Vektorisierung von Zeichnungen (verworfen: unnötiger Umfang für den MVP).
- **Evidence**: `app/types/document.ts:53-66,68-77`; die aktuelle Elementliste sieht Text und Bilder bereits vor.

## R6 — Gemeinsame Rendering-Grundlage für Canvas und PDF

- **Decision**: Zuerst werden Text- und Bild-Elemente im bestehenden Renderer implementiert. Der PDF-Export erzeugt aus dem sichtbaren Seitenzustand einen hochauflösenden Raster-Snapshot und bettet diesen in eine PDF-Datei ein. Ein späterer Release kann echten Text-/Vektor-Export ergänzen.
- **Rationale**: Canvas und PDF zeigen dadurch exakt dieselben Fonts, Positionen, Deckkraftwerte und Bildausschnitte. `pdf-lib` unterstützt das Erzeugen von PDFs und das Einbetten von Bildern in JavaScript-Umgebungen ([Dokumentation](https://pdf-lib.js.org/)); eine zusätzliche Font-Einbettung ist für den ersten visuellen Export nicht nötig.
- **Alternatives considered**: Browser-Druckdialog (verworfen: nicht als deterministischer Dateiexport testbar); sofortiger Vektor-/Text-PDF-Export (verworfen: abweichende Font-Metriken, komplexerer SVG-/Text-Renderer und größerer MVP-Umfang).

## R7 — Asset- und Fehlerstrategie

- **Decision**: Das Original und erzeugte Zeichnungsausschnitte werden über `useAssetStore.putAsync()` als gehashte Assets gespeichert. Erkennung, Zuschnitt und Rendering arbeiten asynchron; Fehler werden im `paperImport.processing`-Status gespeichert. Bei Fehlern bleibt mindestens die Originalebene nutzbar.
- **Rationale**: Der vorhandene Asset-Store dedupliziert Bytes und hält Binärdaten außerhalb der Datenbank. Die bestehende Serialisierung und `collectAssetIds()` können um die neuen Bildquellen erweitert werden.
- **Alternatives considered**: Data-URLs im Seiten-JSON (verworfen: große DB-Transaktionen und schlechte Synchronisierung); temporäre Browser-Blobs ohne Asset-Verweis (verworfen: nach Reload nicht wiederherstellbar).
- **Evidence**: `app/composables/useAssetStore.ts:48-101`, `app/lib/collectAssets.ts:10-25`.

## R8 — Teststrategie und inkrementelle Lieferung

- **Decision**: Pure Mapping-, Serialisierungs-, Review- und Renderlogik wird mit Vitest getestet. Ein Fixture-Provider ermöglicht reproduzierbare Recognition-Ergebnisse; ein kleiner manueller Durchlauf deckt echte Foto- und PDF-Grenzen ab.
- **Rationale**: Der aktuelle Paketstand besitzt bereits Vitest und testet Dokumentmodell, Commands, BBox, Assets und Renderer. Externe KI-Dienste und echte Dateiauswahl wären in Unit-Tests nicht deterministisch.
- **Alternatives considered**: Nur manuelle Tests (verworfen: Provenienz, Review-Zustände und Undo können regressieren); echte Provider-Aufrufe in jeder Testausführung (verworfen: nicht reproduzierbar, teuer und netzabhängig).
- **Evidence**: `apps/haex-notes/package.json:6-18,42-48` und bestehende `app/lib/*.test.ts`.

## Resolved unknowns

- Die Darstellung des Originals ist eine gesperrte Bildebene.
- Recognition ist eine austauschbare lokale/externe Dienstgrenze, kein fest eingebauter Anbieter.
- OCR-Text bleibt als TextElement bearbeitbar; die Handschrift ist eine Font-Auswahl.
- Zeichnungen bleiben im ersten Release Rasterbilder.
- PDF-Export ist im ersten Release ein sichtbarkeitsgetreuer Rasterexport.
- Persönliche Handschriftprofile und individuelles lokales RAG sind nicht erforderlich, um den MVP zu implementieren.
