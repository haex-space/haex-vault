# Vertrag: Commands (Rust ↔ Frontend)

**Spec**: [../spec.md](../spec.md) | **Datenmodell**: [../data-model.md](../data-model.md) | **Zugriff**: [access.md](./access.md)

Alle Nutzlasten camelCase, Argumente in einem Objekt `args` (Muster der vorhandenen
Commands), Antworttypen als benannte Strukturen (ts-rs nach `src/types/bindings/`).
„Command“ meint hier ausschließlich Tauri-Commands (Begriff aus `CONTEXT.md`). Alle laufen
durch das Tresor-Gate (`vault_gate`) und sind nur bei offener Sitzung aufrufbar; ein neuer
Command ist dort standardmäßig gesperrt, sobald die Vault schließt. Der Aufrufer ist bei jedem
Command der Eingang selbst ([access.md](./access.md)), nie ein Argument.

**Geheimnisse** sind: Passwort, TOTP-Secret, Passkey-Schlüssel, Werte eigener Felder. Kein
Command außer `passwords_reveal`, `passwords_history_reveal` und `passwords_get_item` (nur die
Werte eigener Felder, FR-005) gibt ein Geheimnis an den Webview; `passwords_copy_field`,
`passwords_history_copy` und `passwords_totp_code` geben nur Wirkung beziehungsweise Code zurück.

## Lesen

### `passwords_load_overview`

```text
args:   {}
result: { headers: ItemHeader[], groups: GroupRow[], tags: TagRow[] }
```

Eine Abfrage für den Store. Enthält nie `data`-Spalten, Notizen oder Geheimnisse; Einträge im
Papierkorb sind enthalten (`groupId` ist `trash` oder ein Nachfahre), die Oberfläche trennt.
Dieser Command ist ausschließlich für `Caller::User`; er ist die Nutzer-Übersicht und nicht
die berechtigungsgefilterte `list_headers`-Methode für Aufrufer von außen.

### `passwords_get_item`

```text
args:   { itemId }
result: ItemDetail            // ohne Geheimnisse; hasPassword, hasOtpSecret, keyValues[].hasValue;
                              // keyValues[].value gespeichert, Platzhalter unaufgelöst (FR-005)
errors: NotFound
```

### `passwords_reveal`

```text
args:   { itemId, field: { kind: 'password' | 'otpSecret' | 'username' | 'url' | 'note'
                                | 'storedPassword' } | { kind: 'keyValue', id } }
result: { value: string }
errors: NotFound
```

`storedPassword` liefert das Passwort wie gespeichert, mit unaufgelösten Platzhaltern, für das
Eingabefeld des Editors; alle anderen lösen Platzhalter auf.

