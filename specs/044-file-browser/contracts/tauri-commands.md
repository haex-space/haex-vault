# Contract: Tauri-Commands des Dateibrowsers (044)

Alle Commands rufen den `FilesService` mit `Caller::User` auf; der Aufrufer kommt aus dem Eingang, nie
aus einem Parameter (ADR 0007). Fehler haben die Form `{ code, message }` ohne Zugangsdaten (FR-038).
Typen werden mit `ts-rs` nach `src/types/bindings/` erzeugt.

## Quellen und Navigation

| Command                    | Eingabe                              | Ausgabe                                                                    |
| -------------------------- | ------------------------------------ | -------------------------------------------------------------------------- |
| `files_sources`            | –                                    | `{ device: { roots: Root[], known: Place[] }, storages: StorageSource[] }` |
| `files_list`               | `source: SourceRef, path: string`    | `Entry[]` (alle Einträge; Sortieren im Fenster)                            |
| `files_stat`               | `source, path`                       | `Entry`                                                                    |
| `files_watch`              | `path, channel: Channel<WatchEvent>` | `watchId` (Desktop und Android; iOS `unsupported`)                         |
| `files_unwatch`            | `watchId`                            | –                                                                          |
| `files_all_access`         | –                                    | `{ granted: bool, platform: 'android' \| 'other' }`                        |
| `files_request_all_access` | –                                    | – (öffnet die Systemeinstellungen, nur Android)                            |

`Root`: Laufwerk mit Name, Pfad, gesamt/frei (falls bekannt). `Place`: bekannte Orte wie Dokumente,
Bilder, Downloads. `StorageSource`: Id und Name des Speichers aus 038.

## Anzeigen

| Command             | Eingabe                          | Ausgabe                                                                                  |
| ------------------- | -------------------------------- | ---------------------------------------------------------------------------------------- |
| `files_open`        | `source, path, tabId`            | `{ kind: 'text'\|'pdf'\|'image'\|'video'\|'audio'\|'info', url?: string, entry: Entry }` |
| `files_read_text`   | `source, path`                   | `{ text: string, truncated: bool }` (bis 5 MB; binär → Fehler `binary`)                  |
| `files_thumbnail`   | `source, path, size, modifiedMs` | Bytes (JPEG) über `tauri::ipc::Response`; Fehler `broken`/`tooLarge`                     |
| `files_release`     | `url`                            | –                                                                                        |
| `files_release_tab` | `tabId`                          | –                                                                                        |
| `files_open_system` | `source, path`                   | – (Gerät: Opener-Plugin; Speicher: erst laden, dann öffnen)                              |

## Ändern

| Command                 | Eingabe                                                                                                                | Ausgabe      |
| ----------------------- | ---------------------------------------------------------------------------------------------------------------------- | ------------ |
| `files_create_folder`   | `source, path, name`                                                                                                   | `Entry`      |
| `files_rename`          | `source, path, newName`                                                                                                | `Entry`      |
| `files_transfer_start`  | `op: 'copy'\|'move'\|'delete', from: SourceRef, paths: string[], to?: {source, path}, channel: Channel<TransferEvent>` | `transferId` |
| `files_transfer_answer` | `transferId, choice: 'replace'\|'keepBoth'\|'skip', forAll: bool`                                                      | –            |
| `files_transfer_cancel` | `transferId`                                                                                                           | –            |
| `files_transfer_retry`  | `transferId`                                                                                                           | –            |
| `files_import_dropped`  | `paths: string[], to: {source, path}, channel`                                                                         | `transferId` |

`TransferEvent`: `progress { items, bytes }`, `conflict { name }`, `done`, `cancelled`,
`failed { code, message }`. Fehler-Codes vor dem Start: `exists`, `intoItself`, `noSpace`,
`holziOwned`, `noAccess`, `notFound`.

## Suche

| Command               | Eingabe                                                       | Ausgabe    |
| --------------------- | ------------------------------------------------------------- | ---------- |
| `files_search_start`  | `source, path, query, filters, channel: Channel<SearchEvent>` | `searchId` |
| `files_search_cancel` | `searchId`                                                    | –          |

`filters`: `types?: ('image'|'video'|'audio'|'document'|'text')[]`, `sizeMin?`, `sizeMax?`,
`modifiedFrom?`, `modifiedTo?`. `SearchEvent`: `hits { entries }`, `progress { dirs }`, `done`,
`failed`. Eine neue Suche desselben Tabs beendet die alte im Fenster über `files_search_cancel`.

## Berechtigungen der Agents

| Command                         | Eingabe                                                    | Ausgabe                       |
| ------------------------------- | ---------------------------------------------------------- | ----------------------------- |
| `files_agent_permissions`       | –                                                          | `AgentFilePermission[]`       |
| `files_agent_permission_set`    | `agentId, kind, target, status`                            | –                             |
| `files_agent_permission_answer` | `args: { requestId, choice: 'read'\|'readWrite'\|'deny' }` | – (Antwort auf die Rückfrage) |
| `files_agent_check`             | `source, path` (nur Handler von `files.show`)              | `FilesAgentShow`              |

Event `files-agent-permission-request { requestId, agentId, storageId, storageName, wants: 'read'|'readWrite' }`
von Rust an das Fenster; ohne Antwort in 60 s oder ohne Fenster gilt „abgelehnt für diesen Aufruf“
(nicht gespeichert).

`FilesAgentShow { allowed, reason?, source?, folder?, open? }`: dieselben Prüfungen wie `files.stat`,
ohne Inhalt; bei `allowed` der Ordner, in dem der Dateibrowser öffnet, und bei einer Datei ihr Name
für den Viewer. `source` nimmt wie die Aktionen auch `storage:<name>` an.
