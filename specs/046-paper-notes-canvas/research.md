# Research: Paper Notes to Editable Canvas

**Stand**: 2026-10-21 (überarbeitet auf Spec 055; erste Fassung 2026-10-08)

**Zielrepository**: `https://github.com/haex-space/haextension.git`

**Unveränderliche Revision**: `6dfb015198c99018236f5a8e958a916942ab971c` (`apps/haex-notes/`, Paket 0.1.7)

## R1 — Implementierungsgrenze und vorhandene Abstraktionen

- **Decision**: Spezifikation und Planung liegen in holzi; die Umsetzung gehört in `haextension/apps/haex-notes/`
  bei der oben genannten Revision. Vorhandene Dokument-, Command-, Asset- und Renderer-Abstraktionen werden
  erweitert.
- **Rationale**: haex-notes lebt in einem eigenen Repository. Ein paralleles Modell in holzi würde die Erweiterung
  nicht verändern.
- **Befund an der Revision**:
  - `PageDoc` mit `PageLayer[]` und `PageElement[]` (`app/types/document.ts`) kennt `text`, `image`, `stroke`,
    `shape`, `table` und `latex`.
  - Der Renderer (`app/lib/render/elements.ts`, `drawElement`) zeichnet nur `stroke` und `table`. `text` und `image`
    stehen im Modell, werden aber nicht gezeichnet.
  - `TableElement` ist ein Raster (Spalten, Zeilen, Breiten, Höhen) ohne Zelltext; `app/lib/tableActions.ts` legt es an.
  - Die Seitentabelle (`app/database/schemas/index.ts`) speichert `layers` und `background` als JSON.
  - Der Asset-Store ist `app/composables/useAssetStore.ts`, die Freigabe von Seiten sammelt Bilder in
    `app/lib/collectAssets.ts`.
- **Alternatives considered**: Umsetzung in holzi (verworfen: keine haex-notes-Quelle); eigenes Importmodell neben
  `PageDoc` (verworfen: doppelte Persistenz, Undo und Teilen).

## R2 — Originalfoto als gesperrte Ebene statt Hintergrund-Overlay

- **Decision**: Das Originalfoto wird als Asset gespeichert und als `ImageElement` in einer eigenen sichtbaren,
  gesperrten Ebene angelegt. `PageBackground` bleibt für Papierfarbe und Lineatur zuständig.
- **Rationale**: `PageLayer` hat `visible` und `locked`; `ImageElement` hat Position, Größe, Drehung, Quelle und
  Deckkraft. So lässt sich das Original ein- und ausblenden und bleibt unabhängig vom Entwurf.
- **Alternatives considered**: Original als `PageBackground.overlay` (verworfen: keine Ebenensemantik); Original im
  Erkennungs-JSON (verworfen: große Binärdaten).

## R3 — Herkunft in einem optionalen Seitenfeld

- **Decision**: Die Seite erhält ein optionales Feld `paperImport` (neue JSON-Spalte, Migration `0005`). Es verweist
  auf das Original-Asset, hält den Verarbeitungsstand und die Herkunft (Modellname, Ort, Zeitpunkt aus der
  `provenance` des Auftrags) und je Region deren Quellbox und die erzeugten Element-IDs.
- **Rationale**: Geometrie und Inhalt bleiben im `PageElement`-Modell. Die Herkunft gehört zur Seite, damit sie
  nach Speichern, Öffnen und Teilen erhalten bleibt. Prüfzustände und Vorschläge gibt es nicht mehr (kein Prüfteil).
- **Alternatives considered**: normalisierte Tabellen (verworfen: Migrations- und Syncaufwand); nur UI-Zustand
  (verworfen: geht beim Neuladen verloren).

## R4 — Seitenerkennung als Modellauftrag (Spec 055)

- **Decision**: haex-notes ruft `sdk.models.runAsync(job)` (vault-sdk ≥ 4.7.1) mit zwei Schritten auf:
  1. `read`: Bild der Arbeitskopie, `needs: ["text", "image"]`, Format `page-recognition` v1, `effort: "medium"`.
  2. `fix`: `needs: ["text"]`, Eingabe `from: { step: "read", pointer: "/regions" }`, Format `text-fix` v1,
     `effort: "low"`.

  Anweisungen und Formate stehen in [contracts/page-job.md](./contracts/page-job.md). Sie entsprechen der
  Test-Erweiterung von Spec 055 (`src-tauri/tests/fixtures/extension_e2e/model-jobs/`).

