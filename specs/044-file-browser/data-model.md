# Data Model: Dateibrowser und Viewer (044)

Die meisten Entitäten leben nur im Speicher eines laufenden Prozesses. Neu in der Vault-Datenbank ist
eine Tabelle; dazu kommen Gerätepräferenzen und ein Cache-Verzeichnis.

## Vault-Datenbank

### `agent_file_permissions` (neu, mit der Vault synchronisiert)

Berechtigungen der Agents für Dateien (FR-031, FR-031a, FR-031b, research R14). Migration
`0029_agent_file_permissions` (nächste freie Nummer nach `0028_storage_connections`), Trigger-Version
erhöht.

| Spalte       | Typ              | Regel                                                                                      |
| ------------ | ---------------- | ------------------------------------------------------------------------------------------ |
| `id`         | TEXT PRIMARY KEY | deterministisch: UUIDv5 aus `agent_id`, `kind`, `target` (gleiche Zeile auf allen Geräten) |
| `agent_id`   | TEXT NOT NULL    | `builtin` oder die Agent-Id aus Spec 021                                                   |
| `kind`       | TEXT NOT NULL    | `device` oder `storage`                                                                    |
| `target`     | TEXT NOT NULL    | leer bei `device`; die Id aus `haex_storages` bei `storage`                                |
| `status`     | TEXT NOT NULL    | `device`: `granted` oder `denied`; `storage`: `read`, `readWrite` oder `denied`            |
| `updated_at` | INTEGER NOT NULL | Millisekunden                                                                              |

- Kein UNIQUE-Constraint (er würde die Synchronisierung bei einem Konflikt anhalten, wie bei den
  anderen synchronisierten Tabellen); die abgeleitete `id` sorgt dafür, dass (`agent_id`, `kind`,
  `target`) eine Zeile bleibt.
- Kein Fremdschlüssel auf `haex_storages` (CRDT-Tabellen); `remote_storage::store::remove_storage`
  löscht die Zeilen eines Speichers im selben Vorgang.
- **Auswertung**:
  - `device`, keine Zeile: `builtin` → erteilt, jeder andere Agent → nicht erteilt.
  - `storage`, keine Zeile: nicht erteilt → Rückfrage an den Nutzer (Lesen erlauben, Lesen und
    Schreiben erlauben, Ablehnen); die Antwort wird als Zeile gespeichert.
  - `denied` gilt ohne Rückfrage.

### Gerätepräferenzen (vorhanden, `preferences`, ADR 0001)

| Schlüssel      | Werte                                               | Vorgabe    |
| -------------- | --------------------------------------------------- | ---------- |
| `files.view`   | `list`, `grid`                                      | `list`     |
| `files.sort`   | `name`, `size`, `modified`, `type` + `:asc`/`:desc` | `name:asc` |
| `files.hidden` | `true`, `false`                                     | `false`    |

## Auf dem Gerät, außerhalb der Datenbank

### Vorschaubild-Cache

- Ort: `<AppCache>/files-thumbnails/`; liegt in den eigenen Orten von haex-vault und ist damit für Agents
  gesperrt und nie synchronisiert.
- Datei: `<hex(sha256(quelle ‖ pfad ‖ größe ‖ änderungszeit))>.jpg`, längste Kante 320 px.
- Merkzeichen `<hash>.broken` (leer) für Bilder, die nicht dekodiert werden konnten; ändert sich
  Größe oder Änderungszeit, ändert sich der Hash und der Versuch wird wiederholt.
- Kein Aufräumen nach Alter in dieser Spec; das Verzeichnis darf jederzeit geleert werden.

## Im Speicher (Rust)

### `SourceRef`

`Device` oder `Storage { storage_id }`. In der Adresse eines Tabs `device` bzw. `storage/<id>`.

### `Entry`

| Feld          | Typ            | Bemerkung                                                      |
| ------------- | -------------- | -------------------------------------------------------------- |
| `name`        | String         |                                                                |
| `path`        | String         | Gerät: absoluter Pfad; Speicher: Schlüssel bzw. Präfix mit `/` |
| `kind`        | `file` / `dir` |                                                                |
| `size`        | u64?           | nur Dateien                                                    |
| `modified_ms` | i64?           |                                                                |
| `mime`        | String?        | aus der Endung (`extensions/mime.rs::for_path`)                |
| `hidden`      | bool           | Punkt-Datei bzw. Attribut unter Windows                        |
| `symlink`     | bool           | nur Gerät                                                      |
| `no_access`   | bool           | Rechte des Systems fehlen                                      |
| `haex_owned`  | bool           | liegt in den eigenen Orten von haex-vault (FR-037: nur lesen)  |

### `MediaGrant` (Freigabe-URL, FR-016)

- `token` (UUIDv4), `tab_id`, `source: StreamingSource`, `content_type`.
- Lebenszyklus: `files_open` legt an → `files_release`/`files_release_tab` entfernt → Ende des Prozesses
  (Sperren der Vault, ADR 0003) entfernt alle. Ein unbekanntes Token liefert 404.

### `Transfer`

| Feld         | Bemerkung                                                                                   |
| ------------ | ------------------------------------------------------------------------------------------- |
| `id`         | UUIDv4                                                                                      |
| `op`         | `copy`, `move`, `delete`                                                                    |
| `from`, `to` | `SourceRef` + Pfade                                                                         |
| `items`      | Gesamtzahl, erledigt                                                                        |
| `bytes`      | gesamt (wenn bekannt), übertragen                                                           |
| `state`      | `queued` → `running` → `done` \| `cancelled` \| `failed{reason}`; `failed` kann neu starten |
| `conflict`   | offene Frage zum Namen: `replace` \| `keepBoth` \| `skip` (+ „für alle“)                    |

Übergänge: `running` → `waitingForConflict` → `running`; `running` → `cancelled` räumt Teil-Dateien
bzw. Multipart-Uploads auf; `failed` nach drei S3-Wiederholungen.

### `Search`

`id`, `root: SourceRef + Pfad`, `query`, `filters` (Typ, Größe von/bis, Zeitraum), `limits` (nur
Agents: 30 s, 500 Treffer), `CancellationToken`. Treffer sind `Entry` mit Punktzahl.

## Im Fenster (TypeScript)

### Ort eines Tabs (Sitzung, Spec 022)

| Ort      | Pfad           | Query                                   |
| -------- | -------------- | --------------------------------------- |
| Gerät    | `/device`      | `p` = Pfad, optional `open` = Dateiname |
| Speicher | `/storage/:id` | `p` = Präfix, optional `open`           |
| Suche    | wie oben       | zusätzlich `q` und Filter `t`, `s`, `d` |

Freigabe-URLs werden nie in der Sitzung gespeichert; beim Wiederherstellen öffnet `files_open` neu.
