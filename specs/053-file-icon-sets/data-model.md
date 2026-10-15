# Data Model: Austauschbare Icon-Sets für den Dateibrowser

Eine neue synchronisierte Tabelle, eine neue Preference. Keine Änderung an vorhandenen Tabellen.

## Tabelle `haex_file_icon_sets` (Migration `0036_file_icon_sets`)

Ein eigenes Set ist eine Zeile. Mitgelieferte Sets stehen nicht in der Tabelle; sie liegen in der App ([research.md](./research.md) R6).

| Spalte        | Typ                   | Bedeutung                                                                     |
| ------------- | --------------------- | ----------------------------------------------------------------------------- |
| `id`          | TEXT, Primärschlüssel | UUID, auf dem Gerät erzeugt, das importiert                                   |
| `name`        | TEXT, nicht leer      | Anzeigename; beim Import aus `package.json`, dem Manifest oder dem Dateinamen |
| `author`      | TEXT, leer erlaubt    | Urheber, soweit das Set ihn nennt                                             |
| `license`     | TEXT, leer erlaubt    | SPDX-Kennung oder Text, soweit das Set sie nennt                              |
| `source`      | TEXT, leer erlaubt    | Quelle (Adresse), soweit das Set sie nennt                                    |
| `image_count` | INTEGER               | Zahl der Bilder im Paket                                                      |
| `byte_size`   | INTEGER               | Größe des Pakets in Bytes (gepackt)                                           |
| `sha256`      | TEXT                  | SHA-256 des Pakets (Hex); der Lader prüft es beim ersten Öffnen               |
| `pack`        | BLOB                  | das Paket: ZIP mit `icon-theme.json`, den bereinigten Bildern und `LICENSE`   |
| `created_at`  | TEXT                  | Zeitpunkt des Imports (ISO 8601)                                              |

Regeln für synchronisierte Tabellen (wie `migrations_storage.rs` und `migrations_contacts.rs`):

- Primärschlüssel ist Pflicht; **kein UNIQUE** (ein Konflikt beim Anwenden würde den Sync anhalten). Dass ein Name nur einmal vorkommt, prüft der Import im Code (mit Rückfrage zum Ersetzen, US3 Szenario 6); zwei Geräte, die gleichzeitig denselben Namen importieren, ergeben zwei Zeilen gleichen Namens, die die Einstellungen beide zeigen und entfernbar sind.
- Ein Paket ist **unveränderlich**: Ersetzen heißt neue Zeile, dann die alte löschen. Damit gibt es kein feldweises Zusammenführen zweier Fassungen, und eine Zeile kommt mit einer Transaktionsgruppe vollständig oder gar nicht an (FR-015).
- Löschen ist ein einzelnes `DELETE`; haex-crdt schreibt den Löschmarker (`haex_deleted_rows`). Es gibt keine Kind-Zeilen.
- Die Migration trägt die Tabelle in `SYNCED_TABLES` ein (`migrations_icon_sets.rs`), und `HOLZI_TRIGGER_VERSION` wird erhöht (heute 22), mit Eintrag in der Versionsliste von `migrations.rs`. Der Test der Migration (`migrations_icon_sets_tests.rs`) prüft, dass `SYNCED_TABLES` alle Tabellen der Migration abdeckt, wie bei den anderen Familien.
- Die Nummer `0036` folgt `0035_chat_messages_tool_origin`, das während der Umsetzung auf `main` gelandet ist.

Grenzen werden im Code geprüft (Import und Lader), nicht per `CHECK` in der Tabelle: höchstens 10 Zeilen, `byte_size` höchstens 8 MiB, entpackt höchstens 5 MiB.

## Preference `files.iconSet`

| Eigenschaft | Wert                                                                                                                                                 |
| ----------- | ---------------------------------------------------------------------------------------------------------------------------------------------------- |
| Scope       | Vault (nicht Gerät): geht über den Sync auf alle Geräte                                                                                              |
| Schlüssel   | `files.iconSet`                                                                                                                                      |
| Werte       | `builtin:lucide`, `builtin:phosphor`, `builtin:material`, `custom:<uuid>` (die UUID einer Zeile)                                                     |
| Standard    | fehlt der Schlüssel: `builtin:lucide`                                                                                                                |
| Prüfung     | Form in `validate_value` (`storage/preferences_commands.rs`); ein unbekannter oder fehlender Wert ist beim Lesen kein Fehler, er ergibt den Rückfall |

Zustand des gewählten Sets (`IconSetState`, nur zur Anzeige, nicht gespeichert):

