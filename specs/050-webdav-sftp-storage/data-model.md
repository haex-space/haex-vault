# Data Model: WebDAV- und SFTP-Speicher neben S3

Erweitert [038 data-model.md](../038-storage-connections/data-model.md). Was dort steht, gilt weiter,
soweit hier nichts anderes steht.

## Migration `00NN_storage_protocols`

Nächste freie Nummer bei der Umsetzung (nach `0029_agent_file_permissions`; Spec 048 nimmt eventuell
vorher eine für ihre `_no_sync`-Tabellen). `HOLZI_TRIGGER_VERSION` steigt um eins, weil eine
CRDT-Tabelle neue Spalten bekommt (research R13).

```sql
ALTER TABLE haex_storage_connections ADD COLUMN protocol TEXT NOT NULL DEFAULT 's3';
ALTER TABLE haex_storage_connections ADD COLUMN host_key TEXT;
ALTER TABLE haex_storage_connections ADD COLUMN host_key_confirmed_at TEXT;
```

Vorhandene Verbindungen sind danach `s3`. Keine UNIQUE-Bedingung (synchronisierte Tabelle).

## `haex_storage_connections` (synchronisiert)

| Spalte                  | S3 (unverändert)           | WebDAV                                                          | SFTP                                                              |
| ----------------------- | -------------------------- | --------------------------------------------------------------- | ----------------------------------------------------------------- |
| `protocol`              | `s3`                       | `webdav`                                                        | `sftp`                                                            |
| `provider_name`         | vom Nutzer                 | vom Nutzer                                                      | vom Nutzer                                                        |
| `provider_kind`         | `aws`, `rustfs`, `other`   | `hetznerStorageBox`, `nextcloud`, `koofr`, `kdrive`, `other`    | `hetznerStorageBox`, `other`                                      |
| `endpoint`              | Adresse oder leer (AWS)    | `https://host[:port]/pfad/` (mit `/` am Ende), `http` nur lokal | `sftp://host[:port]` (ohne Pfad)                                  |
| `endpoint_scope`        | `public` / `local`         | wie S3                                                          | wie S3                                                            |
| `region`                | Region                     | leer                                                            | leer                                                              |
| `addressing`            | `path` / `virtual`         | `path` (ohne Bedeutung)                                         | `path` (ohne Bedeutung)                                           |
| `host_key`              | NULL                       | NULL                                                            | bestätigter Hostschlüssel als OpenSSH-Zeile (`ssh-ed25519 AAAA…`) |
| `host_key_confirmed_at` | NULL                       | NULL                                                            | Zeitpunkt der Bestätigung (RFC 3339)                              |
| `credentials_item_id`   | Eintrag im Passwortmanager | wie S3                                                          | wie S3                                                            |

**Prüfregeln** (`remote_storage/store.rs`, je Protokoll):

- `protocol` nur `s3`, `webdav`, `sftp`; nach dem Anlegen unveränderlich (FR-001). Ein Speichern mit
  anderem Protokoll als gespeichert ist ein Fehler.
- `provider_kind` muss zum Protokoll passen (Tabelle).
- WebDAV: `endpoint` ist eine Adresse mit `https` oder (nur `local`) `http`, ohne Benutzer, Abfrage und
  Fragment; holzi ergänzt `/` am Ende.
- SFTP: `endpoint` ist `sftp://host` oder `sftp://host:port` (Port 1–65535), ohne Pfad. Ohne bestätigten
  `host_key` wird eine SFTP-Verbindung nicht gespeichert (der Test endet vorher mit `hostKeyUnknown`).
- Region und Adressierung werden bei WebDAV und SFTP ignoriert und als leer bzw. `path` gespeichert.

## `haex_storages` (synchronisiert, unverändert)

`bucket` trägt bei WebDAV und SFTP den **Ordnerpfad** relativ zur Verbindung: leer für den Wurzelordner,
sonst Teile mit `/` getrennt, ohne `/` am Anfang und Ende, kein Teil `.` oder `..`, kein Teil länger als
255 Byte, keine Steuerzeichen. Bei S3 gelten die Regeln für Buckets aus 038 (`bucket_ok`). Zwei Speicher
derselben Verbindung, deren Ordner ineinander liegen, ergeben beim Speichern eine Warnung, kein Fehler
(FR-003).

## `storage_tests_no_sync` (geräteeigen, unverändert)

`outcome` kennt zusätzlich `folderMissing`, `notWebDav`, `hostKeyUnknown`, `hostKeyChanged` (research R10).

## Zugangsdaten-Eintrag im Passwortmanager

