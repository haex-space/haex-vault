# Contract: Verschlüsselte Ordner für Erweiterungen (048)

Neue Funktionen der Brücke und des vault-sdk (eigener PR im vault-sdk, haexmas-Fork, ohne
Agent-Zuschreibung). Grundlage: Berechtigungsart `remoteStorage` aus 038 und die neue Art
`encryptedFolder` (research R12).

## Berechtigungsart `encryptedFolder`

- Ziel: `<storage_id>/<folder_id>` (`folder_id` = Base32 der Ordner-Kennung).
- Aktionen: `read`, `readWrite` (`readWrite` deckt `read`).
- Zustände wie in 017: `granted`, `denied`; keine Zeile = fragen.
- **Nie im Manifest**: Ein Manifest mit dieser Kategorie (auch unter einem Alias) lehnt holzi bei
  `install_preview`, `install`, Update und beim Laden einer Entwicklerversion mit
  `BundleRejection::ForbiddenPermission { category }` ab.
- Gespeichert wird nur mit Haken „Erlaubnis merken“ (ab Werk aus). Ohne Haken hält holzi die Antwort im
  Prozess, bis der Frame der Erweiterung entladen oder neu geladen wird, höchstens bis zum Sperren der
  Vault.

## Funktionen

| Funktion                              | Eingabe                                      | Ausgabe                                           | Berechtigung                                       |
| ------------------------------------- | -------------------------------------------- | ------------------------------------------------- | -------------------------------------------------- |
| `extension_encrypted_folder_choose`   | `{ storageId, access: 'read'\|'readWrite' }` | `{ folderId, name }` oder Abbruch                 | `remoteStorage` für `storageId`; Frage nach FR-036 |
| `extension_encrypted_folder_list`     | `{ storageId, folderId, path }`              | `{ entries: [{ name, kind, size?, modified? }] }` | `encryptedFolder` `read`                           |
| `extension_encrypted_folder_download` | `{ storageId, folderId, path }`              | Bytes (Grenzen aus 017)                           | `read`                                             |
| `extension_encrypted_folder_upload`   | `{ storageId, folderId, path, data }`        | –                                                 | `readWrite`                                        |
| `extension_encrypted_folder_delete`   | `{ storageId, folderId, path }`              | –                                                 | `readWrite`                                        |

- `choose` öffnet in holzi eine Auswahl der lesbaren verschlüsselten Ordner des Speichers mit ihren
  Namen und stellt in derselben Ansicht die Frage (Lesen erlauben / Lesen und Schreiben erlauben /
  Ablehnen, Haken „Erlaubnis merken“). Die Erweiterung erfährt Namen erst nach einer Erlaubnis.
- Die übrigen Funktionen fragen, wenn keine gehaltene oder gespeicherte Erlaubnis besteht, mit dem
  Namen des Ordners in der Frage; die Erweiterung bekommt bis zur Antwort 1004 wie in 017.
- `path` ist relativ zum verschlüsselten Ordner und wird wie Schlüssel in 038 FR-011 geprüft.
- Keine Funktion gibt Objektnamen, Schlüssel, Begleitdateien oder rohe Bytes beim Anbieter heraus
  (FR-036).

### Auf dem Gerät (User Story 7, Betreiber 2026-10-13)

- `storageId` ist `device:/` (Linux, macOS, Android) oder `device:C:` je Laufwerk (Windows).
- Statt `remoteStorage` braucht die Erweiterung Dateizugriff (`filesystem`, Lesen bzw. Lesen und
  Schreiben) auf den Ordner, in dem der verschlüsselte Ordner liegt; ohne ihn 1002 ohne Frage. Die
  Erlaubnis je Ordner (`encryptedFolder`) kommt wie im Speicher dazu und wird immer gefragt.
- `choose` bietet die Ordner an, die diese Vault auf dem Gerät kennt (Zwischenspeicher der Köpfe)
  und deren Ort die Erweiterung erreicht; ist es keiner, 1002 ohne Auswahl. Das Gerät wird nie
  durchsucht.
- Die Auswahl und die Frage nennen den Ordner „auf diesem Gerät“.

## Fehler

| Code | Fall                                                                                                                  |
| ---- | --------------------------------------------------------------------------------------------------------------------- |
| 1002 | Erlaubnis abgelehnt, keine `remoteStorage`-Berechtigung oder auf dem Gerät kein Dateizugriff auf den Ort (ohne Frage) |
| 1004 | Frage an den Nutzer läuft                                                                                             |
| 2002 | Anbieter-Fehler wie 038 (`accessDenied`, `network`, `bucketMissing`)                                                  |
| 3001 | ungültiger `path` oder unbekannte `folderId`                                                                          |
| 3010 | Eintrag beschädigt (`damaged`)                                                                                        |
