# Data Model: Dateianhänge im Passwortmanager anzeigen und öffnen

**Feature**: [spec.md](./spec.md) | **Research**: [research.md](./research.md)

Es gibt **keine Schemaänderung und keine Migration**. Die Spec liest vorhandene Daten und legt eine
Geräte-Einstellung und kurzlebige Dateien an.

## Anhang (vorhanden, Spec 034)

| Quelle                         | Feld          | Verwendung hier                                               |
| ------------------------------ | ------------- | ------------------------------------------------------------- |
| `haex_passwords_item_binaries` | `id`          | Schlüssel aller neuen Commands (`attachmentId`)               |
|                                | `file_name`   | Art (Bild, Text, PDF, andere) über die Endung; Name der Kopie |
|                                | `binary_hash` | Verbindung zur Binärzeile                                     |
| `haex_passwords_binaries`      | `data`        | Bytes für Text, PDF und Kopie (höchstens 25 MiB, FR-020 034)  |

Gelesen wird ausschließlich über `binaries::attachment_data` (Name und Bytes). Fehlt die Binärzeile
(noch nicht synchronisiert) oder der Anhang, ist das Ergebnis `PasswordsNotFound`.

### Art eines Anhangs

| Art     | Regel                                                           | Tipp auf die Karte |
| ------- | --------------------------------------------------------------- | ------------------ |
| `image` | `imageMime(name)`: png, jpg, jpeg, gif, webp (unverändert, 036) | Lightbox           |
| `pdf`   | `viewerKind(name) === 'pdf'` (Rust: `viewer_kind == Pdf`)       | Anhang-Ansicht     |
| `text`  | `viewerKind(name) === 'text'` (Rust: `viewer_kind == Text`)     | Anhang-Ansicht     |
| `other` | alles andere                                                    | Speichern unter    |

Die Reihenfolge gilt von oben nach unten. Nach einem Umbenennen gilt die neue Art (Randfall der Spec).

## Hinweis-Bestätigung (neu, ohne Migration)

| Tabelle       | `vault_device_uuid` | `key`                         | `value` |
| ------------- | ------------------- | ----------------------------- | ------- |
| `preferences` | UUID dieses Geräts  | `passwords.attachmentOpenAck` | `"1"`   |

- Geschrieben nur von `passwords_attachment_open_system` mit `confirmed && dontAskAgain`, über
  `storage::preferences` mit `PrefScope::Device`.
- Fehlt die Zeile, braucht jedes Öffnen mit der System-App den Hinweis.
- Die Zeile ist an das Gerät gebunden; ein anderes Gerät liest nur seine eigene (FR-010). Wird das
  Gerät aus der Vault entfernt, fällt sie mit (`ON DELETE CASCADE`).

## Text für die Ansicht (vorhanden, Spec 044)

`TextContent { text: string, truncated: boolean }` aus `files::local::text`, unverändert. Höchstens
`TEXT_LIMIT` = 5 MB; ungültiges UTF-8 als Ersatzzeichen; ein NUL in den ersten 8 KiB ist eine
Binärdatei.

## Freigabe-URL für ein PDF (vorhanden, Spec 044)

Eine Zeile im Speicher des `MediaServer` (`Grant { tab, source }`), Quelle neu `BytesSource`
(`Arc<[u8]>`, Medientyp `application/pdf`).

| Zustand    | Übergang                                                                      |
| ---------- | ----------------------------------------------------------------------------- |
| angemeldet | `passwords_attachment_open` gibt die URL zurück                               |
| beendet    | `files_release(url)` beim Schließen der Ansicht oder beim Wechsel des Anhangs |
| beendet    | `files_release_tab(tabId)`, wenn der Tab schließt                             |
| beendet    | Media-Server endet mit dem Token der Vault (Sperren, Schließen)               |

## Klartext-Kopie (neu, Datei)

`<AppCache>/files-opened-encrypted/<uuid>/<copy_file_name(file_name)>`, derselbe Ort wie in 048.

| Zustand  | Übergang                                                              |
| -------- | --------------------------------------------------------------------- |
| angelegt | `passwords_attachment_open_system` nach Hinweis oder Bestätigung      |
| entfernt | sofort, wenn das Öffnen scheitert (FR-012)                            |
| entfernt | beim Sperren oder Schließen der Vault (`FilesState::forget_on_close`) |
| entfernt | beim nächsten Start nach einem Absturz (`FilesState::from_app`)       |