Wird nur auf bewusste Handlung des Nutzers aufgerufen; der Wert wird nicht gespeichert, nicht
in Ereignisse oder Protokolle geschrieben. Ausnahme: `storedPassword` holt der Editor beim
Öffnen, weil er das Passwort als normales Eingabefeld zeigt (Änderung 2026-10-05, PR #287).

### `passwords_totp_code`

```text
args:   { itemId }
result: { code: string, remainingSeconds: number, period: number, digits: number }
errors: NotFound, InvalidInput { field: 'otpSecret' | 'otpDigits' | 'otpPeriod' | 'otpAlgorithm' }
```

Rechnet mit der Systemzeit des Backends. Das Frontend fragt neu, wenn `remainingSeconds`
abgelaufen ist. Ein ungültig gespeichertes Secret (`ItemDetail.otpState == invalid`, etwa nach
Sync) liefert `InvalidInput`; die Oberfläche zeigt dann eine Meldung mit „Ersetzen“ und
„Entfernen“. Fehlende Ziffernzahl, Periode und Algorithmus gelten als 6, 30 und `SHA1`.

### `passwords_copy_field`

```text
args:   { itemId, field: 'username' | 'password' | 'totp' | 'url' | 'note' | { kind: 'keyValue', id } }
result: { clearsInSeconds: number | null }
errors: NotFound, InvalidInput
```

Schreibt in Rust in die Zwischenablage und plant das Löschen (R9). `null` bei „Aus“.

### `passwords_copy_text`

```text
args:   { text, itemId?, field?: { kind: 'username' | 'password' | 'url' | 'note' }
                               | { kind: 'keyValue', key } }
result: { clearsInSeconds: number | null }
errors: Reference { reason }, Forbidden
```

Kopiert einen Text, den das Fenster schon hält (ein Wert im Editor, ein offenes Feld eines
Verlaufsstands), mit demselben Leeren wie jede andere Kopie. Mit `field` löst Rust die
Platzhalter vorher auf, wie in diesem Feld des Eintrags `itemId` (keiner bei einem neuen
Eintrag); ein Platzhalter, der nicht auflöst, ist der Fehler und wird nie als Text kopiert
(036 FR-045). Ohne `field` (Titel, Datum, Tags) wird der Text kopiert, wie er ist.

## Schreiben: Einträge

### `passwords_create_item`

```text
args:   { input: ItemInput, groupId?: string }
result: { itemId }
```

`ItemInput`: `title` (optional, auch leer; die Oberfläche zeigt dann „(ohne Titel)“), `username`, `password`, `note`, `url`, `icon`, `color`,
`expiresAt`, `otpSecret` (Secret oder `otpauth://`-Adresse; wird normalisiert und geprüft),
`otpDigits`, `otpPeriod`, `otpAlgorithm`, `autofillAliases`, `tags: string[]` (Namen),
`keyValues: { key, value }[]`. Legt Eintrag, Tags, Felder, Zuordnung und den ersten Verlaufsstand
in **einem** `write` an.

### `passwords_update_item`

```text
args:   { itemId, expectedUpdatedAt, patch: ItemPatch }
result: { updatedAt }
errors: NotFound, Conflict { reason: 'changed' | 'deleted' }, InvalidInput { field }
```

`ItemPatch`: jedes Feld optional; **fehlt** es, bleibt es unverändert; Geheimnisse
(`password`, `otpSecret`, Werte eigener Felder) werden nur ersetzt, wenn sie gesendet
werden, und mit `null` geleert. `tags` (Namen) und `keyValues` (`{ id?, key, value? }`; ohne
`value` bleibt der Wert eines vorhandenen Feldes) ersetzen die Menge. Ein Verlaufsstand
entsteht, wenn sich etwas geändert hat.

## Schreiben: Ordnung

### `passwords_create_group` / `passwords_update_group`

```text
create args: { name, description?, icon?, color?, parentId? }   result: { groupId }
update args: { groupId, patch: { name?, description?, icon?, color?, sortOrder? } }
errors:      InvalidInput, NotFound
```

### `passwords_reorder_groups`

```text
args:   { parentId: string | null, orderedIds: string[] }    // alle Geschwister einer Ebene
result: ()
errors: InvalidInput { reason: 'not_siblings' | 'in_trash' }, NotFound
```

Setzt `sort_order` der Geschwister auf 0, 1, 2, … in dieser Reihenfolge, atomar. Die
Oberfläche bietet es per Ziehen und per „nach oben“ / „nach unten“ an.

### `passwords_move`

```text
args:   { targets: Target[], toGroupId: string | null }       // Target = { kind: 'item' | 'group', id }
result: { moved: number }
errors: InvalidInput { reason: 'cycle' | 'target_in_trash' | 'target_missing' }
```

Verschieben ist atomar für alle Ziele. Ein Ordner darf nicht in sich selbst oder einen
Nachfahren, und nichts in den Papierkorb (dafür `passwords_trash`).

### `passwords_set_tags`

```text
args:   { itemIds: string[], add?: string[], remove?: string[] }    // Tag-Namen
result: { changed: number }
```

Legt fehlende Tags an (`fold`, R2); für Mehrfachauswahl (FR-012).

### `passwords_rename_tag` / `passwords_set_tag_color` / `passwords_delete_tag`

```text
rename args: { tagId, name }     errors: InvalidInput { reason: 'exists' | 'empty' | 'too_long' }
color  args: { tagId, color }
delete args: { tagId }           // löscht Verknüpfungen zuerst
```

## Papierkorb

### `passwords_item_usage`

```text
args:   { itemId }
result: { features: string[] }          // Namen der holzi-Funktionen, die den Eintrag nutzen
```

Fragt alle angemeldeten `EntryUsage`-Anbieter ([access.md](./access.md)) und dient der Warnung vor dem
Löschen (FR-034). In dieser Spec ist kein Anbieter angemeldet (Spec 038 meldet den ersten).

### `passwords_trash` / `passwords_restore` / `passwords_delete_permanently` / `passwords_empty_trash`

```text
args:   { targets: Target[] }          // empty_trash: {}
result: { affected: number }
```

`trash` verschiebt in den Papierkorb und merkt den Ort (Datenmodell, Zustände); bei einem
Ziel, das schon im Papierkorb liegt, wirkt es wie `delete_permanently`. `restore` stellt den
Ort her oder legt an die Wurzel. `delete_permanently` löscht Kinder zuerst und ist atomar. Die
Zahl `affected` zählt Einträge und Ordner einzeln.

## Anhänge

### `passwords_attachment_add`

```text
args:   { itemId, path: string }
result: { attachmentId, fileName, size, binaryHash }
errors: NotFound, AttachmentTooLarge { bytes, limit }, InvalidInput { reason: 'unreadable' | 'empty' }
```

Prüft `metadata().len()` vor dem Lesen; legt Binärzeile (falls neu), Verweis und Verlaufsstand an.
Der Dateiname ist der Basisname von `path`, bereinigt (Datenmodell).

### `passwords_attachment_rename` / `passwords_attachment_remove`

```text
rename args: { attachmentId, fileName }
remove args: { attachmentId }                  // löscht nur den Verweis (R4)
```

### `passwords_attachment_save`

```text
args:   { attachmentId, path: string }
result: ()
errors: NotFound, InvalidInput { reason: 'unwritable' }
```

Schreibt byteweise; überschreibt eine vorhandene Datei nur, wenn der Speichern-Dialog sie
gewählt hat (Pfad stammt aus dem Dialog).

### `passwords_attachment_preview`

```text
args:   { attachmentId }
result: raw bytes (tauri::ipc::Response), Header: Content-Type nach Erweiterung
errors: NotFound, InvalidInput { reason: 'not_previewable' }       // nur png, jpg/jpeg, gif, webp
```

## Verlauf

```text
passwords_history_list     args: { itemId }                result: SnapshotHeader[]  (neueste zuerst)
passwords_history_get      args: { snapshotId }            result: SnapshotView      (wie ItemDetail, ohne Geheimnisse)
passwords_history_reveal   args: { snapshotId, field }     result: { value }
passwords_history_copy     args: { snapshotId, field }     result: { clearsInSeconds }  (Wert bleibt in Rust)
passwords_history_restore  args: { itemId, snapshotId, expectedUpdatedAt }
                           result: { updatedAt, skippedAttachments: string[] }
                           errors: NotFound, Conflict
```

`field` wie bei `passwords_reveal` (`password`, `otpSecret`, `keyValue` mit Schlüsselname).

## Passkeys (nur Daten)

```text
passwords_passkey_rename   args: { passkeyId, nickname }
passwords_passkey_delete   args: { passkeyId }
```

Die Liste je Eintrag steckt in `ItemDetail.passkeys` (ohne Schlüssel).

## Generator-Voreinstellungen

```text
passwords_preset_list     args: {}                          result: Preset[]
passwords_preset_save     args: { preset: PresetInput }     result: { presetId }   // id leer = neu; isDefault setzt die anderen zurück
passwords_preset_delete   args: { presetId }
```

Das Erzeugen selbst läuft im Frontend (research R10).

## Import

Alles aus der Quelle wird übernommen; die Zuordnung je Format steht in
[import-mapping.md](./import-mapping.md).

```text
passwords_import_preview  args: ImportArgs   result: ImportPreview
passwords_import_run      args: ImportArgs & { onDuplicate: 'skip' | 'create' }   result: ImportReport
passwords_import_cancel   args: {}           result: ()
passwords_import_report_save   args: { report: ImportReport, path: string }       result: ()
passwords_icon_preview    args: { hash: string }                                  result: raw bytes

ImportArgs    = { source: 'keepass' | 'bitwarden' | 'lastpass', path: string,
                  password?: string, keyFilePath?: string }
ImportPreview = { entries, groups, trashedEntries, historyStates, attachments, passkeys,
                  duplicates, warnings: string[] }
ImportReport  = { imported, trashed, historyStates, skippedDuplicates,
                  needsAttention: [{ title, folderPath, kind, field?, fileName?, sizeMiB? }] }
errors:       ImportFailed { reason: 'unreadable' | 'wrong_credentials' | 'corrupt' | 'unsupported_format' | 'encrypted_export' }
event:        passwords-import-progress { done, total, phase: 'groups' | 'items' | 'attachments' | 'rollback' }
```

Beide Commands lesen die Datei neu. `run` schreibt in Schritten (Ordner, dann jeder Eintrag in einem
eigenen `write`, dann jeder Anhang in einem eigenen `write`, [R12](../research.md)); ein
Gesamtfehler oder `passwords_import_cancel` entfernt wieder, was der Import angelegt hat
(Phase `rollback`). Fehler an einzelnen Stellen stoppen den Import nicht, sie stehen in
`needsAttention` (Arten in [import-mapping.md](./import-mapping.md) §Bericht). Weder Fehler noch
Bericht tragen Passwort, Pfadinhalt oder Werte von Geheimnissen. `passwords_import_report_save`
schreibt den Bericht als Text in einen Pfad aus dem Speichern-Dialog; `passwords_icon_preview` liefert
die Bytes eines eigenen Symbols (`binary:<hash>` in `icon`).

## Agent

### `passwords_agent_search`

```text
args:   { query?: string, tag?: string, limit?: number }       // limit höchstens 50
result: { items: AgentHeader[] }
```

Wird nur von der Aktion `passwords.items.search` aufgerufen und läuft fest mit
`Caller::BuiltinAgent`: keine Geheimnisse, kein Benutzername, keine Adresse
([access.md](./access.md)). Einträge im Papierkorb erscheinen nicht.

## Einstellung

Die Wartezeit der Zwischenablage ist die Vault-Einstellung `passwords.clipboard_clear_seconds`
(0 = aus, 15, 30, 60, 120; Standard 30) über die vorhandenen Einstellungs-Commands
(`preferences_commands.rs`); sie speichert bei Auswahl.

## Fehlerarten

Neue Varianten in `HolziError` (`src-tauri/src/error.rs`, Feld `kind`), Texte übersetzt das
Frontend (`errors.passwords.*`); kein Wert eines Geheimnisses in einem Feld. In den Abschnitten
oben stehen die Fehler gekürzt (`NotFound`, `Conflict`, `AttachmentTooLarge`, `ImportFailed`); im Feld `kind` tragen sie das Präfix `Passwords` wie in dieser Tabelle,
`InvalidInput` ist die vorhandene Variante ohne Präfix:

| `kind`                        | Felder           |
| ----------------------------- | ---------------- |
| `PasswordsNotFound`           | —                |
| `PasswordsForbidden`          | —                |
| `PasswordsConflict`           | `reason`         |
| `PasswordsAttachmentTooLarge` | `bytes`, `limit` |
| `PasswordsImportFailed`       | `reason`         |

Ungültige Eingaben sind das vorhandene `InvalidInput { reason }`; das Feld nennt den Namen,
nie den Wert.
