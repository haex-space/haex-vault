# Data Model: Adressbuch

Alle Tabellen sind synchronisiert (haex-crdt, LWW je Spalte), außer wo `_no_sync` steht. Regeln aus
`identity/migrations_passwords.rs`: Primärschlüssel Pflicht, kein `WITHOUT ROWID`, kein UNIQUE
(Eindeutigkeit über abgeleitete UUIDv5-IDs), `ON DELETE CASCADE` erlaubt. Migration `0033_contacts` in
`src-tauri/src/identity/migrations_contacts.rs`; `HOLZI_TRIGGER_VERSION` +1.

## haex_contacts

Ein Kontakt (FR-001, FR-003, FR-018a).

| Spalte           | Typ       | Regeln                                                                  |
| ---------------- | --------- | ----------------------------------------------------------------------- |
| `id`             | TEXT PK   | UUIDv4                                                                  |
| `display_name`   | TEXT NULL | leer → aus Namensteilen abgeleitet                                      |
| `given_name`     | TEXT NULL |                                                                         |
| `family_name`    | TEXT NULL |                                                                         |
| `middle_name`    | TEXT NULL |                                                                         |
| `name_prefix`    | TEXT NULL |                                                                         |
| `name_suffix`    | TEXT NULL |                                                                         |
| `nickname`       | TEXT NULL |                                                                         |
| `organization`   | TEXT NULL |                                                                         |
| `job_title`      | TEXT NULL |                                                                         |
| `note`           | TEXT NULL | höchstens 64 KiB                                                        |
| `photo`          | TEXT NULL | SHA-256 hex → `haex_contacts_binaries.hash`                             |
| `origin_kind`    | TEXT      | `holzi` \| `phone` \| `card`; nach dem Anlegen unveränderlich           |
| `origin_device`  | TEXT      | `vault_device_uuid` des anlegenden Geräts                               |
| `origin_caller`  | TEXT      | `user` \| `builtin_agent` \| `external_agent:<id>` \| `internal:<name>` |
| `origin_account` | TEXT NULL | bei `phone`: `account_type/account_name` (lokal: leer)                  |
| `created_at`     | TEXT      | RFC 3339                                                                |
| `updated_at`     | TEXT      | RFC 3339                                                                |

Ein Kontakt braucht mindestens einen Namensteil, eine Organisation, eine Telefonnummer oder eine
E-Mail-Adresse.

## haex_contacts_values

Eine Angabe (FR-003, FR-007, FR-008 unbestätigt).

| Spalte       | Typ     | Regeln                                                     |
| ------------ | ------- | ---------------------------------------------------------- |
| `id`         | TEXT PK | UUIDv4                                                     |
| `contact_id` | TEXT FK | → `haex_contacts.id` ON DELETE CASCADE                     |
| `kind`       | TEXT    | Art, offene Menge (siehe unten)                            |
| `label`      | TEXT    | `home` \| `work` \| `mobile` \| `other` \| `custom:<text>` |
| `value`      | TEXT    | Text oder JSON je Art                                      |
| `position`   | INTEGER | Reihenfolge innerhalb der Art                              |

Arten dieser Spec und ob sie ins Telefon gehen (FR-014):

| `kind`              | `value`                                                                        | Telefon                      |
| ------------------- | ------------------------------------------------------------------------------ | ---------------------------- |
| `phone`             | Text wie eingegeben                                                            | ja (`Phone`)                 |
| `email`             | Text                                                                           | ja (`Email`)                 |
| `address`           | JSON `{street, pobox, neighborhood, city, region, postcode, country}`          | ja (`StructuredPostal`)      |
| `website`           | URL                                                                            | ja (`Website`)               |
| `im`                | JSON `{service, handle}`                                                       | ja (`Im`, `PROTOCOL_CUSTOM`) |
| `sip`               | Text                                                                           | ja (`SipAddress`)            |
| `event`             | JSON `{date: "YYYY-MM-DD" \| "--MM-DD"}`; `label` `birthday`, `anniversary`, … | ja (`Event`)                 |
| `relation`          | Name                                                                           | ja (`Relation`)              |
| `custom`            | Text; `label` ist die Bezeichnung                                              | nein                         |
| `holzi_vault`       | npub (unbestätigt)                                                             | nein                         |
| `holzi_nostr_relay` | `wss://…` (unbestätigt)                                                        | nein                         |
| `holzi_iroh`        | Endpunkt-ID hex, optional `@<iroh-Relay-URL>` (unbestätigt)                    | nein                         |

Unbekannte `kind`-Werte: nur lesen und anzeigen; der Dienst ändert oder löscht sie nur, wenn der
Kontakt gelöscht wird. Spätere Specs fügen Arten hinzu, ohne Migration.

Prüfung beim Speichern (FR-008): `holzi_vault` muss ein gültiger npub sein, `holzi_nostr_relay` eine
`wss://`- oder `ws://`-Adresse, `holzi_iroh` 64 Hex-Zeichen mit optionaler Relay-URL.

