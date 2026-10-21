# Vertrag: Commands und Katalog-Aktionen

Neue und geänderte Tauri-Commands. Alle Commands, die Dateifreigaben anlegen, ändern oder entziehen
oder den Schalter umlegen, laufen über Katalog-Aktionen mit `scope: 'guardrails'`: Agents können sie
nicht aufrufen (FR-045). Fehler als `FilesError` mit den bestehenden Codes plus `Conflict` und
`Unreachable` (neu, research R7, R13).

## Dateibrowser (geändert)

| Command                                                                                          | Änderung                                                                                                             |
| ------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------- |
| `files_sources`                                                                                  | `Sources` + `devices: FilesPeerDevice[]` und `contacts: FilesContactShares[]` (data-model.md)                        |
| `files_list`, `files_stat`, `files_open`, `files_thumbnail`, `files_read_text`, Transfers, Suche | unverändert; `SourceRef::Storage { storageId: "peer:…" \| "share:…" }` wird über `PeerStore` aufgelöst (research R2) |
| `files_open_system`                                                                              | für `peer:`/`share:` erst herunterladen in den Zwischenspeicher wie bei Speichern                                    |

## Neu: Kontakte im Dateibrowser

| Command               | Eingabe         | Ausgabe              | Wirkung                                                                                             |
| --------------------- | --------------- | -------------------- | --------------------------------------------------------------------------------------------------- |
| `files_contact_query` | `{ contactId }` | `FilesContactShares` | klopft an (nostr-knock.md), fragt `Shares`, speichert empfangene Freigaben (FR-028, FR-030, FR-031) |

## Neu: Dateifreigaben (Aktionen `files.shares.*`, `scope: 'guardrails'`)

| Aktion / Command                                    | Eingabe                                                    | Ausgabe             |
| --------------------------------------------------- | ---------------------------------------------------------- | ------------------- |
| `files.shares.list` / `file_shares_list`            | —                                                          | `FileShare[]`       |
| `files.shares.of` / `file_share_of`                 | `{ source: SourceRef, path }`                              | `FileShare \| null` |
| `files.shares.save` / `file_share_save`             | `{ source, path, name, grants: { vaultPubkey, level }[] }` | `FileShare`         |
| `files.shares.revoke` / `file_share_revoke`         | `{ shareId, vaultPubkey }`                                 | `FileShare`         |
| `files.shares.delete` / `file_share_delete`         | `{ shareId }`                                              | `()`                |
| `files.shares.ofContact` / `file_shares_of_contact` | `{ contactId }`                                            | `FileShare[]`       |

Regeln:

- `save` nur für Quellen dieses Geräts: `source` ist `device` oder ein Speicher aus `haex_storages`;
  `peer:`/`share:` → `Refused` (FR-015). Bedienendes Gerät = dieses Gerät (FR-018).
- `save` auf eine schon freigegebene Quelle aktualisiert dieselbe Freigabe (FR-017).
- `grants` nur für Kontakte mit gespeicherter Karte (FR-016).
- Jede Änderung setzt `changed_at`; auf dem bedienenden Gerät setzt der Abonnent `served_at`.

## Neu: Schalter „Geräte-Dateizugriff“

| Aktion / Command                                              | Eingabe       | Ausgabe   |
| ------------------------------------------------------------- | ------------- | --------- |
| `settings.files.deviceAccess.get` / `files_device_access_get` | —             | `boolean` |
| `settings.files.deviceAccess.set` / `files_device_access_set` | `{ enabled }` | `()`      |

## Geändert: Kontakte

| Command                  | Änderung                                                                         |
| ------------------------ | -------------------------------------------------------------------------------- |
| `contacts_delete_impact` | neu: `{ contactId }` → `{ shares: { id, name }[] }` für den Löschdialog (FR-023) |

## Ereignisse

- `vault-data-changed` mit `haex_file_shares*` aktualisiert Übersicht, Dialog und Seitenleiste.
- Transfers melden `Conflict` wie bisher über ihre Konfliktfrage (research R7).
