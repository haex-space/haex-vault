# Contract: Tauri-Commands der Speicher für gewählte Zugangsdaten

Ändert [038 contracts/tauri-commands.md](../../038-storage-connections/contracts/tauri-commands.md) und
[050 contracts/tauri-commands.md](../../050-webdav-sftp-storage/contracts/tauri-commands.md). Alle Commands in
`src-tauri/src/remote_storage/commands.rs`, Typen mit ts-rs nach `src/types/bindings/`. Kein Command gibt ein
Geheimnis zurück (038 FR-005, 052 FR-002, SC-005).

## Neuer Command

| Command                          | Eingabe              | Antwort                  | Fehler                                                            |
| -------------------------------- | -------------------- | ------------------------ | ----------------------------------------------------------------- |
| `storage_credentials_candidates` | `protocol: Protocol` | `CredentialsCandidate[]` | `VaultClosed`, `NoActiveInstance`; `invalid protocol` für `local` |

```ts
type CredentialsCandidate = {
  id: string // Kennung des Eintrags (opak)
  title: string | null
  username: string // bei S3 die Access-Key-ID; nie das Geheimnis
  url: string | null
  tagged: boolean // trägt das Cloud-Speicher-Tag
  owned: boolean // von holzi angelegt (Eigentümer `storage`)
}
```

Inhalt: nur Einträge, die der Nutzer sieht (nicht im Papierkorb) und die für `protocol` passen (siehe
[data-model.md](../data-model.md)). Reihenfolge: `tagged` zuerst, dann Titel ohne Beachtung der
Schreibweise. Die Suche macht das Fenster.

## Geänderte Commands

| Command                     | Änderung                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| --------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `storage_list`              | `ConnectionView` ergänzt um `credentialsEntry?: { id, title, owned }`; `credentials` ist `missing` auch bei vorhandenem, aber unpassendem Eintrag (FR-010).                                                                                                                                                                                                                                                                                                       |
| `storage_connection_save`   | `ConnectionInput.credentials` darf `{ kind: 'entry', itemId }` sein (alle Protokolle). Der Eintrag muss für den internen Aufrufer lesbar sein und für das Protokoll passen, sonst `StorageCredentialsUnavailable` (nicht vorhanden/Papierkorb: Zustand `missing`) bzw. `invalid credentials` (passt nicht). Getestet wird mit den Werten des Eintrags; erst danach wird die Verknüpfung gesetzt und der Eintrag getaggt. Ein Fehltest speichert und taggt nichts. |
| `storage_dialog_resolve`    | `StorageAnswer::Confirm.credentials` darf dieselbe Form `entry` tragen (US4); die Erweiterung erfährt weder Kennung noch Titel.                                                                                                                                                                                                                                                                                                                                   |
| `storage_connection_remove` | unverändert im Vertrag; ein gewählter Eintrag bleibt, ein von holzi angelegter geht, wenn keine Verbindung ihn nutzt.                                                                                                                                                                                                                                                                                                                                             |

`CredentialsInput` (ergänzt, getaggt nach `kind`):

```ts
type CredentialsInput =
  | {
      kind: 's3'
      accessKeyId: string
      secretAccessKey: string
      sessionToken?: string
    }
  | { kind: 'password'; username: string; password: string }
  | {
      kind: 'sshKey'
      username: string
      privateKey: string
      passphrase?: string
    }
  | { kind: 'generatedKey'; username: string; keyId: string }
  | { kind: 'entry'; itemId: string } // neu
```

## Unverändert

- Bridge-Funktionen `extension_remote_storage_*` (038 FR-008, 050 contracts/bridge.md): kein Parameter und
  keine Antwort kennt Zugangsdaten, Eintrags-Kennungen oder Titel; ein Aufruf mit Zugangsdaten bleibt
  abgelehnt (FR-011).
- Regel Z14 und die Warnung vor dem Löschen (034 FR-034).

## Fehlerbild

| Lage                                                  | Antwort                                                              |
| ----------------------------------------------------- | -------------------------------------------------------------------- |
| `itemId` unbekannt, im Papierkorb, fremder Eigentümer | `StorageCredentialsUnavailable { state }` ohne Hinweis, welcher Fall |
| Eintrag lesbar, Werte fehlen oder falsches Protokoll  | `invalid credentials`                                                |
| Test scheitert                                        | Ergebnis des Tests wie 038 FR-003; nichts gespeichert, kein Tag      |