- **Rationale**: holzi wählt das freigegebene Modell und den Ort, fragt vor dem ersten Versand außerhalb des Geräts,
  deckelt den Aufwand und verwirft kaputte Ausgabe. haex-notes braucht dafür weder Anbieter noch Schlüssel. Zwei
  Schritte erlauben ein Bildmodell zum Lesen und ein Textmodell zum Korrigieren.
- **Ergebnis**: Ein Auftrag endet `completed` (beide Ergebnisse) oder `failed` oder `cancelled` mit einem Fehlercode.
  Ein abgelehnter Start (kein passendes Modell, Einwilligung verweigert) wird abgelehnt (`1002`, `8001`).
  haex-notes übersetzt jeden Fehlerfall in einen verständlichen Hinweis und behält das Original (FR-016).
- **Alternatives considered**: der Agent-Task von Spec 047 (abgelöst); direkter HTTP-Aufruf aus haex-notes
  (verworfen: doppelte Anbieter- und Schlüssellogik); ein Schritt ohne Korrektur (verworfen: die Korrektur ist
  Anforderung FR-006).

## R5 — Text bleibt bearbeitbar, Handschrift ist Darstellung

- **Decision**: Der korrigierte Text steht in `TextElement.text`; `fontFamily` verweist auf einen registrierten
  Handschriftstil. Der gelesene Text vor der Korrektur wird nicht gespeichert (kein Zurücksetzen).
- **Rationale**: Der Schüler verbessert Wörter direkt. Ein Font ist ein kontrollierbares Ergebnis; persönliche
  Handschrift bleibt außerhalb des Umfangs.
- **Alternatives considered**: Text als Bild (verworfen: nicht bearbeitbar); gelesenen Text zum Zurücksetzen
  aufheben (verworfen: kein Zurücksetzen gewünscht, Entscheidung 2026-10-16).

## R6 — Gemeinsame Rendering-Grundlage für Canvas und PDF

- **Decision**: Zuerst werden `text` und `image` in `drawElement` gezeichnet und Tabellenzellen beschriftet. Der
  PDF-Export rastert den sichtbaren Seitenzustand hochauflösend und bettet ihn mit `pdf-lib` in eine PDF ein.
- **Rationale**: Canvas und PDF zeigen dieselben Fonts, Positionen und Ausschnitte. Echter Text-Export kann später
  folgen.
- **Alternatives considered**: Druckdialog des Browsers (verworfen: nicht testbar); Vektor-PDF sofort (verworfen:
  abweichende Font-Metriken, größerer Umfang).

## R7 — Asset- und Fehlerstrategie

- **Decision**: Original, Arbeitskopie und Bildausschnitte liegen als gehashte Assets im Asset-Store. Erkennung,
  Zuschnitt und Rendern laufen asynchron; der Stand steht in `paperImport.processing`. Bei jedem Fehler bleibt die
  Originalebene nutzbar.
- **Rationale**: Der Asset-Store dedupliziert Bytes und hält sie aus der Datenbank. `collectAssetIds()` wird um die
  neuen Quellen erweitert, damit eine geteilte Seite ihre Bilder mitnimmt.
- **Alternatives considered**: Data-URLs im Seiten-JSON (verworfen: große Transaktionen).

## R8 — Teststrategie

- **Decision**: Reine Logik (Auftrag bauen, Ergebnis abbilden, Tabellen, Drehung, Layout, Serialisierung, Render)
  wird mit Vitest getestet. Die SDK-Schicht wird über eine kleine Schnittstelle `runPageJob(job)` angesprochen, die im
  Test durch eine Attrappe mit festen Ergebnissen ersetzt wird. Ein Handtest gegen holzi mit Ollama folgt
  [quickstart.md](./quickstart.md).
- **Rationale**: Das Paket nutzt bereits Vitest für Modell, Commands, Assets und Renderer. Echte Modelle sind nicht
  reproduzierbar; holzis Prüfung der Ausgabe ist dort getestet (Spec 055).
