# Vertrag: Commands (Änderungen gegenüber 034)

Keine neuen Commands. Die Import-Commands aus
[034 contracts/tauri-commands.md](../../034-password-manager/contracts/tauri-commands.md)
(`passwords_import_preview`, `passwords_import_run`, `passwords_import_cancel`,
`passwords_import_report_save`) nehmen die neue Quelle an.

## `ImportSource`

```ts
type ImportSource = 'keepass' | 'bitwarden' | 'lastpass' | 'legacyhaexvault'
```

## `ImportArgs` / `ImportRunArgs` für `legacyhaexvault`

| Feld          | Pflicht   | Bedeutung                                                                                      |
| ------------- | --------- | ---------------------------------------------------------------------------------------------- |
| `source`      | ja        | `"legacyhaexvault"`                                                                            |
| `path`        | ja        | Pfad der Vault-Datei von haex-vault 2.x (Legacy); `<path>-wal` wird mitgelesen, wenn vorhanden |
| `password`    | ja        | Vault-Passwort von haex-vault 2.x; nie gespeichert, nie protokolliert                          |
| `keyFilePath` | nein      | wird ignoriert                                                                                 |
| `onDuplicate` | nur `run` | wie 034                                                                                        |

## `ImportPreview`

```ts
interface ImportPreview {
  entries: number
  groups: number
  trashedEntries: number
  historyStates: number
  attachments: number
  passkeys: number
  duplicates: number
  tags: number // neu
  presets: number // neu
  warnings: string[]
}
```

Für `legacyhaexvault` enthält `warnings` immer `legacy_haex_vault_close_first` und, wenn die Datei Tabellen der
alten Erweiterung haex-pass hat, `haex_pass_tables_ignored`.

## Fehler

`PasswordsImportFailed { reason }` mit den Gründen aus
[data-model.md §6](../data-model.md#6-fehlergründe-passwordsimportfailedreason). Kein Grund
enthält Pfad, Passwort oder Inhalte.

## Oberfläche (`src/components/passwords/ImportWizard.vue`)

- `SOURCES` bekommt `legacyhaexvault` (Bezeichnung „haex-vault 2.x“, Dateifilter `db`).
- Die Passwortzeile erscheint für `keepass` und `legacyhaexvault`; die Schlüsseldatei nur für `keepass`.
- `canPreview` verlangt für `legacyhaexvault` Datei und Passwort.
- Die Vorschau zeigt zusätzlich Tags und Voreinstellungen und für `legacyhaexvault` den Hinweis zum
  Schließen von haex-vault 2.x.
