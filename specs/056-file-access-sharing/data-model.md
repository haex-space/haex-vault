# Data Model: Dateizugriff zwischen eigenen Geräten und Dateifreigaben an Kontakte

Migration `0039_file_shares` in `src-tauri/src/identity/migrations_file_shares.rs` (mit
`_tests.rs` und `SYNCED_TABLES`), registriert in `identity/migrations.rs`; `HOLZI_TRIGGER_VERSION`
26 → 27 mit Zeile im Änderungsprotokoll. Regeln der Vault-Datenbank: Primärschlüssel Pflicht, keine
UNIQUE-Bedingungen, Eindeutigkeit über abgeleitete IDs (UUIDv5 über `\n`-verbundene Teile); bei einer
Tabelle mit nur einer verfolgten Spalte kein `INSERT … ON CONFLICT DO UPDATE` (prüfen, dann schreiben).

## Synchronisiert (alle eigenen Geräte)

### `haex_file_shares` — Dateifreigabe

| Spalte              | Typ  | Regeln                                                                                                                                                         |
| ------------------- | ---- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `id`                | TEXT | PK, UUIDv5(`vault_device_uuid\nsource\npath`) — eine Freigabe je Quelle und Gerät (FR-017)                                                                     |
| `vault_device_uuid` | TEXT | bedienendes Gerät; FK `known_devices(vault_device_uuid) ON DELETE CASCADE` (FR-018)                                                                            |
| `source`            | TEXT | `device` oder `storage:<storage_id>`; nie `peer:`/`share:` (FR-015)                                                                                            |
| `path`              | TEXT | Fensterpfad der Wurzel in der Quelle, bei `device` absolut in `/`-Form (research R3)                                                                           |
| `item_kind`         | TEXT | `folder` oder `file`                                                                                                                                           |
| `name`              | TEXT | angezeigter Name, 1–255 Zeichen, Vorgabe Name der Wurzel (FR-016)                                                                                              |
| `created_at`        | TEXT | ISO-8601                                                                                                                                                       |
| `changed_at`        | TEXT | ISO-8601; gesetzt bei jeder Änderung der Freigabe oder ihrer Zugriffsrechte (research R10)                                                                     |
| `served_at`         | TEXT | ISO-8601 oder NULL; nur das bedienende Gerät setzt es auf `changed_at`, sobald es die Änderung gesehen hat                                                     |
| `source_state`      | TEXT | `ok`, `missing` oder `unavailable`; nur das bedienende Gerät schreibt es (beim Öffnen der Vault, beim Anlegen, bei jeder Anfrage, die an der Quelle scheitert) |

Zustand für „Meine Dateifreigaben“ (FR-020), in dieser Reihenfolge:

- `Änderung wartet auf ‹Gerät›`: `served_at IS NULL OR changed_at > served_at`.
- `Quelle fehlt`: `source_state = 'missing'` (FR-022; die Freigabe bleibt, keine Umlenkung).
- `Quelle nicht verfügbar`: `source_state = 'unavailable'`.
- sonst `aktiv`.

### `haex_file_share_grants` — Zugriffsrecht

| Spalte         | Typ  | Regeln                                                               |
| -------------- | ---- | -------------------------------------------------------------------- |
| `id`           | TEXT | PK, UUIDv5(`share_id\nvault_pubkey`) — eines je Kontakt und Freigabe |
| `share_id`     | TEXT | FK `haex_file_shares(id) ON DELETE CASCADE`                          |
| `vault_pubkey` | TEXT | x-only, 64 Hex-Zeichen klein; Vault-Identität des Kontakts           |
| `level`        | TEXT | `read` oder `readWrite`                                              |
| `updated_at`   | TEXT | ISO-8601                                                             |

Entziehen löscht die Zeile und setzt `haex_file_shares.changed_at`. Ein Zugriffsrecht wirkt nur, wenn
für `vault_pubkey` eine Karte in `haex_contacts_cards` liegt und die Vault nicht in
`haex_contacts_blocked_vaults` steht (research R11).

### `haex_file_shares_received` — Empfangene Dateifreigabe

