# Contract: Tauri-Befehle für Icon-Sets

Alle Befehle sind Befehle der App (Webview → Rust). Sie sind **nicht** über die Brücke für Erweiterungen und **nicht** als Aktion für Agenten erreichbar (FR-016); ein Test hält das fest. Fehler sind `HolziError` ohne Pfade der Quelle über den Namen hinaus. Typen werden mit ts-rs nach `src/types/bindings/` erzeugt.

## Typen

```text
IconSetRef      = { kind: 'builtin', name: 'lucide' | 'phosphor' | 'material' } | { kind: 'custom', id: string }
Scheme          = 'light' | 'dark'
IconQuery       = { name: string, isDir: boolean, noAccess: boolean, encrypted: EncryptedInfo | null }
IconAnswer      = { kind: 'image', id: string, mime: 'image/svg+xml' | 'image/png' | 'image/webp' }
                | { kind: 'builtin', name: string }
IconSetInfo     = { ref: IconSetRef, name: string, author: string, license: string, source: string,
                    imageCount: number, state: IconSetState, removable: boolean }
IconSetState    = 'ready' | 'syncing' | 'missing' | 'damaged' | 'unknown'
ImportReport    = { name: string, author: string, license: string, imageCount: number, bytes: number,
                    skipped: { path: string, reason: string }[], sameName: string | null }
IconSource      = { kind: 'file', file: PickedFile } | { kind: 'folder', path: string }
```

## Befehle

| Befehl                         | Parameter                                                   | Ergebnis                                    | Hinweise                                                                                                                                                                                                                                        |
| ------------------------------ | ----------------------------------------------------------- | ------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `files_icons_resolve`          | `set: IconSetRef`, `scheme: Scheme`, `queries: IconQuery[]` | `IconAnswer[]` (gleiche Reihenfolge)        | höchstens 500 Anfragen je Aufruf; ein unbekanntes, fehlendes oder beschädigtes Set antwortet wie Lucide (FR-020), nie mit einem Fehler. Reine Lesung, kein Schreiben                                                                            |
| `files_icon_image`             | `set: IconSetRef`, `id: string`, `scheme: Scheme`           | rohe Bytes (`tauri::ipc::Response`)         | SVG nur bereinigt (R9); unbekannte `id` ist ein Fehler `InvalidInput`; Muster `files_thumbnail`                                                                                                                                                 |
| `file_icon_sets_list`          | –                                                           | `IconSetInfo[]`                             | mitgelieferte zuerst, dann eigene nach Name; nennt den Zustand jedes Sets und den des gewählten (`files.iconSet`)                                                                                                                               |
| `file_icon_set_import_preview` | `source: IconSource`                                        | `ImportReport`                              | schreibt nichts; ein Ordner nur auf Desktop (Android: `InvalidInput`, `folder_pick` ist dort `false`); lehnt ab, wenn nichts Brauchbares da ist                                                                                                 |
| `file_icon_set_import`         | `source: IconSource`, `replace: string \| null`             | `IconSetInfo`                               | liest erneut, schreibt in einer Transaktion eine Zeile; `replace` = UUID des Sets, das ersetzt wird (danach gelöscht); lehnt über 10 Sets und alle Grenzen ab                                                                                   |
| `file_icon_set_remove`         | `id: string`                                                | –                                           | löscht die Zeile; ist das Set gewählt, schreibt der Aufruf `files.iconSet` auf `builtin:lucide` (der Sync bringt beides)                                                                                                                        |
| `file_icon_set_licenses`       | –                                                           | `{ name, author, license, source, text }[]` | Daten der mitgelieferten Pakete (`origin`, `LICENSE`) für die Seite „Lizenzen“; Lucide enthält seine Texte (ISC und MIT) als feste Zeichenkette, Phosphor und Material ihre (MIT, bei Material zusätzlich Apache-2.0 und Hinweis) aus dem Paket |

Die Auswahl selbst läuft über die vorhandenen `get_pref` und `set_pref` (Scope Vault, Schlüssel `files.iconSet`); `validate_value` prüft die Form.

## Fehler und Randfälle

- `file_icon_set_import_preview` und `file_icon_set_import` melden dieselbe Prüfung mit denselben Gründen; der Grund steht je ausgelassener Datei im Bericht (`skipped`), ein nicht lesbares Paket als Fehler mit einem Satz („kein `icon-theme.json`“, „über dem Limit von 5 MiB“, „Archiv mit Pfad außerhalb“).
- Ist das Set zwischen Vorschau und Import gelöscht oder geändert worden, gilt die zweite Prüfung.
- `files_icons_resolve` verändert nichts und nimmt keine Dateipfade an; der Name eines Eintrags wird nur als Schlüssel gebraucht.

## Ereignis

`vault-data-changed` (vorhanden) nennt künftig `haex_file_icon_sets`; das Frontend lädt bei `preferences` und `haex_file_icon_sets` die Liste und leert den Speicher der Antworten.
