# Datenmodell: Passwortmanager

**Spec**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md) | **Research**: [research.md](./research.md)

Alle Tabellen entstehen in Migration `0022_passwords` (SQL in
`src-tauri/src/identity/migrations_passwords.rs`), sind CRDT-synchronisiert (kein
`_no_sync`-Suffix) und gehören der ganzen Vault, nicht einem Gerät (ADR-0001: kein
`vault_device_uuid`). haex-crdt ergänzt Spalten für HLC und Spaltensignaturen selbst; sie
stehen nicht im SQL. Tabellen- und Spaltennamen sind die von haex-vault 2.x (Legacy)
(`src/database/schemas/passwords.ts`, Stand `8dce379`). Alle Kennungen sind `TEXT`,
Zeitstempel `TEXT` (`CURRENT_TIMESTAMP`, wo nicht anders gesagt).

**Abweichungen von haex-vault 2.x** (alle Gründe in research.md):

| Nr. | Abweichung                                                                                                   | Grund |
| --- | ------------------------------------------------------------------------------------------------------------ | ----- |
| A1  | `binaries.data` ist `BLOB` statt Base64-Text                                                                 | R4    |
| A2  | `group_items.trashed_from_group_id` und `groups.trashed_from_parent_id` (neu, nullbar, ohne FK)              | R3    |
| A3  | kein UNIQUE auf `tags.name`, `passkeys.credential_id` und `item_tags (item_id, tag_id)`; stattdessen Indizes | R2    |
| A4  | `item_binaries.binary_hash` und `snapshot_binaries.binary_hash`: `ON DELETE RESTRICT` statt `CASCADE`        | R4    |
| A5  | zusätzliche Indizes auf Fremdschlüsselspalten                                                                | R1    |
| A6  | `binaries.orphaned_at` (nullable): Beginn der Karenzzeit nach Verlust des letzten bekannten Verweises        | R4    |

## Tabellen

### `haex_passwords_item_details` — Eintrag

| Spalte             | Typ     | Regel                                                                                                                               |
| ------------------ | ------- | ----------------------------------------------------------------------------------------------------------------------------------- |
| `id`               | TEXT PK | UUIDv4                                                                                                                              |
| `title`            | TEXT    | optional, auch leer; die Oberfläche zeigt den Platzhalter „(ohne Titel)“                                                            |
| `username`         | TEXT    |                                                                                                                                     |
| `password`         | TEXT    | Geheimnis; nie in Listen, Kopfdaten oder Fehlern                                                                                    |
| `note`             | TEXT    |                                                                                                                                     |
| `icon`, `color`    | TEXT    | `icon`: Name aus der festen Liste der Oberfläche oder `binary:<hash>` (eigenes Bild aus dem Import, Binärzeile vom Typ `icon`)      |
| `url`              | TEXT    |                                                                                                                                     |
| `otp_secret`       | TEXT    | Geheimnis; Base32 nach Normalisierung (`passwords/totp.rs`); aus Sync oder Import kann auch Ungültiges stehen (`otpState: invalid`) |
| `otp_digits`       | INTEGER | Standard 6; zulässig 6–10                                                                                                           |
| `otp_period`       | INTEGER | Standard 30; zulässig 1–300                                                                                                         |
| `otp_algorithm`    | TEXT    | Standard `SHA1`; zulässig `SHA1`, `SHA256`, `SHA512`                                                                                |
| `expires_at`       | TEXT    | `YYYY-MM-DD`                                                                                                                        |
| `autofill_aliases` | TEXT    | JSON `{ "username": [..], … }`; vom Passwortmanager nur verwahrt                                                                    |
| `created_at`       | TEXT    | Standard `CURRENT_TIMESTAMP`; Rust schreibt RFC 3339 mit Millisekunden                                                              |
| `updated_at`       | TEXT    | von Rust bei jeder Änderung gesetzt (Konfliktprüfung, R15)                                                                          |

Index: `idx_pw_items_updated (updated_at)`.

### `haex_passwords_item_key_values` — eigenes Feld

`id` PK (UUIDv4) · `item_id` NOT NULL → `item_details(id)` ON DELETE CASCADE · `key` · `value`
(Geheimnis) · `updated_at`. Index: `idx_pw_kv_item (item_id)`.
Regel: Beim Speichern entfallen Felder mit leerem Schlüssel.

### `haex_passwords_groups` — Ordner