- **Alternatives considered**: nur Handtests (verworfen); echte Modelle in jedem Testlauf (verworfen).

## R9 — Tabellen mit Zelltext

- **Decision**: `TableElement` bekommt `cells: string[][]` (Zeilen × Spalten, leere Zelle `""`). Fehlt das Feld in
  einer alten Seite, gilt jede Zelle als leer. Der Renderer schreibt den Text in die Zellen; ein Doppelklick auf eine
  Zelle öffnet einen Texteditor wie bei `TextElement`. Zeilen und Spalten hinzufügen oder entfernen hält `cells` passend.
- **Rationale**: Tabellen sollen als Zeilen und Zellen bearbeitbar sein, nicht als Bild (FR-011). Das vorhandene
  Raster mit Breiten und Höhen bleibt die Geometrie.
- **Abbildung**: Die `rows` einer Tabellenregion werden auf die längste Zeile aufgefüllt. Die Spaltenbreiten teilen die
  Breite der Quellbox gleich auf, die Zeilenhöhen ihre Höhe.
- **Alternatives considered**: Zellen als einzelne `TextElement`s über dem Raster (verworfen: verschieben und
  Zeilen einfügen zerreißen die Tabelle); Tabelle als Bild (verworfen: FR-011).

## R10 — Gedrehte Fotos

- **Decision**: Das Format hat ein optionales Feld `rotation` (0, 90, 180, 270: im Uhrzeigersinn, um aufrecht zu
  stehen). Meldet das Modell einen Winkel ungleich 0, dreht haex-notes die Arbeitskopie und startet den Auftrag
  einmal neu. Eine zweite Meldung wird nicht befolgt. Im Importdialog kann der Schüler das Foto vor dem Lesen in
  90°-Schritten drehen. Das Original bleibt, wie es aufgenommen wurde; die Originalebene zeigt es aufrecht gedreht
  (`ImageElement.rotation`).
- **Rationale**: Messenger entfernen die EXIF-Drehung. holzi dreht nichts (Spec 055 T066), also muss die
  Erweiterung es tun. Nur einmal nachzulesen verhindert eine Schleife.
- **Befund 2026-10-23**: `qwen3-vl:4b-instruct` ist als Quelle des Hinweises unbrauchbar. Allein danach gefragt,
  antwortet es auf jedes Foto fast immer „90“ und nennt jede Seite „horizontal“. Im ganzen Auftrag meldete es für die
  aufrechte und für die um 90° gedrehte Seite 0. Den gedrehten Text liest es trotzdem richtig; seine Boxen folgen dem
  gedrehten Foto, der Entwurf steht dann seitlich. Das Drehen im Dialog (mit Vorschau) ist deshalb der verlässliche
  Weg; der Hinweis des Modells ist ein Zusatz für größere Modelle.
- **Alternatives considered**: EXIF auswerten (verworfen: meist entfernt); jede Seite viermal lesen (verworfen:
  Dauer).

## R11 — Neu zusammengesetzte Seite

- **Decision**: Die Quellboxen (0–1000 relativ zur Arbeitskopie, `[x1, y1, x2, y2]`) werden auf Seitenkoordinaten
  skaliert. Text, Tabellen und Bildausschnitte landen dort, wo sie auf dem Foto standen, mit zwei Aufräumregeln:
  - Linke Kanten, die weniger als 2 % der Seitenbreite auseinanderliegen, werden auf eine Kante gelegt.
  - Überlappende Regionen werden in Lesereihenfolge nach unten geschoben.

  Die Schriftgröße eines Textblocks ergibt sich aus Boxhöhe und Zeilenzahl, begrenzt auf einen lesbaren Bereich.

- **Rationale**: Ein ordentliches Tafelbild, das der Schüler danach frei umstellt. Die Lage auf dem Foto ist die beste
  verfügbare Anordnung; vollautomatisches Layout ist nicht verlangt.
- **Alternatives considered**: alles untereinander in Lesereihenfolge (verworfen: Tabellen und Skizzen neben dem
  Text gingen verloren); Boxen unverändert übernehmen (verworfen: schiefe Kanten und Überlappungen aus dem Foto).

