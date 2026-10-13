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
| `storage_dialog_resolve`  | `Confirm` ergänzt um `hostKey?` (vom Nutzer im Fenster über der App bestätigt), `createFolder?` (das Wort des Nutzers, einen fehlenden Ordner anzulegen, FR-006; PR E) und `credentials` in der getaggten Form; `StorageTrial.failed` kann `hostKeyUnknown` mit `hostKey` tragen, dann zeigt das Fenster den Fingerabdruck zur Bestätigung.                                                                                                              |

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
  | { kind: 'generatedKey'; username: string; keyId: string }
```

`s3` nur für S3-Verbindungen, `password` für WebDAV und SFTP, `sshKey` und `generatedKey` nur für SFTP;
sonst Fehler
`invalid credentials`. Ein privater Schlüssel, den `russh::keys::decode_secret_key` nicht lesen kann
(auch: falsche Passphrase), ist `invalid credentials` vor jedem Test.

`ConnectionInput` und `StorageInput` haben zusätzlich `createFolder?: boolean` (Standard `false`): Findet
der Test den Ordner eines WebDAV- oder SFTP-Speichers nicht, legt holzi ihn nur dann an und testet erneut
(FR-006). `StorageView` hat in der Antwort von `storage_save` `warning?: "nestedFolder"` (FR-003). Fehler
`StorageInvalid` nennen bei WebDAV und SFTP das Feld `folder` statt `bucket` und bei Benutzername und
Passwort das Feld `login`.

## Neue Commands

| Command                    | Argumente                             | Antwort                                                                                                                                                                                                                                                                                                                                                                                                |
| -------------------------- | ------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `storage_ssh_key_generate` | —                                     | `{ keyId, publicKey }`: neuer Ed25519-Schlüssel (research R4). Der private Schlüssel bleibt im Prozess (vorläufige Schlüssel nach `keyId`, `Zeroizing`, verworfen nach 30 Minuten, beim Sperren der Vault und nach dem Speichern); `storage_connection_save` mit `{ kind: 'generatedKey', keyId }` legt ihn im Passwortmanager ab. Eine unbekannte oder abgelaufene `keyId` ist `invalid credentials`. |
| `storage_host_key_accept`  | `{ connectionId, hostKey }`           | `ConnectionView`; nur wenn der zuletzt gezeigte Hostschlüssel dieser Verbindung auf diesem Gerät (im Prozess gemerkt bei `hostKeyChanged`, research R5) genau dieser `hostKey` ist. Setzt `host_key` und `host_key_confirmed_at`, verwirft offene SSH-Sitzungen, testet danach.                                                                                                                        |
| `storage_folder_create`    | `{ id }` (ein gespeicherter Speicher) | `TestResult` des anschließenden Tests; legt den Ordner (mit fehlenden Elternordnern) an, nur auf ausdrückliche Aktion des Nutzers (FR-006). Vor dem ersten Speichern schickt die Oberfläche stattdessen `createFolder: true` mit `storage_connection_save` bzw. `storage_save`.                                                                                                                        |

Der Nutzer muss den öffentlichen Schlüssel erst beim Anbieter hinterlegen, bevor der Test gelingen kann;
darum liegt der erzeugte private Schlüssel bis zum Speichern als vorläufiger Schlüssel im Prozess. Die
Oberfläche kennt nur `keyId` und den öffentlichen Schlüssel (FR-008).

## Fehler an die Oberfläche

`HostKeyChanged` und `QuotaExceeded` kommen im Dateibrowser als eigene Fehlercodes (`FilesErrorCode`)
an, damit er sie benennt und auf die Einstellungen verweist (US2 Szenario 5, Randfälle).