`id` PK (UUIDv4; Ausnahme `trash`) · `name` · `description` · `icon` · `sort_order` INTEGER ·
`color` · `parent_id` → `groups(id)` ON DELETE CASCADE · `created_at` · `updated_at` ·
**`trashed_from_parent_id` TEXT (A2)**. Index: `idx_pw_groups_parent (parent_id)`.
Regeln: kein Zyklus (`parent_id` darf nicht der Ordner selbst oder ein Nachfahre sein); der
Papierkorb `id = 'trash'` hat `parent_id NULL` und `name NULL`; Geschwister sortieren nach
`sort_order` (leer = 0), dann nach `name` (Unicode-Reihenfolge ohne Rücksicht auf Groß-/Kleinschreibung).
Das Umsortieren (`reorder_groups`) vergibt den Geschwistern einer Ebene `sort_order` 0, 1, 2, … in der
neuen Reihenfolge, in einem `write`; Ordner ohne eigene Reihenfolge bleiben alphabetisch.

### `haex_passwords_group_items` — Ordnerzuordnung

`item_id` PK → `item_details(id)` ON DELETE CASCADE · `group_id` → `groups(id)` ON DELETE
CASCADE (leer = Wurzel) · **`trashed_from_group_id` TEXT (A2)**. Index:
`idx_pw_group_items_group (group_id)`. Regel: höchstens eine Zeile je Eintrag.

### `haex_passwords_binaries` — Binärdaten

| Spalte        | Typ                    | Regel                                                                                              |
| ------------- | ---------------------- | -------------------------------------------------------------------------------------------------- |
| `hash`        | TEXT PK                | SHA-256 der Rohdaten, kleingeschriebenes Hex                                                       |
| `data`        | **BLOB** NOT NULL (A1) | höchstens 25 MiB                                                                                   |
| `size`        | INTEGER NOT NULL       | Länge von `data`                                                                                   |
| `type`        | TEXT                   | `attachment` (Standard) oder `icon` (eigenes Symbol, vom Import; Verweis über `binary:<hash>`)     |
| `created_at`  | TEXT                   | Zeitpunkt der Anlage; Rust schreibt RFC 3339 mit Millisekunden                                     |
| `orphaned_at` | TEXT                   | nullable; Zeitpunkt, an dem kein Verweis mehr bekannt war; Karenzzeit über `datetime(orphaned_at)` |

`data` wird nur in eigenen Abfragen gelesen. `size`, nicht `length(data)`, trägt die Anzeige.

### `haex_passwords_item_binaries` — Anhang eines Eintrags

`id` PK (UUIDv4) · `item_id` NOT NULL → `item_details(id)` ON DELETE CASCADE ·
`binary_hash` NOT NULL → `binaries(hash)` **ON DELETE RESTRICT (A4)** · `file_name` NOT NULL.
Index: `idx_pw_item_bin_item (item_id)`, `idx_pw_item_bin_hash (binary_hash)`.
Regel: `file_name` ist Text (nie Pfad); Pfadtrenner und Steuerzeichen werden beim Hinzufügen
durch `_` ersetzt, Länge höchstens 255 Zeichen.

### `haex_passwords_item_snapshots` — Verlaufsstand

