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
`origin`, `targets[]` (Telefone), `has_card`, `issue?` (ein Grund, wenn eine Änderung ein Telefon nicht
erreichte). `Contact` hat dazu `targets[]` (`device`, `accountType`, `accountName`) und `issues[]`
(`device`, `reason`, `at`) aller Telefone.

`ContactPatch` enthält nur Geänderte: Kopf-Felder als `Option<Option<T>>`; Angaben als
`values: { upsert: ValueInput[], remove: id[] }`. Der Dienst lehnt `upsert`/`remove` für Werte mit
unbekannter `kind` ab (`UnknownKindReadOnly`) und ändert die Herkunft nie (FR-018a).

## Abgleichsziele

| Command                   | Eingabe                                                                 | Ausgabe                                                        |
| ------------------------- | ----------------------------------------------------------------------- | -------------------------------------------------------------- |
| `contacts_phones`         | –                                                                       | `PhoneSummary[]` (Telefone mit Abgleich an, Konten, Zielkonto) |
| `contacts_targets_set`    | `{ contactIds, device, account? }` oder `{ groupId, device, account? }` | `()`                                                           |
| `contacts_targets_remove` | `{ contactIds, device, onPhone: "delete" \| "keep" }` (FR-018d)         | `()`                                                           |

`PhoneSummary`: `device`, `name` (Name des Geräts in der Vault), `target` (Zielkonto), `accounts` (die
Konten, wie der letzte Lauf des Telefons sie sah; Geräte-Einstellung `contacts.phone.accounts`). Nur
der Nutzer setzt Ziele (`Caller::User`); ein Telefon ohne eingeschalteten Abgleich oder ein Konto, das
das Telefon nicht als beschreibbar meldete, antwortet `ContactsInvalid`. Entfernen geht auch für ein
Telefon, dessen Abgleich aus ist. `contacts_create` nimmt `targets` nur vom Nutzer an.

## Abgleich mit dem Telefon (nur mit Fähigkeit `phoneContacts`)

| Command                      | Eingabe                                    | Ausgabe                                                                              |
| ---------------------------- | ------------------------------------------ | ------------------------------------------------------------------------------------ |
| `contacts_phone_enable`      | –                                          | `PhoneEnableResult` (`review`, `otherVault`); Ablehnung → Fehler `ContactsDenied`    |
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
Desktop), `PhoneContactsFailed` (das Telefon-Adressbuch lehnte einen Aufruf ab; `reason` nennt nur die
Art). Die Einstellungen bieten als Zielkonto nur beschreibbare Konten an.

## Visitenkarten

> Spec 060: `contacts_own_card*`, `contacts_card_preview` und `contacts_card_accept` sind durch `contacts_share_*` ersetzt (siehe `specs/060-contact-sharing/contracts/tauri-commands.md`).

| Command                               | Eingabe                                                             | Ausgabe                                                                                                                |
| ------------------------------------- | ------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| `contacts_own_card`                   | –                                                                   | `{ text, qr_svg, issued_at, can_sign, omitted[], own_contact, value_ids }` (ein Hauptgerät erstellt die erste)         |
| `contacts_own_card_set`               | `{ own_contact?, value_ids }`                                       | wie `contacts_own_card` (Hauptgerät erstellt neu)                                                                      |
| `contacts_card_preview`               | `{ text }`                                                          | `CardPreview` (Name, npub, Geräte, Angaben, `existing_contact?`) oder Fehler `CardInvalid` \| `CardOwn` \| `CardOlder` |
| `contacts_card_accept`                | `{ text, into: { kind: "new" } \| { kind: "contact", contactId } }` | `Contact` (die Karte wird erneut geprüft)                                                                              |
| `contacts_blocked`                    | –                                                                   | `BlockedVault[]`                                                                                                       |
| `contacts_block` / `contacts_unblock` | `{ vault_pubkey }`                                                  | `()`                                                                                                                   |

## Eingebauter Agent

| Command                 | Eingabe             | Ausgabe            |
| ----------------------- | ------------------- | ------------------ |
| `contacts_agent_search` | `{ query, limit? }` | `OutsideSummary[]` |
| `contacts_agent_get`    | `{ id }`            | `OutsideContact`   |
| `contacts_agent_update` | `{ id, patch }`     | `OutsideContact`   |

`OutsideSummary` und `OutsideContact` (`contacts/outside.rs`) sind dieselbe Sicht wie
`ContactSummary` und `ContactDetail` der Erweiterungen ([bridge.md](./bridge.md)), mit den Kennungen
der Angaben. `patch` ist `ContactPatch` ohne `groups`; ein Patch mit `groups` wird abgelehnt.

Freigaben sind Zeilen der Art `contacts` in `agent_file_permissions` (Ziel `*` oder `group:<id>`,
Status `read`, `readWrite`, `denied`). Deckt keine Zeile den Aufruf, parkt der Command auf der
Rückfrage (Ereignis `files-agent-permission-request` mit `kind: "contacts"` und `wants`, Antwort über
`files_agent_permission_answer`, R15): „Einmal erlauben“ (`remember: false`) gilt für diesen Aufruf,
„Immer erlauben“ (`remember: true`) und „Ablehnen“ werden für `*` gemerkt; eine abgelehnte
Schreib-Frage lässt eine Lese-Freigabe stehen. Unter einem abgelehnten `*` fragt holzi nicht mehr.
Keine Antwort in 60 s oder kein Fenster → abgelehnt, nichts gemerkt. Ablehnung → `NotFound`-gleicher
Fehler (FR-031). Die Einstellungen (KI › Agenten-Zugriff) setzen und widerrufen die Zeilen über
`settings.agents.files.set` und `settings.agents.files.remove` mit `kind: "contacts"`.
