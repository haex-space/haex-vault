# Data Model: Verschlüsselte Ordner in Speichern (048)

Die Wahrheit liegt im Bucket ([contracts/format.md](./contracts/format.md)). In der Vault-Datenbank
liegen nur Zwischenspeicher, die jedes Gerät aus dem Bucket neu aufbauen kann, und Freigaben.

## Im Bucket

| Entität                | Objekt                 | Felder (verschlüsselt, soweit nicht anders gesagt)                                                                                          |
| ---------------------- | ---------------------- | ------------------------------------------------------------------------------------------------------------------------------------------- |
| Verschlüsselter Ordner | `…/<fid>.hxef/h`       | sichtbar: `fid`, Formatversion, Empfänger-Arten und `key_id`; verschlüsselt: `FK` je Empfänger, Name                                        |
| Begleitdatei           | `…/<fid>.hxef/m/<sid>` | `entry`, `parent`, `name`, `kind`, `revision`, `base`, `written`; bei Dateien `size`, `modified`, `type`, `sha256`, `content`, `cid`, `dek` |
| Inhaltsobjekt          | `…/<fid>.hxef/c/<oid>` | sichtbar: `cid`, Länge; verschlüsselt: Inhalt in Blöcken                                                                                    |

**Zustände eines verschlüsselten Ordners** (aus dem Kopf, für die Anzeige):

```text
readable     Kopf gültig, ein Empfänger mit einem Schlüssel dieses Geräts
otherVault   Kopf gültig, kein Empfänger entpackbar
newerFormat  VERSION > 1
damaged      Kopf fehlt, falsche MAGIC, fid passt nicht zum Präfix, Name nicht zu öffnen
```

**Fassungen eines Eintrags**:

```text
(keine) --anlegen--> r1 --ändern/umbenennen/verschieben--> r2 --> … --löschen--> (keine)
r_n und r_n' mit gleicher base  ==>  Konflikt: beide sichtbar, die ältere als Konfliktkopie
```

## Vault-Datenbank

Neue Migration `00NN_encrypted_folders` (nächste freie Nummer bei der Umsetzung; nach
`0029_agent_file_permissions` aus 044, falls die vorher kommt). Beide Tabellen enden auf `_no_sync`,
bekommen keine CRDT-Trigger und brauchen keine neue Trigger-Version.

### `encrypted_folder_heads_no_sync`

| Spalte       | Typ              | Regel                                                        |
| ------------ | ---------------- | ------------------------------------------------------------ |
| `storage_id` | TEXT NOT NULL    | Id aus `haex_storages`                                       |
| `object_key` | TEXT NOT NULL    | Schlüssel des Kopfes beim Anbieter                           |
| `etag`       | TEXT NOT NULL    | ETag beim letzten Laden                                      |
| `folder_id`  | TEXT NOT NULL    | Base32 von `fid`                                             |
| `name`       | TEXT             | entschlüsselter Name; NULL, wenn nicht `readable`            |
| `state`      | TEXT NOT NULL    | `readable`, `otherVault`, `newerFormat`, `damaged`           |
| `key_id`     | BLOB             | Generation, mit der `FK` entpackt wurde                      |
| `open_ack`   | INTEGER NOT NULL | 1 = „Nicht mehr fragen“ für „Mit System-App öffnen“ (FR-028) |
| `loaded_at`  | INTEGER NOT NULL | ms                                                           |

PRIMARY KEY (`storage_id`, `object_key`).

`open_ack` ist eine Gerätepräferenz, kein Zwischenspeicher: Neuladen aus dem Bucket (neues ETag,
Umbenennen) lässt sie stehen; beim Verschieben des Ordners (neuer `object_key`, gleiche `folder_id`)
zieht sie mit.

### `encrypted_folder_entries_no_sync`

