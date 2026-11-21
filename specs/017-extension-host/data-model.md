# Datenmodell: Erweiterungs-Host

Phase 1 von [plan.md](./plan.md). Begründungen in [research.md](./research.md) (R4, R5, R8, R10, R11, R15,
R16, R20).

Konventionen:

- Kerntabellen ohne Präfix wie die übrigen von haex-vault (`src-tauri/src/identity/migrations.rs`).
- Synchronisierte Tabellen haben **keine UNIQUE-Constraints**; Eindeutigkeit über abgeleitete Kennungen
  (UUIDv5, je Tabelle ein eigener Namensraum `NS_*`, Konstanten in `extensions/ids.rs`).
- Gerätebezogene, dauerhafte Zeilen folgen ADR-0001: Spalte `vault_device_uuid` mit Verweis auf
  `known_devices`; die Nil-Kennung bedeutet „vault-weit“.
- `_no_sync`-Tabellen beschreiben den Zustand dieser Datei (Journal, ausgeführtes Aufräumen, Protokolle, geparkte Gruppen,
  Entwicklermodus) und gehen nie in den Sync.
- Eine Migration `0023_extensions` (nach `0022_passwords` aus 034) in einer eigenen Datei
  `identity/migrations_extensions.rs`; `HAEX_TRIGGER_VERSION` 14 → 15.
- Tabellen **einer Erweiterung** (`<publicKey>__<name>__<tabelle>`) entstehen nur über deren Migrationen
  (R8) und sind hier nicht beschrieben.

## Synchronisierte Tabellen

### `extensions` — Erweiterung

| Spalte                       | Typ       | Bedeutung                                                                                |
| ---------------------------- | --------- | ---------------------------------------------------------------------------------------- |
| `id`                         | TEXT PK   | v5(`NS_EXT`, `lower(publicKey) ":" name`)                                                |
| `public_key`                 | TEXT      | 64 Hex-Zeichen, Ed25519                                                                  |
| `name`                       | TEXT      | `^[a-z][a-z0-9-]*$`, ohne `__` (FR-004)                                                  |
| `display_name`               | TEXT NULL | aus dem Manifest der wirksamen Fassung                                                   |
| `enabled`                    | INTEGER   | 1 aktiv, 0 deaktiviert (FR-007)                                                          |
| `state`                      | TEXT      | `installed` \| `removed`                                                                 |
| `purge_data`                 | INTEGER   | beim Entfernen: 1 = Daten löschen (FR-008)                                               |
| `purge_hlc`                  | TEXT NULL | HLC des Entfernens; ältere Änderungen an Tabellen der Erweiterung werden verworfen (R11) |
| `installed_at`, `updated_at` | INTEGER   | Millisekunden                                                                            |

Die Zeile bleibt nach dem Entfernen als Grabstein. Eine Neuinstallation setzt `state = installed` und lässt
`purge_data` und `purge_hlc` stehen; das Aufräumen richtet sich nach `purge_hlc`, nicht nach `state` (R11).

### `extension_bundles` — Fassung (unveränderlich bis auf `retired`)

| Spalte           | Typ                                      | Bedeutung                                                                                                                                                        |
| ---------------- | ---------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `id`             | TEXT PK                                  | v5(`NS_BUNDLE`, SHA-256 der signierten Nachricht) — gleiche Signatur, gleiche Zeile                                                                              |
| `extension_id`   | TEXT FK → `extensions` ON DELETE CASCADE |                                                                                                                                                                  |
| `version`        | TEXT                                     | Semver aus dem Manifest                                                                                                                                          |
| `manifest_json`  | BLOB                                     | exakte Bytes (JCS, R2)                                                                                                                                           |
| `signature_json` | BLOB                                     | exakte Bytes von `haextension/signature.json`                                                                                                                    |
| `retired`        | INTEGER                                  | 1 = durch ein bestätigtes Downgrade oder eine bestätigte Ersetzung derselben Version zurückgezogen (R11); einzige änderbare Spalte, erneute Installation setzt 0 |
| `added_at`       | INTEGER                                  |                                                                                                                                                                  |

**Wirksame Fassung**: höchste Semver mit `retired = 0`, bei Gleichstand die größere `id`.

### `extension_bundle_files` — Datei einer Fassung

| Spalte      | Typ                                             | Bedeutung                            |
| ----------- | ----------------------------------------------- | ------------------------------------ |
| `id`        | TEXT PK                                         | v5(`NS_BFILE`, `bundle_id ":" path`) |
| `bundle_id` | TEXT FK → `extension_bundles` ON DELETE CASCADE |                                      |
| `path`      | TEXT                                            | normalisierter Pfad nach R3          |
| `size`      | INTEGER                                         | Bytes                                |
| `sha256`    | TEXT                                            | Hex, Schlüssel in `extension_blobs`  |

