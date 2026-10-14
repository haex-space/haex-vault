# Android-Plugin: Befehle für das Telefon-Adressbuch

Im Plugin `src-tauri/plugins/holzi-android` (ADR 0010). Kotlin bildet nur Zeilen des Contacts
Providers auf JSON ab; jede Regel liegt in Rust (`contacts/phone/`). Rust ruft über
`run_mobile_plugin`; `other.rs` liefert auf dem Desktop `PhoneContactsUnavailable`.

## Manifest und Berechtigung

- `android/src/main/AndroidManifest.xml`: `READ_CONTACTS`, `WRITE_CONTACTS`, `GET_ACCOUNTS` (nur bis
  API 22 nötig, mit `maxSdkVersion="22"`).
- Kotlin: in `@TauriPlugin(permissions = …)` ein Eintrag mit `READ_CONTACTS` und `WRITE_CONTACTS`,
  Alias `contacts`; Rust `Permission::Contacts`. Anfrage wie `voice.rs::ensure_microphone` in
  `spawn_blocking`; alles außer `Granted` → `HolziError::ContactsDenied`.

## Befehle

| Kotlin               | Eingabe                         | Ausgabe                                                                                                     |
| -------------------- | ------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| `contactsAccounts`   | –                               | `[{type, name, writable, isDefault}]` (Sync-Adapter mit `supportsUploading`, lokales Konto)                 |
| `contactsIndex`      | `{offset, limit}`               | `[{id, version, deleted, accountType, accountName, sourceId, contactId, lookupKey, lastUpdated, readOnly}]` |
| `contactsRead`       | `{ids: number[]}` (≤ 200)       | `[{id, data: [{dataId, mimetype, isReadOnly, fields}]}]`                                                    |
| `contactsApply`      | `{ops: Op[]}` (≤ 400 je Aufruf) | `{results: [{rawContactId?, version?}], error?}`                                                            |
| `contactsPhotoRead`  | `{id}`                          | `{jpegBase64?}` (`DisplayPhoto`, sonst Vorschaubild)                                                        |
| `contactsPhotoWrite` | `{id, jpegBase64}`              | `{version}`                                                                                                 |
| `contactsGroups`     | `{accountType, accountName}`    | `[{id, title, readOnly}]`                                                                                   |

`lastUpdated` ist `Contacts.CONTACT_LAST_UPDATED_TIMESTAMP` (ms) des zusammengefassten Kontakts
(Konfliktregel, research R6).

`fields` je Mimetype: `StructuredName` (given, family, middle, prefix, suffix, display),
`Nickname`, `Organization` (company, title), `Note`, `Phone`/`Email`/`Website`/`SipAddress`
(value, type, label), `StructuredPostal` (Teile), `Im` (protocol, customProtocol, data),
`Event` (startDate, type, label), `Relation` (name, type, label), `GroupMembership` (groupId).
Andere Mimetypes liefert `contactsRead` nicht; holzi schreibt sie nie (FR-022).

`Op`:

- `insertRaw {accountType, accountName, ref}`: neuer Roh-Kontakt; `ref` für Rückverweise
- `insertData {rawContactId | ref, mimetype, fields}`
- `updateData {dataId, fields}`
- `deleteData {dataId}`
- `deleteRaw {rawContactId}`
- `insertGroup {accountType, accountName, title, ref}` / `addToGroup {rawContactId | ref, groupId | ref}`

`contactsApply` führt die Ops mit `applyBatch` aus (`withYieldAllowed` alle 100), liest danach
`VERSION` der berührten Roh-Kontakte und gibt sie zurück (R6). Ein `OperationApplicationException`
bricht nur diesen Aufruf ab; Rust trägt eine Issue ein (FR-025).

## Grenzen

- Antworten ≤ 1 MiB je Aufruf (Paging über `offset`/`limit` und `ids`).
- Fotos getrennt vom Batch (Größe der Binder-Transaktion).
- Alle Aufrufe laufen außerhalb des UI-Threads (Kotlin `Dispatchers.IO`).
