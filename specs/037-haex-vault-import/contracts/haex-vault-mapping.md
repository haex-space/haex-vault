# Vertrag: Abbildung haex-vault 2.x (Legacy) → Importmodell

Quelle: haex-vault 2.x @ `8dce379d94e18fcd42c3b73686a06f984ca3f574`. Ergänzt
[034 contracts/import-mapping.md](../../034-password-manager/contracts/import-mapping.md) um die
Quelle `legacyhaexvault`. Alle Abfragen nennen Spalten beim Namen.

## Einträge (`haex_passwords_item_details` → `ImportItem`)

| Quelle                                         | Ziel               | Regel                                                                       |
| ---------------------------------------------- | ------------------ | --------------------------------------------------------------------------- |
| `title`, `username`, `password`, `url`, `note` | gleichnamig        | unverändert; leerer Text bleibt leer                                        |
| `icon`                                         | `icon`             | siehe „Symbole“                                                             |
| `color`                                        | `color`            | unverändert                                                                 |
| `otp_secret`                                   | `otp_raw`          | unverändert; `check_otp` prüft, ungültig → `TotpInvalid`, Wert bleibt       |
| `otp_digits`, `otp_period`, `otp_algorithm`    | gleichnamig        | NULL bleibt NULL (Standardwerte setzt der Schreiber)                        |
| `expires_at`                                   | `expires_at`       | `YYYY-MM-DD` unverändert; anderes Format über `holzi_time`, dann Datumsteil |
| `autofill_aliases`                             | `autofill_aliases` | JSON-Text parsen; kein Objekt → NULL und `UnknownSourceData`                |
| `created_at`, `updated_at`                     | gleichnamig        | `holzi_time` (versteht `YYYY-MM-DD HH:MM:SS` und ISO)                       |
| `id`                                           | —                  | nur zum Verknüpfen innerhalb des Lesens; holzi vergibt neue Kennungen       |

## Eigene Felder (`haex_passwords_item_key_values`)

`ORDER BY rowid` je `item_id` → `key_values` (`KeyValueInput { key, value }`); `key` NULL → leerer
Text.

## Ordner (`haex_passwords_groups`, `haex_passwords_group_items`)

- Jede Zeile → `ImportGroup { reference: id, parent_ref: parent_id, name, description, icon,
color, sort_order }`; `name` NULL → leerer Text.
- Zeile `id = 'trash'` → `is_recycle_bin: true`.
- `parent_id` zeigt auf keine Zeile oder bildet einen Kreis → `parent_ref: None`,
  `GroupReparented`.
- `group_items.group_id` → `ImportItem.group_ref`; keine Zeile oder NULL → oben.
- `trashed` = Ordnerkette des Eintrags erreicht `trash`; `trashed_from_ref`,
  `previous_parent_ref` bleiben `None`.

## Tags (`haex_passwords_tags`, `haex_passwords_item_tags`)

- Namen je Eintrag → `ImportItem.tags` (über `push_tag`).
- Farbe nicht NULL → `ImportModel.tag_colors`.
- Zwei Namen derselben Faltung → ein Tag, `TagMerged` in `source_problems`.

## Binärdaten (`haex_passwords_binaries`)

- `data` Base64 (Standard, mit Padding) → Bytes. Dekodieren scheitert → `AttachmentUnreadable`.
- Anhänge: `item_binaries` (`binary_hash`, `file_name`) → `ImportAttachment::within_limit`
  (über 25 MiB → `AttachmentTooLarge`); Hash ohne Zeile → `AttachmentUnreadable`.
- Die Binärzeile wird je Hash einmal dekodiert und für Anhänge, Verlauf und Bilder geteilt.

## Verlauf (`haex_passwords_item_snapshots`, `haex_passwords_snapshot_binaries`)

- `snapshot_data` → `SnapshotData` (camelCase, fehlende Felder → Standard, unbekannte ignoriert).
  Nicht lesbar → Stand entfällt, `HistoryUnreadable`.
- `ImportState { modified_at: modified_at ?? created_at (holzi_time), data, attachments }`.
- `attachments` aus `snapshot_binaries` des Stands (Regeln wie Anhänge); die Liste in der JSON wird
  nicht benutzt.
- Reihenfolge: nach `modified_at ?? created_at` aufsteigend.

## Passkeys (`haex_passwords_passkeys` → `PasskeyInput`)

Alle Spalten 1:1: `credential_id`, `relying_party_id`, `relying_party_name`, `user_handle`,
`user_name`, `user_display_name`, `private_key`, `public_key`, `algorithm`, `sign_count`,
`is_discoverable` (0/1 → bool), `icon` (Regel „Symbole“, nur Namen), `color`, `nickname`,
`created_at`, `last_used_at` (beide `holzi_time`). Base64-Werte bleiben Text wie in der Quelle.
`item_id` gesetzt → an `ImportItem.passkeys` des Eintrags; NULL oder unbekannt →
`ImportModel.passkeys`.

## Generator-Voreinstellungen (`haex_passwords_generator_presets` → `PresetInput`)

`name`, `length`, `uppercase`, `lowercase`, `numbers`, `symbols`, `exclude_chars`, `use_pattern`,
`pattern`, `is_default` 1:1; `id` leer (neu anlegen). Ungültig nach `presets::save`
(Name leer, Länge außerhalb) → übersprungen, `ValueNotStorable`.

## Symbole

| Wert der Quelle                               | Ziel                                                                           |
| --------------------------------------------- | ------------------------------------------------------------------------------ |
| NULL                                          | kein Symbol (holzi zeigt sein Standardsymbol)                                  |
| `binary:<hash>`, Zeile vorhanden              | `IconRef::Custom(bytes)`                                                       |
| `binary:<hash>`, Zeile fehlt                  | kein Symbol, `IconNotMapped`                                                   |
| `i-lucide-<name>`                             | `IconRef::Standard("lucide:<name>")`, wenn in der Tabelle; sonst wie unbekannt |
| `lucide:<name>`                               | unverändert, wenn in der Tabelle                                               |
| `mdi:<name>`, Kurznamen der alten Erweiterung | Tabellen-Eintrag → `lucide:…`                                                  |
| sonst                                         | kein Symbol, `IconNotMapped`                                                   |

Die Tabelle steht in `src-tauri/src/passwords/import/legacy_haex_vault/icons.rs`. Jedes Ziel steht in
`IMPORT_ICONS` (`src/lib/passwords/icons.ts`); ein Test prüft das.

## Ignoriert (FR-015)

`haex_hlc`, `haex_column_hlcs`, `haex_column_sigs` in jeder Tabelle; alle Tabellen außerhalb
`haex_passwords_*` (`haex_deleted_rows`, `haex_crdt_*`, Erweiterungen, Einstellungen).
