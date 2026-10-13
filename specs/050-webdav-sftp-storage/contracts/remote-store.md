# Contract: `RemoteStore` über drei Protokolle

Der Trait in `src-tauri/src/remote_storage/mod.rs` ist die eine Schnittstelle, über die Einstellungen,
Test, Dateibrowser, Transfers, Suche, Agents, Erweiterungen und später verschlüsselte Ordner einen
Speicher erreichen (FR-018). `ProtocolStore` reicht nach `access.location.protocol` an `S3Store`,
`WebDavStore` oder `SftpStore` weiter (research R1). Jeder Aufruf endet an seiner Frist (FR-013).

## Methoden und Verhalten je Protokoll

| Methode              | S3 (wie heute)                 | WebDAV                                                               | SFTP                                                                 |
| -------------------- | ------------------------------ | -------------------------------------------------------------------- | -------------------------------------------------------------------- |
| `put`                | `PUT`                          | Zwischendatei, dann `MOVE` (R7); Schlüssel mit `/` am Ende → `MKCOL` | Zwischendatei, dann Umbenennen (R7); `/` am Ende → `create_dir`      |
| `get`                | `GET` bis `max_bytes`          | `GET` bis `max_bytes`                                                | lesen bis `max_bytes`                                                |
| `list` (rekursiv)    | Seiten mit Präfix              | Ebene für Ebene (`Depth: 1`), Ordner als `ordner/` mit Größe 0       | `read_dir` Ebene für Ebene, Ordner als `ordner/`                     |
| `list_dir`           | `delimiter=/`                  | eine Ebene: Dateien als Objekte, Ordner als Präfixe (auch leere)     | wie WebDAV                                                           |
| `delete`             | `DELETE`                       | `DELETE` einer Datei; `ordner/` → nur wenn leer                      | `remove_file`; `ordner/` → `remove_dir` (scheitert, wenn nicht leer) |
| `head`               | `HEAD`                         | `PROPFIND` `Depth: 0`                                                | `metadata`                                                           |
| `get_range`          | `GET` mit `Range`              | `GET` mit `Range` (200 ohne Bereich: Bytes vor `start` verwerfen)    | Datei öffnen, an `start` springen, `len` Bytes als Strom             |
| `copy`               | `CopyObject` bis 5 GB          | `COPY` mit `Overwrite: T`                                            | nicht angeboten (`copies_on_provider` = nein)                        |
| `copies_on_provider` | ja                             | ja                                                                   | nein → der Transfer streamt über holzi                               |
| `rename_tree`        | `Unsupported` (nie aufgerufen) | `MOVE` des Ordners                                                   | Umbenennen des Ordners                                               |
| `remove_tree`        | `Unsupported` (nie aufgerufen) | `DELETE` des Ordners                                                 | rekursiv von unten                                                   |
| `begin_upload`       | `CreateMultipartUpload`        | gestreamter `PUT` auf Zwischendatei beginnt                          | Zwischendatei anlegen                                                |
| `upload_part`        | `UploadPart` → ETag            | Teil in den Strom; Wiederholen nach Fehler nicht möglich (`Network`) | Teil an seine Stelle schreiben; Wiederholen möglich                  |
| `finish_upload`      | `CompleteMultipartUpload`      | Strom schließen, Antwort abwarten, `MOVE` auf den Zielnamen          | `fsync` (falls möglich), schließen, umbenennen                       |
| `abort_upload`       | `AbortMultipartUpload`         | Strom abbrechen, Zwischendatei löschen                               | Zwischendatei löschen                                                |

## Gemeinsame Regeln

- **Schlüssel** sind Pfade relativ zum Ordner des Speichers (bei S3: Objektschlüssel im Bucket). Teile
  vor einem `/` sind Ordner. Ein Schlüssel mit `/` am Ende bezeichnet einen Ordner.
- **Zwischendateien** `.holzi-part-<UUID>` erscheinen in keiner Antwort; ältere als 24 Stunden löscht
  `list`/`list_dir` nebenbei (R7).
- **Fehlende Elternordner** unterhalb des Speicherordners legt `put`/`begin_upload` an; fehlt der
  Speicherordner selbst, ist das `NotFound` (R2).
- **`ObjectInfo.etag`**: S3 ETag, WebDAV `getetag` (ohne Anführungszeichen), SFTP `"<size>-<mtime>"`.
  Bei SFTP hat `mtime` nur Sekundenauflösung; gleich große Überschreibungen innerhalb derselben
  Sekunde können daher dieselbe Marke ergeben. Falls Spec 048 solche Änderungen erkennen muss,
  muss sie für SFTP ein stärkeres Änderungsmerkmal zusätzlich verlangen.
- **`last_modified`**: in Auflistungen (`ObjectInfo`) bei jedem Protokoll ISO 8601 wie bei S3 (WebDAV
  rechnet `getlastmodified` um, SFTP die Unix-Zeit; bei PR D geändert, weil der Dateibrowser ISO liest),
  bei `head` (`ObjectHead`) ein HTTP-Datum wie der `Last-Modified`-Kopf von S3.
- **Fehler**: `NotFound`, `AccessDenied`, `MissingRight`, `Network`, `TooLarge`, `TimedOut` wie heute,
  dazu `QuotaExceeded`, `HostKeyChanged`, `HostKeyUnknown`, `Unsupported` (R10) und `Protocol` (der
  Server antwortet nicht wie sein Protokoll: Weiterleitung, 405 auf `PROPFIND`, keine 207). Fehlermeldungen und
  Log nennen nie Zugangsdaten; das Log nennt Status bzw. SFTP-Fehlercode, nicht den Antworttext.
- **Gleichzeitigkeit**: je Verbindung höchstens 4 WebDAV-Anfragen bzw. 2 SSH-Sitzungen (R12).
