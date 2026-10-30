# Quickstart: Paper Notes to Editable Canvas

Handtest in `https://github.com/haex-space/haextension.git` ab Revision
`6dfb015198c99018236f5a8e958a916942ab971c`, unter `apps/haex-notes/`, gegen holzi mit Spec 055.

## Voraussetzungen

- holzi mit Spec 055 und ein Modell, das Bilder annimmt. Lokal: Ollama mit `qwen3-vl:4b-instruct` als Modell-Server
  `http://localhost:11434/v1`, am Modell „nimmt Bilder an“ gesetzt (Einrichtung:
  [055 quickstart.md](../055-extension-model-access/quickstart.md#lokales-bildmodell-einrichten-ollama)). Für den
  Schritt „Ziel außerhalb des Geräts“ zusätzlich ein Server im eigenen Netz oder in der Cloud.
- Einverständlich aufgenommene oder de-identifizierte Fotos mit Handschrift, einer Tabelle, einer Zeichnung, einem
  Rechtschreibfehler und einem seitlich aufgenommenen Blatt (zum Beispiel die Testbilder des Modellvergleichs).

## Automatische Prüfungen

Aus `apps/haex-notes/`:

```sh
pnpm install
pnpm test
pnpm build
```

Erwartet:

- Gewöhnliche Seiten und Tabellen ohne `cells` laden unverändert; importierte Seiten überstehen Speichern und Laden.
- Der gebaute Auftrag entspricht [contracts/page-job.md](./contracts/page-job.md).
- Die Abbildung erzeugt Text-, Tabellen- und Bildelemente, übergeht leere Regionen und ungültige Boxen und legt
  Kanten und Überlappungen nach research R11 zurecht.
- Ein Drehhinweis führt genau einmal zu einem neuen Auftrag mit gedrehter Arbeitskopie.
- Jeder Fehlercode ergibt eine Seite mit dem Foto (Ebene „Foto“, zusätzlich als Anhang) und den Hinweis aus dem Vertrag.
- Text, Bilder und Zelltexte werden gezeichnet, auch im Export; fehlende Assets zeigen einen Platzhalter.

## Handtest

1. Notizbuch öffnen, „Foto importieren“ wählen, ein Foto aufnehmen oder auswählen. Die Seite zeigt nur den Entwurf;
   das Original hängt als Anhang „Original“ an der Seite.
2. Ein schlechtes Foto zeigt den Qualitätshinweis mit „Neu aufnehmen“ und „Trotzdem lesen“.
3. Lesen starten. holzi fragt beim ersten Mal nach dem Modell; „Merken“ und erlauben. Der Entwurf zeigt Texte,
   die Tabelle mit Zeilen und Zellen und die Zeichnung als Bild; der Rechtschreibfehler ist korrigiert, Abkürzungen
   und Zeilenumbrüche sind wie auf dem Foto. Die Herkunft nennt Modell und „dieses Gerät“.
4. Ein seitliches Foto lesen: Der Entwurf steht aufrecht, das Original ist unverändert gespeichert.
5. Eine Zelle der Tabelle ändern, eine Zeile einfügen, die Zeichnung verschieben und vergrößern, einen Text ändern und
   den Handschriftstil wechseln.
6. Den Anhang „Original“ öffnen: Er erscheint neben der Seite, mit Zoom, und bleibt Byte für Byte gleich; die Seite bleibt
   bearbeitbar. Im Vollbild öffnen: Es ist nur der Anhang zu sehen, weder die Seite noch das Original. Sichern ergibt die unveränderte Datei, Entfernen fragt zuerst nach; danach ist die Seite eine gewöhnliche Seite, Rückgängig stellt den Anhang her. Eine zweite Datei anhängen,
   speichern, neu öffnen, wieder entfernen. Die Seitenliste hat keinen zweiten Eintrag.
   6a. Ein Rechteck über eine schlecht gelesene Stelle ziehen und ein Nahfoto davon lesen lassen: Die erzeugten Elemente
   im Rechteck sind ersetzt, eigene Striche und der Anhang „Original“ sind unverändert; Rückgängig stellt den alten Stand her.
   Abbrechen während des Lesens ändert nichts.
   6b. Vorlage und Papierfarbe der Seite wechseln (kariert, Creme) und den Seitenrand ändern: Rückgängig stellt den alten
   Stand her, nach Speichern und Öffnen steht er noch da.
   6c. Eine Seite mit Reaktionsgleichungen lesen (eine Zeile und eine Tabellenzelle mit nur einer Formel): Sie stehen als
   gesetzte Formeln da, eine Formel im Satz als Text mit Indizes. Doppelklick auf eine Formel: einfache Schreibweise
   (Chemie, Mathe & Physik, dazu LaTeX) mit Vorschau und Knöpfen; eine kaputte Formel wird nicht übernommen, Rückgängig stellt die alte her. „Formel einfügen“ legt eine neue an. Im
   PDF stehen die Formeln wie auf der Seite.
   6d. Kariertes Papier wählen, einen Text auswählen und in der Leiste darüber die Zeilenhöhe 1×, dann 2× und eine Farbe
   wählen: Die Grundlinie sitzt auf einer Linie, die Schrift ist entsprechend größer. „Schrift an Papier anpassen“ im
   Hintergrund-Menü stellt alle Texte der Seite auf die Zeilen, eine Überschrift bleibt groß; Rückgängig stellt den alten
   Stand her. Auf blankem Papier gibt es die Zeilenhöhe nicht.
7. Speichern, schließen, neu öffnen: Ebenen, Zellen, Positionen und Herkunft sind erhalten.
8. Als PDF exportieren, mit den Voreinstellungen und mit geändertem Format, Ausrichtung und Qualität: Die PDF zeigt
   die Seite mit Papier und Rand, keinen Anhang.
9. Ziel außerhalb des Geräts: In holzi den Server im Netz an die erste Stelle der Rangfolge setzen und lesen. holzi
   fragt vor dem Versand mit Ziel und Ort; „ablehnen“ ergibt eine Seite mit Foto (Ebene „Foto“, zusätzlich als Anhang)
   und den Hinweis zur Ablehnung.
10. In holzi die Freigabe widerrufen und lesen: Der Hinweis nennt das fehlende Modell; der manuelle Canvas bleibt
    nutzbar. Ebenso beim Abbrechen während des Lesens.

## Nachweise

Je Foto festhalten: Zeit bis zum ersten Entwurf oder Hinweis, Anzahl der erwarteten und gefundenen Text-, Tabellen-
und Bildregionen, ob Korrektur, Zellbearbeitung, Verschieben, Speichern und Neuöffnen und jede PDF-Einstellung
gelangen, ob das Original unverändert blieb und ob jeder Fehlerfall einen nutzbaren Canvas hinterließ. Daraus folgen
SC-001 bis SC-006; SC-007 ist eine spätere Pilotbewertung. Die Fixtures prüfen Vertrag und Canvas, nicht die Güte
eines Modells für deutsche Handschrift.