| Zustand   | Bedingung                                                  | Wirkung                                                       |
| --------- | ---------------------------------------------------------- | ------------------------------------------------------------- |
| `ready`   | mitgeliefert, oder Zeile da und Paket besteht die Prüfung  | das Set gilt                                                  |
| `syncing` | `custom:<uuid>` gewählt, Zeile fehlt, kein Löschmarker     | Lucide gilt; Einstellungen: „wird synchronisiert“             |
| `missing` | `custom:<uuid>` gewählt, Zeile fehlt, Löschmarker da       | Lucide gilt; Einstellungen: „Set wurde entfernt“              |
| `damaged` | Zeile da, Prüfung (Aufbau, Limits, SHA-256) schlägt fehl   | Lucide gilt; Einstellungen: „Set beschädigt“, „Entfernen“     |
| `unknown` | `builtin:<name>` unbekannt (Set einer neueren App-Version) | Lucide gilt; Einstellungen: „braucht eine neuere App-Version“ |

## Paket (ZIP, nicht Tabelle)

Aufbau, so wie der Lader ihn liest ([research.md](./research.md) R6, R7):

| Eintrag           | Inhalt                                                                                         |
| ----------------- | ---------------------------------------------------------------------------------------------- |
| `icon-theme.json` | Manifest im Format der Icon-Themes von VS Code; Pfade in `iconPath` zeigen auf Bilder im Paket |
| Bilder            | SVG (bereinigt), PNG oder WebP, flach oder in Unterordnern                                     |
| `LICENSE`         | Lizenztext des Sets (bei mitgelieferten Sets Pflicht; bei eigenen, soweit vorhanden)           |

Ergänzung des Manifests (ignoriert jedes andere Programm, das das Format liest):

```json
"x-holzi": {
  "roles": { "image": "<Bild-Kennung>", "lockedFolder": "<Bild-Kennung>" },
  "origin": { "author": "…", "license": "MIT", "source": "…" }
}
```

Rollen: `folder`, `folderNoAccess`, `file`, `image`, `video`, `audio`, `pdf`, `text`, `encryptedFolder`, `lockedFolder`, `lockedFile`, `damaged`. Jede ist optional.

Prüfregeln (Import und Lader, eine Funktion): jeder Pfad besteht `haex_bundle::format::path_rule_violation` und `PathChecker`; keine Verzeichniseinträge, keine Symlinks; höchstens 2000 Bilder, 256 KiB je Bild, 5 MiB entpackt, Verhältnis gepackt/entpackt höchstens 200; Pixelbilder höchstens 512 × 512 px; jede `iconPath` zeigt auf einen Eintrag im Paket; SVG besteht die Bereinigung ([research.md](./research.md) R9).

## Antworten der Befehle (Laufzeit, nicht gespeichert)

- **`IconAnswer`**: entweder `image { id, mime }` (`id` ist die Bild-Kennung im Set) oder `builtin { name }` (Symbol des Standard-Sets, zum Beispiel `lucide:folder-lock`).
- **`IconQuery`**: `{ name, isDir, noAccess, encrypted? }`, wobei `encrypted` `{ role, state }` aus `Entry.encrypted` ist. Mehr Wissen über den Eintrag braucht die Auflösung nicht.
- Rückfall-Tabelle Rolle → Symbol (Standard-Set, in Rust):

| Rolle             | Symbol                |
| ----------------- | --------------------- |
| `folder`          | `lucide:folder`       |
| `folderNoAccess`  | `lucide:folder-lock`  |
| `file`            | `lucide:file`         |
| `image`           | `lucide:image`        |
| `video`           | `lucide:film`         |
| `audio`           | `lucide:music`        |
| `pdf`, `text`     | `lucide:file-text`    |
| `encryptedFolder` | `lucide:folder-key`   |
| `lockedFolder`    | `lucide:folder-lock`  |
| `lockedFile`      | `lucide:file-lock`    |
| `damaged`         | `lucide:file-warning` |

(entspricht dem heutigen `EntryIcon.vue` und `encrypted.ts`; die Tabelle ersetzt sie.)

## Zustandswechsel eines eigenen Sets

```text
(Datei oder Ordner) --Vorschau--> Bericht --Import--> Zeile in haex_file_icon_sets
Zeile --Sync--> andere Geräte (Prüfung beim ersten Öffnen: ready | damaged)
Zeile --Entfernen--> Löschmarker --Sync--> überall weg; Wahl auf diesem Set fällt auf Lucide
Ersetzen: neue Zeile, dann alte löschen (Wahl auf der alten Zeile zeigt auf die neue UUID, wenn der Nutzer das beim Ersetzen bestätigt)
```