Kein Fremdschlüssel auf `extension_blobs`: BLOBs können später ankommen (Zustand „wird übertragen“).

### `extension_blobs` — Dateiinhalt

| Spalte        | Typ          | Bedeutung                                                                                         |
| ------------- | ------------ | ------------------------------------------------------------------------------------------------- |
| `hash`        | TEXT PK      | SHA-256 (Hex) des Inhalts                                                                         |
| `data`        | BLOB         | Inhalt, ≤ 25 MiB                                                                                  |
| `size`        | INTEGER      |                                                                                                   |
| `orphaned_at` | INTEGER NULL | Zeitpunkt, seit dem keine Datei mehr auf den BLOB verweist; Aufräumen nach sieben Tagen (wie 034) |

Jeder BLOB wird in einem eigenen `write` geschrieben. Beim Lesen wird der Hash immer neu geprüft (R4).

### `extension_migrations` — Migration

| Spalte         | Typ                                      | Bedeutung                                                     |
| -------------- | ---------------------------------------- | ------------------------------------------------------------- |
| `id`           | TEXT PK                                  | v5(`NS_MIG`, `extension_id ":" name ":" sql_sha256`)          |
| `extension_id` | TEXT FK → `extensions` ON DELETE CASCADE |                                                               |
| `name`         | TEXT                                     | Name der Migration (Reihenfolge nach Journal-Index oder Name) |
| `position`     | INTEGER                                  | Reihenfolge                                                   |
| `sql`          | TEXT                                     | Anweisungen, getrennt durch `--> statement-breakpoint`        |
| `sql_sha256`   | TEXT                                     |                                                               |

Gleicher Name mit anderem SQL ergibt zwei Zeilen; das ist erkennbar, und die Erweiterung startet dann nicht
(R11). Die Zeilen bleiben, solange Daten behalten werden (FR-008).

### `extension_permissions` — gemerkte Berechtigung

| Spalte              | Typ                                         | Bedeutung                                                                                                     |
| ------------------- | ------------------------------------------- | ------------------------------------------------------------------------------------------------------------- |
| `id`                | TEXT PK                                     | v5(`NS_PERM`, `extension_id \| kind \| action \| target \| vault_device_uuid`)                                |
| `extension_id`      | TEXT FK → `extensions` ON DELETE CASCADE    |                                                                                                               |
| `kind`              | TEXT                                        | `database` \| `filesystem` \| `web` \| `notifications` \| `passwords` \| `remoteStorage` \| `mail` \| `shell` |
| `action`            | TEXT                                        | je Art, siehe [contracts/permissions.md](./contracts/permissions.md)                                          |
| `target`            | TEXT                                        | `*`, Präfix mit `*` oder genauer Wert; bei `database` Präfix einer Erweiterung oder eine ihrer Tabellen       |
| `status`            | TEXT                                        | `granted` \| `denied` \| `ask`                                                                                |
| `declared`          | INTEGER                                     | 1 = im Manifest erklärt                                                                                       |
| `vault_device_uuid` | TEXT FK → `known_devices` ON DELETE CASCADE | Nil = vault-weit; sonst nur dieses Gerät (ADR-0001)                                                           |
| `updated_at`        | INTEGER                                     |                                                                                                               |

Unbekannte `kind`/`action`/`target` gelten beim Lesen als nicht vorhanden (FR-022). Vorläufige Berechtigungen
stehen nicht hier, sondern nur im Speicher.

### `extension_limits` — Grenzwerte

| Spalte               | Typ                                      | Bedeutung                          |
| -------------------- | ---------------------------------------- | ---------------------------------- |
| `id`                 | TEXT PK                                  | v5(`NS_LIM`, `extension_id`)       |
| `extension_id`       | TEXT FK → `extensions` ON DELETE CASCADE |                                    |
| `max_rows`           | INTEGER                                  | Standard 10.000                    |
| `max_concurrent`     | INTEGER                                  | Standard 20                        |
| `max_sql_bytes`      | INTEGER                                  | Standard 1.000.000                 |
| `timeout_ms`         | INTEGER                                  | Standard 5.000                     |
| `max_response_bytes` | INTEGER                                  | Netz und Ergebnis, Standard 16 MiB |

Fehlt die Zeile, gelten die Standardwerte. Nur die Oberfläche von haex-vault ändert sie.

### `extension_device_status` — Zustand je Gerät (Anzeige)

