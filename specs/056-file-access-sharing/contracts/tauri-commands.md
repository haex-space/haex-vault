# Vertrag: Commands und Katalog-Aktionen

Neue und geänderte Tauri-Commands. Dateifreigaben laufen über Katalog-Aktionen mit eigenem Scope
`files.shares`: Agents erreichen sie nur mit der Agent-Berechtigung „Dateifreigaben verwalten“, und
Erteilen bestätigt der Nutzer immer in einem Dialog (FR-045, research R17). Der Schalter läuft über
`scope: 'guardrails'` (kein Agent). Fehler als `FilesError` mit den bestehenden Codes plus `Conflict` und
`Unreachable` (neu, research R7, R13).

## Dateibrowser (geändert)

| Command                                                                                          | Änderung                                                                                                                                              |
| ------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------- |
| `files_sources`                                                                                  | `Sources` + `devices: FilesPeerDevice[]` und `contacts: FilesContactShares[]` (data-model.md)                                                         |
| `files_list`, `files_stat`, `files_open`, `files_thumbnail`, `files_read_text`, Transfers, Suche | unverändert; `SourceRef::Storage { storageId: "peer:…" \| "share:…" }` wird über `PeerStore` aufgelöst (research R2)                                  |
| `files_watch`                                                                                    | neuer Parameter `storage` (`peer:`/`share:`); Meldungen kommen über `Watch` vom bedienenden Gerät, das Ende als letzte Meldung (FR-046, research R16) |
| `files_peer_roots` (neu)                                                                         | `{ storageId }` → `FilesPeerRoots`: Laufwerke und bekannte Orte eines anderen eigenen Geräts; das Fenster öffnet dort den persönlichen Ordner         |
| `files_peer_trash` (neu)                                                                         | `{ storageId }` → `bool`: ob Löschen dort in den Papierkorb geht (`Welcome.trash`); ohne Papierkorb fragt das Fenster vorher (FR-038, research R8)    |
| `files_open_system`                                                                              | für `peer:`/`share:` erst herunterladen in den Zwischenspeicher wie bei Speichern                                                                     |

## Neu: Kontakte im Dateibrowser

| Command               | Eingabe         | Ausgabe              | Wirkung                                                                                                         |
| --------------------- | --------------- | -------------------- | --------------------------------------------------------------------------------------------------------------- |
| `files_contact_query` | `{ contactId }` | `FilesContactShares` | wählt die Geräte der Karte an (FR-008), fragt `Shares`, speichert empfangene Freigaben (FR-028, FR-030, FR-031) |

## Neu: Dateifreigaben (Aktionen `files.shares.*`, Scope `files.shares`)

| Aktion / Command                                    | Eingabe                                                              | Ausgabe             |
| --------------------------------------------------- | -------------------------------------------------------------------- | ------------------- |
| `files.shares.list` / `file_shares_list`            | —                                                                    | `FileShare[]`       |
| `files.shares.of` / `file_share_of`                 | `{ source: SourceRef, path }`                                        | `FileShare \| null` |
| `files.shares.save` / `file_share_save`             | `{ shareId?, source, path, name, grants: { vaultPubkey, level }[] }` | `FileShare`         |
| `files.shares.revoke` / `file_share_revoke`         | `{ shareId, vaultPubkey }`                                           | `FileShare`         |
| `files.shares.delete` / `file_share_delete`         | `{ shareId }`                                                        | `()`                |
| `files.shares.ofContact` / `file_shares_of_contact` | `{ contactId }`                                                      | `FileShare[]`       |

Regeln:

- `save` ohne vorhandene Freigabe (Neuanlage) nur für Quellen dieses Geräts: `source` ist `device`
  oder ein Speicher aus `haex_storages`; `peer:`/`share:` → `Refused` (FR-015). Bedienendes Gerät =
  dieses Gerät (FR-018).
- `save` mit `shareId` einer vorhandenen Freigabe (Name, Kontakte, Stufen) von jedem eigenen Gerät;
  Quelle und bedienendes Gerät bleiben unverändert (FR-019).
- `save` auf eine schon freigegebene Quelle aktualisiert dieselbe Freigabe (FR-017).
- `grants` nur für Kontakte mit gespeicherter Karte (FR-016).
- Jede Änderung setzt `changed_at`; auf dem bedienenden Gerät setzt der Abonnent `served_at`.
- Aufruf durch einen Agent: ohne `AgentFileKind::FileShares` → `Refused`; Anlegen, neuer Kontakt oder
  höhere Stufe → Bestätigungsdialog `FileShareConfirmRequest` über das Fenster, Antwort bestätigen oder
  ablehnen; ohne Antwort in 120 s oder ohne Fenster → `Refused`; Entziehen, Löschen und Herabstufen
  ohne Dialog.

## Neu: Schalter „Geräte-Dateizugriff“

| Aktion / Command                                              | Eingabe       | Ausgabe                      |
| ------------------------------------------------------------- | ------------- | ---------------------------- |
| `files_device_access_get` (Command, liest nur)                | —             | `boolean`                    |
| `settings.files.deviceAccess.set` / `files_device_access_set` | `{ enabled }` | `{ enabled }` (Stand danach) |

Wie `settings.privacy.screenCapture.set` ist nur das Setzen eine Aktion (`scope: 'guardrails'`); das
Lesen ruft das Fenster direkt.

## Geändert: Kontakte

Der Löschdialog nennt die betroffenen Dateifreigaben über `files.shares.ofContact`
(`file_shares_of_contact`, FR-023); ein eigenes `contacts_delete_impact` braucht es nicht.

## Ereignisse

- `vault-data-changed` mit `haex_file_shares*` aktualisiert Übersicht, Dialog und Seitenleiste.
- Transfers melden `Conflict` wie bisher über ihre Konfliktfrage (research R7).