## haex_contacts_cards

Bestätigte holzi-Adressen aus einer Visitenkarte (FR-008 bis FR-013).

| Spalte         | Typ     | Regeln                                                    |
| -------------- | ------- | --------------------------------------------------------- |
| `vault_pubkey` | TEXT PK | hex, x-only; PK erzwingt „nur bei einem Kontakt“ (FR-013) |
| `contact_id`   | TEXT FK | → `haex_contacts.id` ON DELETE CASCADE                    |
| `card`         | BLOB    | vollständige Karte (Länge ‖ postcard ‖ Sig.)              |
| `issued_at`    | TEXT    | aus der Karte; nur eine neuere ersetzt (FR-012)           |
| `received_at`  | TEXT    |                                                           |

Die Geräte und Nostr-Relays werden beim Lesen aus `card` entnommen, nicht doppelt gespeichert.

## haex_contacts_groups, haex_contacts_group_members

| Tabelle                       | Spalten                                                                                     |
| ----------------------------- | ------------------------------------------------------------------------------------------- |
| `haex_contacts_groups`        | `id` TEXT PK (UUIDv4), `name` TEXT                                                          |
| `haex_contacts_group_members` | `id` TEXT PK = UUIDv5(group_id, contact_id), `group_id` FK, `contact_id` FK (beide CASCADE) |

Ziel von Berechtigungen ist der Gruppenname (Erweiterungen) beziehungsweise die Gruppen-ID (Agenten).

## haex_contacts_binaries

| Spalte        | Typ       | Regeln                                                          |
| ------------- | --------- | --------------------------------------------------------------- |
| `hash`        | TEXT PK   | SHA-256 hex des JPEG                                            |
| `data`        | BLOB      | JPEG, ≤ 512 × 512, ≤ 200 KiB (FR-005, R10)                      |
| `orphaned_at` | TEXT NULL | gesetzt, wenn kein Kontakt mehr verweist; nach 7 Tagen gelöscht |

## haex_contacts_targets

Abgleichsziel (FR-018b bis FR-018d).

| Spalte              | Typ     | Regeln                                |
| ------------------- | ------- | ------------------------------------- |
| `id`                | TEXT PK | UUIDv5(contact_id, vault_device_uuid) |
| `contact_id`        | TEXT FK | → `haex_contacts.id` CASCADE          |
| `vault_device_uuid` | TEXT FK | → `known_devices` CASCADE             |
| `account_type`      | TEXT    | leer = lokales Konto                  |
| `account_name`      | TEXT    |                                       |

Ein Kontakt hat höchstens ein Ziel je Telefon (abgeleitete ID). Entfernen einer Zeile + Antwort des
Nutzers (FR-018d) steuert, ob das Telefon den Roh-Kontakt löscht: Bei „löschen“ bleiben die
Zuordnungen des Telefons stehen, und das Telefon löscht beim nächsten Lauf die Roh-Kontakte, die es
ohne Ziel zugeordnet findet (ohne Massenlösch-Sperre, der Nutzer hat genau diese gewählt). Bei
„behalten“ gehen die Zuordnungen des Telefons in derselben Transaktion mit; der Roh-Kontakt bleibt
in `haex_contacts_phone_known_no_sync` und wird nicht neu übernommen. Bekommt der Kontakt das Telefon
wieder als Ziel, führt der Lauf ihn mit diesem Roh-Kontakt zusammen, statt ihn neu zu schreiben.

Gruppen gleicht der Abgleich über den Namen ab: Eine Mitgliedschaft in `haex_contacts_group_members`
entspricht im Telefon einer `GroupMembership` in der Gruppe gleichen Titels im Konto des Roh-Kontakts;
fehlt die Gruppe dort, legt holzi sie an. Gruppen des Systems („My Contacts“) bleiben unberührt.

## haex_contacts_phone_links

Zuordnung je Telefon (FR-024, ADR 0001: synchronisierte Tabelle mit `vault_device_uuid`).

| Spalte              | Typ       | Regeln                                                                   |
| ------------------- | --------- | ------------------------------------------------------------------------ |
| `id`                | TEXT PK   | UUIDv5(vault_device_uuid, raw_contact_id)                                |
| `vault_device_uuid` | TEXT FK   | → `known_devices` CASCADE                                                |
| `contact_id`        | TEXT FK   | → `haex_contacts.id` CASCADE                                             |
| `raw_contact_id`    | INTEGER   | `RawContacts._ID`                                                        |
| `lookup_key`        | TEXT      | `Contacts.LOOKUP_KEY` beim letzten Lauf                                  |
| `account_type`      | TEXT      |                                                                          |
| `account_name`      | TEXT      |                                                                          |
| `source_id`         | TEXT NULL | `RawContacts.SOURCE_ID`, Wiedererkennung nach Neuanlage des Kontos       |
| `version`           | INTEGER   | `RawContacts.VERSION` nach dem letzten Lauf                              |
| `read_only`         | INTEGER   | 0/1                                                                      |
| `base`              | TEXT      | JSON: abgebildete Angaben dieses Roh-Kontakts nach dem letzten Lauf (R6) |
| `photo_base`        | TEXT NULL | Hash des holzi-Fotos beim letzten Lauf                                   |