| Spalte              | Typ                                         | Bedeutung                                                                           |
| ------------------- | ------------------------------------------- | ----------------------------------------------------------------------------------- |
| `id`                | TEXT PK                                     | v5(`NS_DEVST`, `extension_id ":" vault_device_uuid`)                                |
| `extension_id`      | TEXT FK → `extensions` ON DELETE CASCADE    |                                                                                     |
| `vault_device_uuid` | TEXT FK → `known_devices` ON DELETE CASCADE |                                                                                     |
| `status`            | TEXT                                        | `transferring` \| `ready` \| `signature_failed` \| `migration_failed` \| `disabled` |
| `bundle_id`         | TEXT NULL                                   | Fassung, die auf dem Gerät läuft                                                    |
| `error`             | TEXT NULL                                   | Fehlerart ohne Daten der Erweiterung; `parked_limit`: Parkgrenze erreicht (R10)     |
| `updated_at`        | INTEGER                                     |                                                                                     |

### `extension_kv` — Schlüssel-Wert-Speicher

| Spalte              | Typ                                          | Bedeutung                                         |
| ------------------- | -------------------------------------------- | ------------------------------------------------- |
| `vault_device_uuid` | TEXT FK → `known_devices` ON DELETE CASCADE  | Gerät (ADR-0001)                                  |
| `extension_id`      | TEXT FK → `extensions` ON DELETE CASCADE     |                                                   |
| `key`               | TEXT                                         | ≤ 1 KiB                                           |
| `value`             | TEXT                                         | ≤ 1 MiB; gesamt je Erweiterung und Gerät ≤ 10 MiB |
| PK                  | (`vault_device_uuid`, `extension_id`, `key`) |                                                   |

## Geräteeigene Tabellen (`_no_sync`)

### `extension_migrations_applied_no_sync` — Journal

| Spalte         | Typ                      | Bedeutung           |
| -------------- | ------------------------ | ------------------- |
| `extension_id` | TEXT                     |                     |
| `name`         | TEXT                     |                     |
| `sql_sha256`   | TEXT                     | Abweichung = Fehler |
| `applied_at`   | INTEGER                  |                     |
| PK             | (`extension_id`, `name`) |                     |

### `extension_logs_no_sync` — Protokoll

| Spalte              | Typ                      | Bedeutung                                             |
| ------------------- | ------------------------ | ----------------------------------------------------- |
| `id`                | INTEGER PK AUTOINCREMENT |                                                       |
| `extension_id`      | TEXT                     |                                                       |
| `vault_device_uuid` | TEXT                     | schützt vor fremden Einträgen nach Kopieren der Datei |
| `level`             | TEXT                     | `debug` \| `info` \| `warn` \| `error`                |
| `message`           | TEXT                     | ≤ 4 KiB                                               |
| `metadata`          | TEXT NULL                | JSON, ≤ 16 KiB                                        |
| `created_at`        | INTEGER                  |                                                       |

Ringpuffer: höchstens 5.000 Einträge je Erweiterung, älteste fallen weg.

### `sync_parked_groups_no_sync` — geparkte Sync-Gruppe

| Spalte             | Typ                      | Bedeutung                                                                                                               |
| ------------------ | ------------------------ | ----------------------------------------------------------------------------------------------------------------------- |
| `id`               | INTEGER PK AUTOINCREMENT |                                                                                                                         |
| `origin`           | TEXT                     | Ursprungsgerät                                                                                                          |
| `hlc`              | TEXT                     | HLC der Gruppe                                                                                                          |
| `extension_prefix` | TEXT                     | `<publicKey>__<name>__`                                                                                                 |
| `tables`           | TEXT                     | JSON-Liste der berührten Tabellen                                                                                       |
| `group_blob`       | BLOB                     | die Gruppe, wie sie ankam                                                                                               |
| `bytes`            | INTEGER                  | Grenze je Erweiterung 256 MiB; an der Grenze hält der Empfang an, nichts wird verworfen (R10)                           |
| `reason`           | TEXT                     | `missing_table` \| `missing_column` \| `after_parked` \| `awaiting_purge` \| `dev_version` \| `refetch` (R10, R11, R16) |
| `parked_at`        | INTEGER                  |                                                                                                                         |

### `extension_purges_applied_no_sync` — ausgeführtes Aufräumen (lokal)

| Spalte         | Typ     | Bedeutung                                               |
| -------------- | ------- | ------------------------------------------------------- |
| `extension_id` | TEXT PK | Erweiterung                                             |
| `purge_hlc`    | TEXT    | zuletzt auf diesem Gerät ausgeführter `purge_hlc` (R11) |

