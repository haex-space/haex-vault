# Contract: Tauri-Commands der Speicher für WebDAV und SFTP

Ändert [038 contracts/tauri-commands.md](../../038-storage-connections/contracts/tauri-commands.md). Alle
Commands in `src-tauri/src/remote_storage/commands.rs`, Typen mit ts-rs nach `src/types/bindings/`. Kein
Command gibt ein Geheimnis zurück (FR-007, FR-020).

## Geänderte Commands

| Command                   | Änderung                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| ------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `storage_list`            | `ConnectionView` ergänzt um `protocol`, bei SFTP `hostKey?: { fingerprint, confirmedAt }`; `StorageView.bucket` trägt bei WebDAV und SFTP den Ordner (Anzeige „Ordner“).                                                                                                                                                                                                                                                                                 |
| `storage_connection_save` | `ConnectionInput` ergänzt um `protocol` (beim Ändern muss es dem gespeicherten gleichen), `hostKey?` (die OpenSSH-Zeile, die der Nutzer nach `hostKeyUnknown` bestätigt hat) und `credentials` als getaggte Form (unten); `bucketForTest` = Bucket oder Ordner. Test vor dem Speichern wie 038 und zusätzlich bei geändertem Port. Ohne bestätigten Hostschlüssel endet der Test einer SFTP-Verbindung mit `hostKeyUnknown` und nichts wird gespeichert. |
| `storage_save`            | `StorageInput.bucket` = Ordnerpfad bei WebDAV und SFTP; Antwort zusätzlich `warning?: "nestedFolder"` (FR-003).                                                                                                                                                                                                                                                                                                                                          |
| `storage_test`            | `TestResult` ergänzt um `hostKey?: { key, fingerprint }` bei `hostKeyUnknown` und `hostKeyChanged`; neue `TestOutcome`-Werte `folderMissing`, `notWebDav`, `hostKeyUnknown`, `hostKeyChanged`.                                                                                                                                                                                                                                                           |
| `storage_dialog_resolve`  | `Confirm` ergänzt um `hostKey?` (vom Nutzer im Fenster über der App bestätigt) und `credentials` in der getaggten Form; `StorageTrial.failed` kann `hostKeyUnknown` mit `hostKey` tragen, dann zeigt das Fenster den Fingerabdruck zur Bestätigung.                                                                                                                                                                                                      |

`CredentialsInput` (getaggt nach `kind`):

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
```

`s3` nur für S3-Verbindungen, `password` für WebDAV und SFTP, `sshKey` nur für SFTP; sonst Fehler
`invalid credentials`. Ein privater Schlüssel, den `russh::keys::decode_secret_key` nicht lesen kann
(auch: falsche Passphrase), ist `invalid credentials` vor jedem Test.

## Neue Commands

| Command                    | Argumente                                                                  | Antwort                                                                                                                                                                                                                             |
| -------------------------- | -------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `storage_ssh_key_generate` | —                                                                          | `{ privateKey, publicKey }`: neuer Ed25519-Schlüssel (research R4). Der private Schlüssel geht nur ins Formular zurück und wird erst mit `storage_connection_save` im Passwortmanager abgelegt; holzi speichert ihn sonst nirgends. |
| `storage_host_key_accept`  | `{ connectionId, hostKey }`                                                | `ConnectionView`; nur wenn der letzte Test dieser Verbindung auf diesem Gerät `hostKeyChanged` mit genau diesem `hostKey` ergab. Setzt `host_key` und `host_key_confirmed_at`, verwirft offene SSH-Sitzungen, testet danach.        |
| `storage_folder_create`    | `{ storageId }` oder `{ connectionId, folder }` (vor dem ersten Speichern) | `TestResult` des anschließenden Tests; legt den Ordner (mit fehlenden Elternordnern) an, nur auf ausdrückliche Aktion des Nutzers (FR-006).                                                                                         |

`storage_ssh_key_generate` gibt den privaten Schlüssel einmal an das Formular, weil der Nutzer den
öffentlichen Schlüssel erst beim Anbieter hinterlegen muss, bevor der Test gelingen kann. Die Oberfläche
hält ihn nur im Formular und zeigt ihn nicht an.

## Fehler an die Oberfläche

`HostKeyChanged` und `QuotaExceeded` kommen im Dateibrowser als eigene Fehlercodes (`FilesErrorCode`)
an, damit er sie benennt und auf die Einstellungen verweist (US2 Szenario 5, Randfälle).