`id` PK (UUIDv4) · `item_id` NOT NULL → `item_details(id)` ON DELETE CASCADE ·
`snapshot_data` NOT NULL (JSON, [SnapshotData](#snapshotdata)) · `created_at` · `modified_at`
(Zeitpunkt der Änderung, RFC 3339 mit Millisekunden). Index: `idx_pw_snap_item (item_id, modified_at)`.

### `haex_passwords_snapshot_binaries` — Anhang eines Verlaufsstands

`id` PK (UUIDv4) · `snapshot_id` NOT NULL → `item_snapshots(id)` ON DELETE CASCADE ·
`binary_hash` NOT NULL → `binaries(hash)` **ON DELETE RESTRICT (A4)** · `file_name` NOT NULL.
Index: `idx_pw_snap_bin_snap (snapshot_id)`, `idx_pw_snap_bin_hash (binary_hash)`.

### `haex_passwords_generator_presets` — Voreinstellung

`id` PK (UUIDv4) · `name` NOT NULL · `length` INTEGER NOT NULL (16) · `uppercase`,
`lowercase`, `numbers`, `symbols` INTEGER NOT NULL (1) · `exclude_chars` ('') · `use_pattern`
INTEGER NOT NULL (0) · `pattern` ('') · `is_default` INTEGER NOT NULL (0) · `created_at` ·
`updated_at`. Regeln: Name nicht leer; beim Setzen eines Standards werden alle anderen
zurückgesetzt, in demselben `write`; bei mehreren Standards gilt der mit dem jüngsten
`updated_at` (R2). Nichts wird vorbelegt; ohne Voreinstellung gelten die Standardwerte der
Oberfläche (Länge 20, alle vier Klassen).

### `haex_passwords_tags` — Tag

`id` PK = `UUIDv5(NS_TAG, fold(name))` bei Neuanlage (A3) · `name` NOT NULL · `color` ·
`created_at`. Index: `idx_pw_tags_name (name)`. Regeln: `fold` = NFC, Kleinschreibung, getrimmt;
leere Namen und Namen mit mehr als 64 Zeichen sind ungültig; die Anwendung verhindert
Doppelte nach `fold` beim Anlegen und Umbenennen; `reconcile_tags` heilt Wettläufe (R2).
Es gibt keine für holzi reservierten Tagnamen; was eine holzi-Funktion nutzt, meldet sie selbst (`EntryUsage`, FR-034).

### `haex_passwords_item_tags` — Tag-Zuordnung

`id` PK = `UUIDv5(NS_ITEM_TAG, item_id ":" tag_id)` (A3) · `item_id` NOT NULL →
`item_details(id)` ON DELETE CASCADE · `tag_id` NOT NULL → `tags(id)` ON DELETE CASCADE.
Indizes: `idx_pw_item_tags_item (item_id)`, `idx_pw_item_tags_tag (tag_id)`.

### `haex_passwords_passkeys` — Passkey (nur Daten)

| Spalte                                                 | Typ                   | Regel                                                         |
| ------------------------------------------------------ | --------------------- | ------------------------------------------------------------- |
| `id`                                                   | TEXT PK               | `UUIDv5(NS_PASSKEY, credential_id)` (A3)                      |
| `item_id`                                              | TEXT                  | → `item_details(id)` ON DELETE CASCADE; leer = ohne Eintrag   |
| `credential_id`                                        | TEXT NOT NULL         | Base64; kein UNIQUE (A3); Index `idx_pw_passkeys_cred`        |
| `relying_party_id`                                     | TEXT NOT NULL         | Index `idx_pw_passkeys_rp`                                    |
| `relying_party_name`, `user_name`, `user_display_name` | TEXT                  |                                                               |
| `user_handle`                                          | TEXT NOT NULL         |                                                               |
| `private_key`                                          | TEXT NOT NULL         | Geheimnis; PKCS8, Base64                                      |
| `public_key`                                           | TEXT NOT NULL         | SPKI, Base64; `''`, wenn der Import ihn nicht ableiten konnte |
| `algorithm`                                            | INTEGER NOT NULL (-7) | COSE: -7 ES256, -8 EdDSA, -257 RS256                          |
| `sign_count`                                           | INTEGER NOT NULL (0)  |                                                               |
| `is_discoverable`                                      | INTEGER NOT NULL (1)  |                                                               |
| `icon`, `color`, `nickname`                            | TEXT                  | `nickname` ist änderbar                                       |
| `created_at`, `last_used_at`                           | TEXT                  |                                                               |

Regel: Die Oberfläche legt keine Passkeys an; sie entstehen durch Import oder Sync
(später durch die External Bridge). Löschen und Spitznamen sind erlaubt. Beim Import liefern
die Quellen nur den privaten Schlüssel; `public_key` wird aus ihm abgeleitet (`passkeys.rs`:
ES256, EdDSA, RS256), bei einem anderen Algorithmus oder unlesbarem Schlüssel bleibt er leer
(`''`; der Passkey wird trotzdem gespeichert, zum Anmelden genügt der private Schlüssel, der
Bericht nennt ihn). Die Kodierung von `credential_id`,
`user_handle` und der Schlüssel entspricht der von haex-vault 2.x (Prüfung in T003).

## Abgeleitete Kennungen

Namensräume sind feste UUIDs in `passwords/ids.rs` (Konstanten mit Prüfung in `ids_tests.rs`;
sie dürfen nie geändert werden, weil sonst gleiche Tags auf alten und neuen Geräten
verschiedene Kennungen bekämen).

| Namensraum    | Eingabe              | Ergebnis       |
| ------------- | -------------------- | -------------- |
| `NS_TAG`      | `fold(name)`         | `tags.id`      |
| `NS_ITEM_TAG` | `item_id ":" tag_id` | `item_tags.id` |
| `NS_PASSKEY`  | `credential_id`      | `passkeys.id`  |

## SnapshotData

JSON in `item_snapshots.snapshot_data`, Format `version: 1`. Leser tolerieren fehlende Felder
(Stände aus haex-vault 2.x kennen weder `version` noch die OTP-Parameter noch `autofillAliases`).

```text
SnapshotData {
  version: 1,
  title, username, password, url, note, icon, color, expiresAt,    // null-bar
  otpSecret, otpDigits, otpPeriod, otpAlgorithm,                   // null-bar
  autofillAliases: { [field]: string[] } | null,
  tagNames: string[],
  keyValues: { key, value }[],
  attachments: { fileName, binaryHash }[]
}
```

Nicht enthalten: Ordner (Wiederherstellen verschiebt nicht), Passkeys, Zeitstempel (stehen in
der Zeile). Die Liste im Verlauf zeigt je Stand die Namen der gegenüber dem Vorgänger
geänderten Felder; Werte geheimer Felder nur auf `passwords_history_reveal`.

## Zustände und Übergänge

### Eintrag und Ordner

```text
aktiv (in Ordner oder Wurzel)
  ── trash ──▶ im Papierkorb (group_id = 'trash' oder Nachfahre; trashed_from_* gesetzt)
  ◀── restore ──  (zurück an trashed_from_*, sonst Wurzel; trashed_from_* wird NULL)
im Papierkorb
  ── delete_permanently ──▶ entfernt (Kinder zuerst, ein write, je Zeile ein Löschmarker)
```

- `trash` auf ein Element, das schon im Papierkorb liegt, ist `delete_permanently`.
- Ein Ordner im Papierkorb behält seine Kinder und ihre Struktur; nur das direkt gelöschte
  Element trägt eine Herkunft.
- `restore` eines Eintrags aus einem Ordner im Papierkorb, der selbst nicht wiederhergestellt
  wird, legt den Eintrag an die Wurzel.
- `empty_trash` ist `delete_permanently` für alle Kinder des Papierkorbs; die Papierkorb-Zeile
  bleibt.

### Binärdaten

```text
angelegt (created_at = jetzt, orphaned_at = NULL)
  ── Verweis in item_binaries oder snapshot_binaries ──▶ in Benutzung
in Benutzung ── letzter bekannte Verweis entfällt ──▶ verwaist (orphaned_at = jetzt)
verwaist ── neuer Verweis trifft ein ──▶ in Benutzung (orphaned_at = NULL)
verwaist ── beim Öffnen der Vault, orphaned_at mindestens 7 Tage alt ──▶ gelöscht
```

Verweis heißt bei Anhängen eine Zeile in `item_binaries` oder `snapshot_binaries`. Bei `type = 'icon'`
nennt `binary:<hash>` in `item_details.icon`, `groups.icon`, `passkeys.icon` oder im JSON eines
Verlaufsstands den Hash; solange eine Stelle ihn nennt, gilt die Zeile als benutzt, setzt
`orphaned_at` zurück auf `NULL` und wird nicht aufgeräumt (R4). `created_at` beschreibt nur die
Anlage; auch eine alte Binärzeile bekommt beim ersten Erkennen ohne Verweis einen neuen
`orphaned_at`-Zeitpunkt.

## Rust-Typen (nicht gespeichert; ts-rs-Export nach `src/types/bindings/`)

| Typ              | Inhalt                                                                                                                                                                                                                                                                                                     |
| ---------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `ItemHeader`     | id, title, username, url, icon, color, groupId, tags `[{id,name,color}]`, expiresAt, hasPassword, hasTotp, passkeyCount, attachmentCount, createdAt, updatedAt                                                                                                                                             |
| `ItemDetail`     | `ItemHeader` plus note, autofillAliases, otpDigits, otpPeriod, otpAlgorithm, hasOtpSecret, otpState (`none`, `valid`, `invalid`), keyValues `[{id,key,hasValue}]`, attachments `[{id,fileName,size,binaryHash}]`, passkeys `[{id,relyingPartyId,relyingPartyName,userName,nickname,createdAt,lastUsedAt}]` |
| `AgentHeader`    | id, title, tags (Namen), folder (Name), hasTotp                                                                                                                                                                                                                                                            |
| `GroupRow`       | id, name, description, icon, color, sortOrder, parentId, trashedFromParentId                                                                                                                                                                                                                               |
| `TagRow`         | id, name, color, itemCount                                                                                                                                                                                                                                                                                 |
| `SnapshotHeader` | id, itemId, modifiedAt, changedFields `string[]`, attachmentCount                                                                                                                                                                                                                                          |
| `ImportReport`   | imported, trashed, historyStates, skippedDuplicates, needsAttention `[{title, folderPath, kind, field?, fileName?, sizeMiB?}]`                                                                                                                                                                             |

Typen mit Geheimnissen (`PasswordInput`, `RevealedSecret`, `SnapshotData`) implementieren
`Debug` ohne Werte; sie stehen nie in `Display` oder in Fehlerfeldern.