## R12 — Korrektur ohne Zurücksetzen

- **Decision**: haex-notes ordnet jeden Text des Schritts `fix` dem ähnlichsten gelesenen Text einer Region mit `text`
  zu (Dice-Ähnlichkeit über Zeichenpaare, mindestens 0,5, das ähnlichste Paar zuerst). Überzählige Texte, etwa
  Tabellenzellen, bleiben unbeachtet; ein gelesener Text ohne Zuordnung bleibt, wie er gelesen wurde; ein
  umgeschriebener Text (Ähnlichkeit unter 0,5) wird nicht übernommen. Antwortet der Schritt nicht im Format
  `text-fix`, bleiben alle gelesenen Texte. Es gibt keinen Fehler.
- **Rationale**: Die Korrektur ist eine Verbesserung, kein Pflichtteil, und sie darf nur Rechtschreibung und
  eindeutige Lesefehler ändern (SC-005). Ein Lauf mit `qwen3-vl:4b-instruct` am 2026-10-23 zeigte, dass ein kleines
  Modell nie genau einen Text je Region liefert: Es schrieb jede Tabellenzelle als eigenen Text mit (3 Texte für 2
  Regionen, 13 für 4), in 12 von 12 Läufen, auch mit einer ausdrücklicheren Anweisung. Eine Zuordnung nach der Reihe
  oder nach der Anzahl hätte nie eine Korrektur übernommen.
- **Alternatives considered**: nach der Reihe, nur bei gleicher Anzahl (verworfen: im Lauf nie erfüllt); die
  Seite verwerfen (verworfen: der gelesene Text ist schon brauchbar); den Auftrag auf getrennte Listen je Art
  umstellen (verworfen: ändert das Format, die Zuordnung genügt).

## R13 — Eine Stelle über ein Nahfoto neu lesen

- **Decision**: Der Schüler zieht auf einer Fotoseite ein Rechteck über eine schlecht oder nicht erkannte Stelle und
  macht davon ein Nahfoto. Es läuft durch denselben Auftrag „Seitenerkennung“ (contracts/page-job.md); die Regionen
  werden mit den Regeln aus R11 in das Rechteck statt in die Seite eingepasst. Ersetzt werden die erzeugten Elemente
  (aus `paperImport.regions`), die ganz im Rechteck liegen und auf einer nicht gesperrten Ebene stehen. Elemente,
  Regionen und `paperImport.closeUps` ändern sich in einem Kommando. Das Nahfoto wird erst nach Erfolg gespeichert.
- **Rationale**: Der Schüler bestimmt selbst, was ersetzt wird; eine Zuordnung alter und neuer Regionen entfällt, und
  eine Erkennung von Änderungen des Schülers auch: ein Fehlgriff lässt sich zurücknehmen. Ein Nahfoto hat mehr Pixel
  pro Zeichen als das Seitenfoto und wird deshalb besser gelesen.
- **Alternatives considered**: das ganze Blatt erneut fotografieren und alte und neue Regionen zuordnen (verworfen:
  Lage und Zuschnitt eines neuen Fotos stimmen nicht mit dem alten überein); dasselbe Original erneut lesen und nur
  unveränderte Elemente über einen Fingerabdruck ersetzen (verworfen: hilft nicht bei einem schlechten Foto, und ein
  anderes Modell wählt man in holzi); immer vor dem Ersetzen fragen (verworfen: Undo genügt).

## Resolved unknowns

- Erkennung: Modellauftrag über holzi in zwei Schritten (R4), kein Agent-Task, kein Profil.
- Tabellen: eigenes Zellfeld im `TableElement` (R9).
- Gedrehte Fotos: Drehhinweis des Modells oder Drehen durch den Schüler, einmal nachlesen (R10).
- Anordnung: Lage auf dem Foto mit Kanten- und Überlappungsregeln (R11).
- Neu lesen: ein Nahfoto einer Stelle ersetzt die erzeugten Elemente im gewählten Rechteck (R13).
- Das Original ist eine gesperrte Bildebene; der Text ist ein `TextElement` mit Handschrift-Font; Zeichnungen sind
  Rasterausschnitte; der PDF-Export rastert die sichtbare Seite.