## haex_contacts_phone_known_no_sync

Welche Roh-Kontakte dieses Telefon schon einmal zugeordnet hat (Migration `0034`). Gerätelokal (kein
CRDT): Löscht holzi einen Kontakt, gehen seine Zuordnungen mit (`ON DELETE CASCADE`); diese Liste
sagt dem Telefon dann noch, welchen Roh-Kontakt es löschen muss, statt ihn als neu zu übernehmen.

| Spalte           | Typ        | Regeln                               |
| ---------------- | ---------- | ------------------------------------ |
| `raw_contact_id` | INTEGER PK | `RawContacts._ID`                    |
| `contact_id`     | TEXT       | Kontakt, zu dem er gehörte (ohne FK) |

Ein Roh-Kontakt ohne Zuordnung, aber in dieser Liste: Ist sein Kontakt gelöscht, löscht der Abgleich
ihn im Telefon (mit der Massenlösch-Sperre); sonst bleibt er unberührt. „Behalten“ bei einer
Massenlöschung und ein entferntes Konto nehmen ihn aus der Liste.

## haex_contacts_phone_issues

Nicht angekommene Änderungen (FR-025).

| Spalte              | Typ       | Regeln                                                       |
| ------------------- | --------- | ------------------------------------------------------------ |
| `id`                | TEXT PK   | UUIDv5(vault_device_uuid, contact_id)                        |
| `vault_device_uuid` | TEXT FK   | → `known_devices` CASCADE                                    |
| `contact_id`        | TEXT FK   | → `haex_contacts.id` CASCADE                                 |
| `reason`            | TEXT      | `read_only` \| `account_removed` \| `permission` \| `failed` |
| `detail`            | TEXT NULL | Fehlertext ohne persönliche Daten                            |
| `at`                | TEXT      |                                                              |

Wird beim nächsten erfolgreichen Schreiben des Kontakts gelöscht.

## haex_contacts_phone_runs

Stand des Abgleichs je Telefon (FR-015a); eine Zeile je Gerät.

| Spalte              | Typ        | Regeln                                                      |
| ------------------- | ---------- | ----------------------------------------------------------- |
| `vault_device_uuid` | TEXT PK FK | → `known_devices` CASCADE                                   |
| `state`             | TEXT       | `review` (erste Übersicht offen) \| `active` \| `paused`    |
| `paused_reason`     | TEXT NULL  | `permission` \| `mass_delete`                               |
| `last_run_at`       | TEXT NULL  |                                                             |
| `counts`            | TEXT NULL  | JSON der letzten Zahlen (übernommen, geschrieben, gelöscht) |

An/aus und Zielkonto liegen nur in `preferences` (Gerät): `contacts.phone.enabled` ist die einzige
Quelle für an/aus, `contacts.phone.target_account` das Zielkonto, `contacts.phone.accounts` die Konten
des Telefons, wie sein letzter Lauf sie sah (damit andere Geräte sie als Ziel anbieten). Ist der Abgleich aus, gibt es keine
Zeile in `haex_contacts_phone_runs` (Ausschalten löscht sie, Zuordnungen bleiben).

Zustände:

```text
(aus, keine Zeile) --einschalten+Berechtigung--> review --bestätigt--> active
active --Massenlöschung erkannt--> paused(mass_delete) --Entscheidung--> active
active --Berechtigung fehlt--> paused(permission) --Berechtigung wieder da--> active
* --ausschalten--> (aus, Zeile gelöscht; Zuordnungen bleiben; beim Wiedereinschalten review)
```

## haex_contacts_own_card

Die zuletzt erstellte eigene Visitenkarte (R12).

| Spalte      | Typ        | Regeln                      |
| ----------- | ---------- | --------------------------- |
| `id`        | INTEGER PK | immer 1                     |
| `card`      | BLOB       | Länge ‖ postcard ‖ Signatur |
| `issued_at` | TEXT       |                             |

Welche Angaben die Karte trägt: `preferences` (Vault) `contacts.card.values` (Liste von Wert-IDs des
eigenen Kontakts) und `contacts.card.own_contact` (ID des eigenen Kontakts).

## haex_contacts_blocked_vaults

| Spalte         | Typ     | Regeln |
| -------------- | ------- | ------ |
| `vault_pubkey` | TEXT PK | hex    |
| `blocked_at`   | TEXT    |        |

## Später mit 027/028 (US7)

Ausgehende und eingegangene Einladungen gehören in die Tabellen der Specs 027/028; diese Spec ergänzt
dort nur `recipient_contact_id` und `delivery_state` (`pending` \| `delivered` \| `undeliverable`) und den
Bereich „Von Unbekannten“. Festgelegt in deren Plänen.