| Spalte                | Typ  | Regeln                                                                    |
| --------------------- | ---- | ------------------------------------------------------------------------- |
| `id`                  | TEXT | PK, UUIDv5(`owner_vault_pubkey\nshare_id`); Speicher-Kennung `share:<id>` |
| `owner_vault_pubkey`  | TEXT | Vault-Identität des Kontakts                                              |
| `share_id`            | TEXT | Kennung der Freigabe beim Eigentümer                                      |
| `name`                | TEXT | Name, wie der Eigentümer ihn meldet                                       |
| `item_kind`           | TEXT | `folder` oder `file`                                                      |
| `level`               | TEXT | `read` oder `readWrite`, wie zuletzt gemeldet                             |
| `owner_device_pubkey` | TEXT | Geräteschlüssel des bedienenden Geräts                                    |
| `owner_endpoint_id`   | TEXT | Endpoint-ID des bedienenden Geräts (Hex)                                  |
| `last_reached_at`     | TEXT | ISO-8601 oder NULL                                                        |

Lebenszyklus: angelegt oder aktualisiert bei jeder erfolgreichen Abfrage `shares` (FR-030); fehlt eine
Freigabe in der Antwort des bedienenden Geräts, wird die Zeile gelöscht (FR-031). Wird der Kontakt
gelöscht, löscht ein Abonnent auf `haex_contacts_cards` die Zeilen dieser Vault.

## Nur dieses Gerät (`_no_sync`)

### `haex_file_peer_lists_no_sync` — höchste gesehene Geräteliste (FR-003)

| Spalte         | Typ     | Regeln                                        |
| -------------- | ------- | --------------------------------------------- |
| `vault_pubkey` | TEXT    | PK; nur fremde Vaults                         |
| `generation`   | INTEGER | höchste geprüfte Generation                   |
| `list_hash`    | TEXT    | SHA-256 der Liste mit dieser Generation (Hex) |
| `seen_at`      | TEXT    | ISO-8601                                      |

Bei gleicher Generation und anderem Hash gilt die Regel aus 024 (kleinster Hash) nicht: das bedienende
Gerät lehnt ab, bis eine höhere Generation kommt.

### `haex_file_device_access_no_sync` — Schalter „Geräte-Dateizugriff“ (FR-014)

| Spalte       | Typ     | Regeln                       |
| ------------ | ------- | ---------------------------- |
| `id`         | INTEGER | PK, immer 1                  |
| `enabled`    | INTEGER | 0 oder 1; fehlende Zeile = 1 |
| `updated_at` | TEXT    | ISO-8601                     |

## Laufzeitzustand (nicht gespeichert)

- **Verbindungen** je Gegenstelle mit `Mode::OwnDevice` oder `Mode::Contact { vault }` (research R4).
- **Laufende Streams** je (Freigabe, Vault): ein `CancellationToken`; ein Entzug oder Herabstufen
  bricht sie ab (FR-024).
- **Adressen von Kontakt-Geräten**: aus Antworten auf das Anklopfen, in der `MemoryLookup` des
  Endpoints, verfallen mit der Sitzung.
- **Upload-Sitzungen** des bedienenden Geräts: je Upload-ID Ziel, erwarteter Versionsstand und
  Zwischendatei; beim Abbruch, Entzug oder Sperren aufgeräumt.

## Typen für das Fenster (ts-rs, Präfix gegen Namenskollisionen)

- `FileShare { id, name, source: SourceRef, path, itemKind, servingDevice: VaultDeviceRef, grants:
FileShareGrant[], state: FileShareState }`
- `FileShareGrant { vaultPubkey, contactId, contactName, level: FileShareLevel }`
- `FileShareLevel = "read" | "readWrite"`
- `FileShareState = "active" | "sourceMissing" | "sourceUnavailable" | "pending"`
- `FilesPeerDevice { storageId, vaultDeviceUuid, name, state: FilesPeerState }`
- `FilesPeerState = "reachable" | "unreachable" | "accessOff"`
- `FilesContactShares { contactId, contactName, vaultPubkey, shares: FilesReceivedShare[] }`
- `FilesReceivedShare { storageId, name, itemKind, level, reachable }`

`Sources` bekommt `devices: FilesPeerDevice[]` und `contacts: FilesContactShares[]`.