| Spalte         | Typ              | Regel                          |
| -------------- | ---------------- | ------------------------------ |
| `storage_id`   | TEXT NOT NULL    |                                |
| `folder_id`    | TEXT NOT NULL    |                                |
| `sidecar`      | TEXT NOT NULL    | `sid` (Base32)                 |
| `etag`         | TEXT NOT NULL    |                                |
| `entry_id`     | TEXT NOT NULL    |                                |
| `parent_id`    | TEXT NOT NULL    |                                |
| `name`         | TEXT NOT NULL    |                                |
| `kind`         | TEXT NOT NULL    | `file`, `folder`               |
| `revision`     | INTEGER NOT NULL |                                |
| `base`         | TEXT             |                                |
| `written`      | INTEGER NOT NULL | ms                             |
| `size`         | INTEGER          | nur Dateien                    |
| `modified`     | INTEGER          | nur Dateien                    |
| `content_type` | TEXT             | nur Dateien                    |
| `sha256`       | TEXT             | nur Dateien                    |
| `object_id`    | TEXT             | nur Dateien; `oid` (`content`) |
| `content_id`   | TEXT             | nur Dateien; `cid`             |
| `wrapped_dek`  | BLOB             | nur Dateien; bleibt verpackt   |

PRIMARY KEY (`storage_id`, `folder_id`, `sidecar`); INDEX (`storage_id`, `folder_id`, `parent_id`).

- `FK` und `DEK` liegen nie in der Datenbank; holzi entpackt sie bei Bedarf und hält sie nur im
  Arbeitsspeicher (`Zeroizing`).
- `remote_storage::store::remove_storage` löscht die Zeilen eines Speichers im selben Vorgang.
- Fehlt eine Zeile oder passt ein ETag nicht, lädt holzi aus dem Bucket nach (research R6).

### `agent_file_permissions` (aus 044)

- Neuer `kind`: `encryptedFolder`, `target` = `<storage_id>/<folder_id>`, `status` = `read`,
  `readWrite` oder `denied`.
- Neue Spalte `reach` TEXT: `local` oder `cloud` bei `encryptedFolder`, sonst NULL. Liegt die
  Migration 0029 von 044 noch nicht vor, kommt die Spalte gleich mit ihr; sonst als `ALTER TABLE` in
  der Migration von 048 (synchronisierte Tabelle: Trigger-Version erhöhen).
- Zeilen entstehen nur mit „Erlaubnis merken“ (FR-031).

### Berechtigungen der Erweiterungen (aus 017)

- Neue Art `encryptedFolder`, Ziel `<storage_id>/<folder_id>`, Aktionen `read`/`readWrite`; Zeilen nur
  mit „Erlaubnis merken“, nie aus einem Manifest ([contracts/bridge.md](./contracts/bridge.md)).

### Verschlüsselte Ordner auf dem Gerät (User Story 7, research R16)

- Keine neue Tabelle. Die Spalte `storage_id` der beiden `_no_sync`-Tabellen und das Ziel der
  Freigaben (`<storage_id>/<folder_id>`) tragen für das Gerät die Kennung `device:<wurzel>`
  (`device:/` unter Linux, macOS und Android; `device:C:` je Laufwerk unter Windows); `object_key` ist
  der Pfad ohne die Wurzel.
- Zeilen zu Ordnern, deren Kopf beim Laden fehlt, löscht holzi.
- `EncryptedInfo` bekommt `foreign: number` (Zahl fremder Dateien, nur für den Ordner selbst, sonst 0).

## Nur im Arbeitsspeicher

| Entität                           | Inhalt                                                  | Lebensdauer                                          |
| --------------------------------- | ------------------------------------------------------- | ---------------------------------------------------- |
| Geöffneter verschlüsselter Ordner | `FK`, `K_meta`, `K_wrap`, Baum der Einträge             | bis zum Sperren der Vault                            |
| Gehaltene Freigabe (Agent)        | Agent, Ordner, Stufe, Reichweite, Turn oder MCP-Sitzung | Ende des Turns oder der Sitzung, höchstens Sperren   |
| Gehaltene Freigabe (Erweiterung)  | Erweiterung, Ordner, Stufe                              | Entladen oder Neuladen des Frames, höchstens Sperren |
| Vorschaubilder                    | JPEG-Bytes, Schlüssel wie 044                           | LRU, 64 MB, bis zum Sperren                          |
| Blockfenster einer offenen Datei  | bis zu zwei Fenster à 8 MiB                             | solange die Freigabe-URL gilt                        |

## Auf dem Gerät, außerhalb der Datenbank

- `<AppCache>/files-opened-encrypted/<uuid>/<name>`: Kopien für „Mit System-App öffnen“; gelöscht beim
  Sperren oder Schließen der Vault und beim Start.
- Gerätepräferenz `files.encryptedNotice` (`true` nach dem Hinweis aus FR-002).
