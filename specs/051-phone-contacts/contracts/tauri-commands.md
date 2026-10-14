# Tauri-Commands: Adressbuch

Alle Commands in `src-tauri/src/contacts/commands/*.rs`, Typen über `ts-rs` nach
`src/types/bindings/`. Der Aufrufer ist immer `Caller::User` (eigene Oberfläche, FR-028); Commands für
den eingebauten Agenten setzen `Caller::BuiltinAgent` im Rumpf, nie als Argument. Fehler als
`HolziError` mit `kind`.

## Kontakte

| Command                  | Eingabe                                             | Ausgabe           |
| ------------------------ | --------------------------------------------------- | ----------------- |
| `contacts_list`          | –                                                   | `ContactHeader[]` |
| `contacts_get`           | `{ id }`                                            | `Contact`         |
| `contacts_create`        | `ContactInput` + `targets: TargetInput[]` (FR-018c) | `Contact`         |
| `contacts_update`        | `{ id, patch: ContactPatch }`                       | `Contact`         |
| `contacts_delete`        | `{ ids }`                                           | `()`              |
| `contacts_set_photo`     | `{ id, bytes }` (wird auf R10-Grenzen verkleinert)  | `Contact`         |
| `contacts_photo`         | `{ hash }`                                          | Bytes (JPEG)      |
| `contacts_groups`        | –                                                   | `Group[]`         |
| `contacts_group_save`    | `{ id?, name }`                                     | `Group`           |
| `contacts_group_members` | `{ group_id, add: id[], remove: id[] }`             | `()`              |

`ContactHeader`: `id`, `display_name`, `organization`, `phones[]`, `emails[]`, `groups[]`, `photo?`,
`origin`, `targets[]`, `has_card`, `issue?`.

`ContactPatch` enthält nur Geänderte: Kopf-Felder als `Option<Option<T>>`; Angaben als
`values: { upsert: ValueInput[], remove: id[] }`. Der Dienst lehnt `upsert`/`remove` für Werte mit
unbekannter `kind` ab (`UnknownKindReadOnly`) und ändert die Herkunft nie (FR-018a).

## Abgleichsziele

| Command                   | Eingabe                                                                   | Ausgabe                                                        |
| ------------------------- | ------------------------------------------------------------------------- | -------------------------------------------------------------- |
| `contacts_phones`         | –                                                                         | `PhoneSummary[]` (Telefone mit Abgleich an, Konten, Zielkonto) |
| `contacts_targets_set`    | `{ contact_ids, device, account? }` oder `{ group_id, device, account? }` | `()`                                                           |
| `contacts_targets_remove` | `{ contact_ids, device, on_phone: "delete" \| "keep" }` (FR-018d)         | `()`                                                           |

## Abgleich mit dem Telefon (nur mit Fähigkeit `phoneContacts`)

| Command                      | Eingabe                                    | Ausgabe                                                                              |
| ---------------------------- | ------------------------------------------ | ------------------------------------------------------------------------------------ |
| `contacts_phone_enable`      | –                                          | `PhoneEnableResult` (`denied` \| `review`)                                           |
| `contacts_phone_disable`     | –                                          | `()` (FR-026: löscht nichts)                                                         |
| `contacts_phone_accounts`    | –                                          | `PhoneAccount[]` (`writable`)                                                        |
| `contacts_phone_set_target`  | `{ account }`                              | `()`                                                                                 |
| `contacts_phone_review`      | –                                          | `ReviewPlan` (neu ins Adressbuch, neu ins Telefon, zusammengeführt, unsichere Paare) |
| `contacts_phone_confirm`     | `{ pair_decisions: { pair_id, merge }[] }` | `RunSummary`                                                                         |
| `contacts_phone_status`      | –                                          | `PhoneStatus` (Zustand, letzter Lauf, Zahlen, Issues)                                |
| `contacts_phone_mass_delete` | `{ decision: "delete" \| "keep" }`         | `RunSummary`                                                                         |
| `contacts_phone_run`         | –                                          | `RunSummary` (von Hand anstoßen)                                                     |

Ereignisse: `contacts-phone-status` (Zustand geändert), `contacts-phone-mass-delete` (Lauf angehalten,
mit Zahlen), Datenänderungen über das vorhandene `vault-data-changed`.

Fehler: `ContactsDenied` (Android-Berechtigung fehlt, FR-017), `PhoneContactsUnavailable` (FR-015
Desktop), `PhoneAccountNotWritable`.

## Visitenkarten

| Command                               | Eingabe                                   | Ausgabe                                                                                                                |
| ------------------------------------- | ----------------------------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| `contacts_own_card`                   | –                                         | `{ text, qr_svg, issued_at, can_sign, omitted[] }`                                                                     |
| `contacts_own_card_set`               | `{ own_contact?, value_ids }`             | `()` (Hauptgerät erstellt neu)                                                                                         |
| `contacts_card_preview`               | `{ text }`                                | `CardPreview` (Name, npub, Geräte, Angaben, `existing_contact?`) oder Fehler `CardInvalid` \| `CardOwn` \| `CardOlder` |
| `contacts_card_accept`                | `{ text, into: "new" \| { contact_id } }` | `Contact`                                                                                                              |
| `contacts_blocked`                    | –                                         | `BlockedVault[]`                                                                                                       |
| `contacts_block` / `contacts_unblock` | `{ vault_pubkey }`                        | `()`                                                                                                                   |

## Eingebauter Agent

| Command                 | Eingabe             | Ausgabe           |
| ----------------------- | ------------------- | ----------------- |
| `contacts_agent_search` | `{ query, limit? }` | `ContactHeader[]` |
| `contacts_agent_get`    | `{ id }`            | `Contact`         |
| `contacts_agent_update` | `{ id, patch }`     | `Contact`         |

Ohne gemerkte Freigabe parkt der Command auf der Rückfrage (Ereignis
`files-agent-permission-request` mit `kind: "contacts"`, Antwort über
`files_agent_permission_answer`, R15). Ablehnung → `NotFound`-gleicher Fehler (FR-031).
