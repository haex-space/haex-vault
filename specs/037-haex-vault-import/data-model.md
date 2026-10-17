# Data Model: Passwörter aus haex-vault 2.x (Legacy) übernehmen

Diese Spec ändert **kein** Tabellenschema von holzi und braucht keine Migration. Sie ändert nur
das Importmodell im Speicher (`src-tauri/src/passwords/import/mod.rs`) und Werte, die über
die Tauri-Grenze gehen.

## 1. Quelle (nur gelesen)

Tabellen von haex-vault 2.x @ `8dce379d94e1`, `src-tauri/database/migrations/0000_jazzy_chat.sql:414-552`.
Die genaue Abbildung jeder Spalte steht in [contracts/haex-vault-mapping.md](contracts/haex-vault-mapping.md).

| Tabelle                                           | Rolle                              | Pflicht                        |
| ------------------------------------------------- | ---------------------------------- | ------------------------------ |
| `haex_passwords_item_details`                     | Einträge                           | ja (fehlt sie: `no_passwords`) |
| `haex_passwords_item_key_values`                  | eigene Felder (`ORDER BY rowid`)   | ja                             |
| `haex_passwords_groups`                           | Ordner, `trash` als feste Zeile    | ja                             |
| `haex_passwords_group_items`                      | Eintrag → Ordner (höchstens einer) | ja                             |
| `haex_passwords_binaries`                         | Anhänge und Bilder, Base64-Text    | ja                             |
| `haex_passwords_item_binaries`                    | Anhänge eines Eintrags             | ja                             |
| `haex_passwords_item_snapshots`                   | Verlauf (JSON)                     | ja                             |
| `haex_passwords_snapshot_binaries`                | Anhänge eines Verlaufsstands       | ja                             |
| `haex_passwords_tags`, `haex_passwords_item_tags` | Tags                               | ja                             |
| `haex_passwords_passkeys`                         | Passkeys, `item_id` darf NULL sein | ja                             |
| `haex_passwords_generator_presets`                | Generator-Voreinstellungen         | ja                             |

Bekannte Zusatzspalten jeder Tabelle (ignoriert): `haex_hlc`, `haex_column_hlcs`,
`haex_column_sigs`.

## 2. Änderungen am Importmodell

Alle neuen Felder haben einen leeren Standardwert; KeePass, Bitwarden und LastPass füllen sie
nicht und verhalten sich unverändert.

```text
ImportSource      + LegacyHaexVault                        // serde "legacyhaexvault", ts-rs-Binding neu erzeugt

ImportItem        + color: Option<String>            // "#rrggbb" wie in der Quelle
                  + autofill_aliases: Option<serde_json::Value>   // Record<string, string[]>

ImportGroup       + color: Option<String>
                  + sort_order: Option<i64>

ImportModel       + tag_colors: Vec<(String, String)> // (Tag-Name, Farbe)
                  + passkeys: Vec<PasskeyInput>      // ohne Eintrag, item_id = None
                  + presets: Vec<PresetInput>        // id leer → neu
```

`ImportItem.passkeys` (schon `Vec<PasskeyInput>`) trägt beim neuen Leser alle Angaben der Quelle,
einschließlich `public_key`, `icon`, `color`, `nickname`, `sign_count`, `last_used_at`.

## 3. Änderungen in `apply.rs`

| Stelle                 | Neu                                                                                                                                                            |
| ---------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `write_item`           | schreibt `color` und `autofill_aliases` (JSON-Text) statt NULL                                                                                                 |
| `write_groups`         | schreibt `color`, `sort_order`; nimmt einen vorhandenen Ordner mit gleichem Namen unter demselben aufgelösten Elternordner, statt neu anzulegen (alle Quellen) |
| neu nach den Einträgen | `tag_colors`: `tags::set_color`, nur wenn das Tag in holzi noch keine Farbe hat                                                                                |
| neu nach den Einträgen | `model.passkeys` über `passkeys::insert` mit `item_id = None`; `Duplicate` → `PasskeyDuplicate`                                                                |
| neu am Ende            | `model.presets` über `presets::save`; Name vorhanden → übersprungen; `is_default` nur, wenn holzi keine Standard-Voreinstellung hat                            |
| `Ledger`               | `+ passkeys: Vec<String>`, `+ presets: Vec<String>`; nur neu angelegte Ordner stehen in `groups`                                                               |
| `rollback`             | löscht zusätzlich die Passkeys und Voreinstellungen aus dem Ledger                                                                                             |

## 4. Vorschau (`ImportPreview`, `model.rs:222`)

```text
ImportPreview     + tags: u32
                  + presets: u32
```

`passkeys` zählt künftig Passkeys an Einträgen **und** ohne Eintrag. `warnings` bekommt für
diese Quelle den festen Hinweis zum Schließen von haex-vault 2.x (Schlüssel
`legacy_haex_vault_close_first`) und, falls vorhanden, den Hinweis auf Tabellen der alten Erweiterung
(`haex_pass_tables_ignored`).

## 5. Bericht (`AttentionKind`, `model.rs:177-187`)

Neue Arten, je mit Text in `src/i18n/locales/{de,en}.json`:

| Art                 | Wann                                                         | Felder             |
| ------------------- | ------------------------------------------------------------ | ------------------ |
| `HistoryUnreadable` | `snapshot_data` ist kein lesbarer Stand                      | Eintrag, Zeitpunkt |
| `GroupReparented`   | Elternordner fehlt oder Kreis                                | Ordner             |
| `TagMerged`         | zwei Tags der Quelle fallen beim Falten zusammen             | Tag-Namen          |
| `UnknownSourceData` | unbekannte Spalte oder Tabelle im Passwortmanager der Quelle | Tabelle, Spalte    |

Weiterverwendet: `IconNotMapped` (unbekannter Name, Bild fehlt), `AttachmentUnreadable`
(Binärdaten fehlen oder Base64 kaputt), `AttachmentTooLarge`, `TotpInvalid`, `PasskeyDuplicate`.

## 6. Fehlergründe (`PasswordsImportFailed.reason`)

Neu: `legacy_haex_vault_locked`, `no_passwords`. Weiterverwendet: `unreadable`, `unsupported_format`,
`cancelled`. Texte unter `errors.passwords.importReason` in `de.json`/`en.json`.

## 7. Zustände

Kein neuer Zustand. Ablauf wie 034: Datei wählen → Vorschau (liest, schreibt nichts) → Import
(schreibt mit Ledger) → Bericht | Abbruch/Fehler → Rückbau. Das temporäre Verzeichnis mit der
Kopie existiert nur während eines `read_model`-Aufrufs, also einmal für die Vorschau und einmal
für den Import.
