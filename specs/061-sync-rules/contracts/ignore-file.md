# Vertrag: Ausnahmedatei `.haex-ignore`

## Ort und Name

- Genau eine Datei `.haex-ignore` im Wurzelordner eines Orts. Dateien gleichen Namens in Unterordnern werden nicht
  ausgewertet (FR-038); sie sind gewöhnliche Dateien und werden abgeglichen.
- Zeichensatz UTF-8, Zeilenende `\n` oder `\r\n`, höchstens 64 KiB und 2 000 Zeilen. Der Editor lehnt größere
  Schreibvorgänge ab. Ist eine vorhandene Datei größer, verwendet holzi sie nicht verkürzt, sondern bricht den
  Lauf geschlossen mit einem Fehler im Regelstatus ab, bis die Datei korrigiert wurde.

## Schreibweise

Wie `.gitignore` (Crate `ignore`, `GitignoreBuilder::add_line`):

- Leere Zeilen und Zeilen mit `#` am Anfang sind Kommentare.
- `*`, `?`, `[a-z]` und `**` gelten wie in Git.
- Ein `/` am Ende trifft nur Ordner, ein `/` am Anfang oder in der Mitte verankert das Muster an der Wurzel.
- `!` am Anfang hebt ein früheres Muster derselben Datei auf. Ein Muster kann keinen Ordner wieder aufnehmen,
  dessen Elternordner ausgenommen ist (wie in Git).
- Pfade sind relativ zum Wurzelordner des Orts, mit `/` als Trenner; Groß- und Kleinschreibung zählt.

Eine Zeile, die der Parser ablehnt, wird übersprungen und im Regelstatus mit Zeilennummer genannt; die übrigen
Zeilen gelten (FR-038).

## Auswertung

1. `ausgenommen(pfad) = fest(pfad) ∨ matcher_A(pfad) ∨ matcher_B(pfad)`; jede Seite wertet nur die Datei ihres
   Orts aus (FR-039).
2. Ein Pfad ist ausgenommen, wenn er selbst oder ein Elternordner trifft.
3. `.haex-ignore` selbst ist nie ausgenommen.
4. Feste Ausnahmen (immer, nicht änderbar): `.holzi-trash/`, die temporären Kopierdateien der Regel
   (`.*.holzi-part-*`, `.*.holzi-old-*`), `.DS_Store`, `Thumbs.db`, `desktop.ini`.
5. Eine Datei, die ausgenommen ist, erscheint nicht im Plan, nicht im Stand und nicht in den Zählern (nur in der
   Zahl „ausgenommen“ der Vorschau und des Editors, FR-042).
6. Wird eine Ausnahme aufgehoben, gilt die Datei im nächsten Lauf als neu (FR-040).
7. Bei einem Konflikt der beiden `.haex-ignore`-Dateien bleiben beide ursprünglichen Inhalte als Konfliktkopien
   erhalten; als aktive Datei wird auf beiden Seiten die zusammengeführte Vereinigung der Muster geschrieben. Diese
   aktive Vereinigung bleibt bei folgenden Läufen wirksam, damit kein Muster aus einer Konfliktseite verloren geht.

## Lesen und Schreiben durch die Oberfläche

- Der Editor liest die Datei im Quellort, auch bei beidseitig; fehlt sie, ist der Inhalt leer.
- Speichern schreibt die Datei über `Place::write_text` in dieselbe Quelle (temporäre Datei, dann Umbenennen).
- Ist der Ort nicht beschreibbar (Speicher ohne Recht, schreibgeschützt), liefert `sync_rules_ignore_get`
  `writable: false` mit Grund; `sync_rules_ignore_set` lehnt mit `readOnly` ab.