Eigentümer `storage` wie in 038 (Regel Z14). Titel nach Protokoll: `S3: <Anbieter>`, `WebDAV: <Anbieter>`,
`SFTP: <Anbieter>`; `url` = Endpunkt.

| Feld des Eintrags           | S3               | WebDAV                | SFTP mit Passwort | SFTP mit Schlüssel                                        |
| --------------------------- | ---------------- | --------------------- | ----------------- | --------------------------------------------------------- |
| `username`                  | Zugangsschlüssel | Benutzername          | Benutzername      | Benutzername                                              |
| `password`                  | Geheimnis        | Passwort/App-Passwort | Passwort          | leer                                                      |
| eigenes Feld `sessionToken` | optional         | —                     | —                 | —                                                         |
| eigenes Feld `privateKey`   | —                | —                     | —                 | privater Schlüssel als Text (wie eingegeben oder erzeugt) |
| eigenes Feld `passphrase`   | —                | —                     | —                 | optional                                                  |

Ein erzeugter Schlüssel (`generatedKey`) wird beim Speichern als `privateKey` im Eintrag abgelegt.
`credentials::read` liefert je nach Protokoll der Verbindung (Typ unten) und meldet einen Eintrag ohne
die nötigen Felder als `missing` wie in 038.

## Typen in Rust (Auszug)

- `Protocol` = `S3 | WebDav | Sftp` (Text `s3`, `webdav`, `sftp`, `text_enum!`), ts-rs.
- `ProviderKind` ergänzt um `HetznerStorageBox`, `Nextcloud`, `Koofr`, `Kdrive`.
- `Location` ergänzt um `protocol: Protocol` und `host_key: Option<String>`; `bucket` = Bucket oder
  Ordnerpfad; `Location::is_tree()` = WebDAV oder SFTP.
- `Credentials` wird ein `enum`:
  - `S3 { access_key_id, secret_access_key, session_token }` (wie bisher),
  - `Password { username, password }` (WebDAV, SFTP mit Passwort),
  - `SshKey { username, private_key, passphrase }`.
    Alle Geheimnisse in `Zeroizing`, `Debug` zeigt nur den Benutzernamen bzw. Zugangsschlüssel.
- `ObjectInfo` ergänzt um `etag: Option<String>` (research R8).
- `StorageError` ergänzt um `QuotaExceeded`, `HostKeyChanged`, `HostKeyUnknown`, `Unsupported` und
  `Protocol` (PR C).
- `TestOutcome` ergänzt um `FolderMissing`, `NotWebDav`, `HostKeyUnknown`, `HostKeyChanged`.
- `TestResult` ergänzt um `hostKey?: { key, fingerprint }` (der vom Server gezeigte Schlüssel bei
  `hostKeyUnknown` und `hostKeyChanged`).

## Zustände einer SFTP-Verbindung (Hostschlüssel)

```text
neu ──Test──▶ hostKeyUnknown ──Nutzer bestätigt──▶ Speichern mit host_key ──Test──▶ gespeichert
gespeichert ──Server zeigt anderen Schlüssel──▶ hostKeyChanged (alle Aufrufe scheitern)
hostKeyChanged ──Nutzer nimmt neuen Schlüssel an (Einstellungen)──▶ gespeichert (host_key neu, Sync)
```

## Zwischendateien (nicht gespeichert)

`.holzi-part-<UUID>` im Zielordner, während eines Hochladens oder Kopierens über holzi (research R7).
Ausgeblendet beim Auflisten; älter als 24 Stunden (Änderungszeit beim Server) → gelöscht, wenn holzi sie
sieht.

## Sitzungen (nur im Speicher des Prozesses)

- **Upload-Sitzung** je Store: Kennung → Zustand (WebDAV: Strom und laufende Anfrage; SFTP: offene
  Zwischendatei und Stellen der Teile). Ohne Teil nach 30 Minuten verworfen.
- **Vorläufige SSH-Schlüssel**: Kennung → erzeugter privater Schlüssel (`Zeroizing`), bis zum Speichern,
  höchstens 30 Minuten, verworfen beim Sperren der Vault (FR-008).
- **Zuletzt gezeigter Hostschlüssel** je Verbindung (bei `hostKeyChanged`), Grundlage für
  `storage_host_key_accept`; nicht gespeichert.
- **SSH-Sitzungen** je Verbindung (Kennung, Hostschlüssel, Benutzer): bis zu zwei, nach 60 s ohne Aufruf
  geschlossen; verworfen bei neuen Zugangsdaten oder neuem Hostschlüssel.
