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
| `contactsIndex`      | `{afterId, limit}`              | `[{id, version, deleted, accountType, accountName, sourceId, contactId, lookupKey, lastUpdated, readOnly}]` |
| `contactsRead`       | `{ids: number[]}` (≤ 200)       | `[{id, hasPhoto, data: [{dataId, mimetype, isReadOnly, fields}]}]`                                          |
| `contactsApply`      | `{ops: Op[]}` (≤ 400 je Aufruf) | `{results: [{rawContactId?, version?}], error?}`                                                            |
| `contactsPhotoRead`  | `{id}`                          | `{jpegBase64?}` (`DisplayPhoto`, sonst Vorschaubild)                                                        |
| `contactsPhotoWrite` | `{id, jpegBase64}`              | `{version?}` (`RawContacts.DisplayPhoto`, das Vorschaubild macht der Provider)                              |
| `contactsGroups`     | `{accountType, accountName}`    | `[{id, title, readOnly}]` (nicht gelöschte Gruppen; `readOnly` = Gruppe des Systems, `SYSTEM_ID` gesetzt)   |

`lastUpdated` ist `Contacts.CONTACT_LAST_UPDATED_TIMESTAMP` (ms) des zusammengefassten Kontakts
(Konfliktregel, research R6). Listen kommen als `{rows: […]}`, weil eine Antwort des Plugins ein Objekt
ist. `contactsIndex` blättert nach ID (`_ID > afterId`, `LIMIT_PARAM_KEY`), damit eine Seite nicht von
Löschungen zwischen zwei Aufrufen verschoben wird. Accounts kommen aus den Roh-Kontakten (dafür reicht
`READ_CONTACTS`); `isDefault` liefert das Plugin noch nicht (immer `false`), Rust schlägt dann das erste
beschreibbare Konto eines Sync-Adapters vor. `readOnly` und `isReadOnly` sind immer `false`: Der Provider beantwortet
`RAW_CONTACT_IS_READ_ONLY` und `IS_READ_ONLY` gegenüber Apps nicht (`Invalid column`); nur lesbar ist
ein Roh-Kontakt in einem Konto, dessen Sync-Adapter nicht hochlädt.

`fields` je Mimetype: `StructuredName` (given, family, middle, prefix, suffix, display),
`Nickname`, `Organization` (company, title), `Note`, `Phone`/`Email`/`Website`/`SipAddress`
(value, type, label), `StructuredPostal` (Teile), `Im` (protocol, customProtocol, data),
`Event` (startDate, type, label), `Relation` (name, type, label), `GroupMembership` (groupId; beim
Lesen zusätzlich `title` und `readOnly` der Gruppe, damit Rust Mitgliedschaften über den Titel
abgleicht und die Gruppen des Systems wie „My Contacts“ nie anfasst).
Andere Mimetypes liefert `contactsRead` nicht; holzi schreibt sie nie (FR-022).

`Op`:

- `insertRaw {accountType, accountName, ref}`: neuer Roh-Kontakt; `ref` für Rückverweise
- `insertData {rawContactId | ref, mimetype, fields}`
- `updateData {dataId, mimetype, fields}`
- `deleteData {dataId}`
- `deleteRaw {rawContactId}`
- `insertGroup {accountType, accountName, title}`: neue Gruppe eines Kontos. Rust legt fehlende
  Gruppen in einem eigenen Aufruf vor dem Batch an und liest ihre IDs danach mit `contactsGroups`;
  eine Mitgliedschaft ist ein `insertData` mit `GroupMembership` und `groupId` (T096).

`raw` ist `{id}` oder `{ref}`; ein `ref` ist der Index des `insertRaw` im selben Aufruf.

`contactsApply` führt die Ops mit `applyBatch` aus (`withYieldAllowed` alle 100), liest danach
`VERSION` der berührten Roh-Kontakte und gibt sie zurück (R6). Ein `OperationApplicationException`
bricht nur diesen Aufruf ab; Rust trägt eine Issue ein (FR-025).

## Grenzen

- Antworten ≤ 1 MiB je Aufruf (Paging über `offset`/`limit` und `ids`).
- Fotos getrennt vom Batch (Größe der Binder-Transaktion).
- Alle Aufrufe laufen außerhalb des UI-Threads, auf einem eigenen Thread des Plugins (ein
  `Executor`, damit das Plugin keine Coroutines-Abhängigkeit braucht).