### `dev_extensions_no_sync`, `dev_extension_permissions_no_sync` — Entwicklermodus

Wie `extensions` und `extension_permissions`, zusätzlich `vault_device_uuid`, `project_path` und `dev_url`
(nur `localhost` oder `127.0.0.1`: eine CSP-Quelle kann keine IPv6-Adresse nennen); Kennung v5(`NS_DEV`,
`device ":" publicKey ":" name`). Keine Fassungen, keine BLOBs: Dateien kommen vom Entwicklungsserver.

### `dev_extension_kv_no_sync` — Speicher einer Entwicklungsfassung (Migration `0024_dev_extension_kv`)

| Spalte         | Typ                                                  | Bedeutung                             |
| -------------- | ---------------------------------------------------- | ------------------------------------- |
| `extension_id` | TEXT FK → `dev_extensions_no_sync` ON DELETE CASCADE | Entwicklungsfassung                   |
| `key`, `value` | TEXT                                                 | wie `extension_kv`, ohne Gerätespalte |

Entladen löscht Registrierung, Berechtigungen, diesen Speicher und die Tabellen der Entwicklungsfassung.

## Nur im Speicher (Rust)

- **Rahmensitzung**: `frame` (Zufallskennung), `extension_id`, `tab_id`, `frame_instance_id`, Start-Token,
  Generation des Kanals, offen seit. Endet mit dem Tab oder der Vault.
- **Vorläufige Berechtigung**: wie `extension_permissions`, ohne Geltungsbereich; endet mit der Vault. Dazu
  rahmengebundene Berechtigungen aus Dialogen (R19), die mit dem Rahmen enden.
- **Anfrage**: `request_id`, Erweiterung, Art, Aktion, Ziel, erklärt, gerätebezogen, wartende Rahmen.
- **Beobachtungen** (Dateien, Mail) und **Shell-Sitzungen**: je (Erweiterung, Kennung), mit Besitzprüfung.
- **Semaphoren** je Erweiterung (gleichzeitige Anfragen).

## Zustände und Übergänge

### Erweiterung auf einem Gerät

```text
(nicht vorhanden) ──Sync/Installation──▶ transferring ──alle BLOBs da──▶ prüfen
prüfen ──Signatur falsch──▶ signature_failed            (startet nicht, Sync läuft weiter)
prüfen ──ok──▶ migrieren ──Fehler──▶ migration_failed   (startet nicht, Gruppen bleiben geparkt)
migrieren ──ok──▶ ready ──geparkte Gruppen anwenden──▶ ready
ready ──enabled = 0──▶ disabled ──enabled = 1──▶ prüfen
ready/disabled ──state = removed──▶ aufräumen (Tabellen nur bei purge_data) ──▶ (nicht vorhanden)
ready ──neue wirksame Fassung──▶ transferring/prüfen/migrieren (offene Tabs laden neu)
```

### Berechtigung

```text
erklärt + bestätigt ──▶ granted (vault-weit; shell: dieses Gerät)
erklärt + abgewählt ──▶ ask
nicht erklärt, angefragt ──Erlauben/Verweigern ohne Merken──▶ vorläufig (Speicher)
                        ──mit Merken──▶ granted/denied (Geltungsbereich nach Art)
Einstellungen ──ändern/widerrufen──▶ neuer Zustand, sofort wirksam (Cache verworfen)
erklärt, Update erklärt sie nicht mehr ──▶ gelöscht
nicht erklärt + gemerkt, Update erklärt sie ──▶ declared = 1, Zustand bleibt
```

## Rust-Typen (nicht gespeichert; ts-rs-Export nach `src/types/bindings/`)

- `ExtensionId(Uuid)`, `ExtensionRef { public_key, name }`, `TablePrefix` (exakte Zerlegung, R6).
- `Manifest` (Name, Fassung, Herausgeber, Einstieg, Symbol, Beschreibung, `single_instance`, `display_mode`
  wird ignoriert (FR-014), `migrations_dir`, `i18n`, `permissions: DeclaredPermissions`).
- `PermissionKind`, `PermissionAction`, `PermissionTarget`, `PermissionStatus`, `GrantScope { Vault, Device(Uuid) }`.
- `BridgeRequest { id, method, params }`, `BridgeResponse { id, result | error { code, message, details } }`.
- `ExtensionErrorCode` (HV-Codes plus 8000 „nicht unterstützt“, 8001 „nicht verfügbar“, 8002
  „deaktiviert“), siehe [contracts/bridge.md](./contracts/bridge.md).
- `SqlResult { rows: Vec<Vec<Json>>, columns: Vec<String>, rows_affected: u64, last_insert_id: Option<i64> }`.
