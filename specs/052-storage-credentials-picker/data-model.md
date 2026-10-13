# Data Model: Zugangsdaten aus dem Passwortmanager wählen

Keine neue Tabelle, keine neue Spalte, keine Migration. Der Trigger-Stand der Tabellen bleibt.

## Vorhandene Daten, neu gedeutet

| Daten                | Ort                                                                               | Änderung                                                                                                                   |
| -------------------- | --------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| Verknüpfung          | `haex_storage_connections.credentials_item_id`                                    | zeigt auf einen Eintrag mit Eigentümer `storage` (von holzi angelegt) **oder** auf einen Eintrag ohne Eigentümer (gewählt) |
| Zugangsdaten-Eintrag | `haex_passwords_item_details` (+ Felder)                                          | bei gewählten Einträgen unverändert; bei von holzi angelegten kommt das Tag dazu                                           |
| Cloud-Speicher-Tag   | `haex_passwords_tags` (`name = 'haex-cloud-storage'`), `haex_passwords_item_tags` | gewöhnliches Tag (034: keine reservierten Namen); Verknüpfungs-Kennung abgeleitet aus Eintrag und Tag                      |

## Zuordnung Eintrag → Zugangsdaten (unverändert, an einer Stelle: `credentials::from_item`)

| Protokoll | Benutzername  | Passwort | Eigene Felder                         | Ergebnis                |
| --------- | ------------- | -------- | ------------------------------------- | ----------------------- |
| S3        | Access-Key-ID | Secret   | `sessionToken` (optional)             | `Credentials::S3`       |
| WebDAV    | Benutzername  | Passwort | –                                     | `Credentials::Password` |
| SFTP      | Benutzername  | Passwort | –                                     | `Credentials::Password` |
| SFTP      | Benutzername  | leer     | `privateKey`, `passphrase` (optional) | `Credentials::SshKey`   |

Ein Eintrag „passt“, wenn `from_item` etwas liefert. Leere Werte zählen als fehlend.

## Zustand der Zugangsdaten einer Verbindung (`CredentialsState`)

| Zustand   | Bedingung (neu: Punkt 3)                                                                                 |
| --------- | -------------------------------------------------------------------------------------------------------- |
| `present` | Eintrag vorhanden, nicht im Papierkorb, `from_item` liefert Zugangsdaten                                 |
| `missing` | 1. Eintrag im Papierkorb, 2. Löschmarker für den Eintrag, 3. Eintrag da, aber `from_item` liefert nichts |
| `syncing` | kein Eintrag, kein Löschmarker (noch nicht angekommen)                                                   |

## Neue Typen (ts-rs, `src/types/bindings/`)

- `CredentialsInput::Entry { itemId }` (`kind: 'entry'`).
- `CredentialsCandidate { id, title, username, url, tagged, owned }` – nie ein Geheimnis.
- `CredentialsEntry { id, title, owned }` – in `ConnectionView.credentialsEntry?`, nur wenn der Eintrag
  vorhanden ist und passt; `title` ist der Titel des Eintrags oder leer, wenn er keinen hat.

## Übergänge der Verknüpfung

```text
neu eingegeben  ──save──▶ Eintrag von holzi angelegt (owner=storage, Tag)       credentials_item_id = neu
gewählt         ──save──▶ Eintrag unverändert + Tag (als Nutzer)                credentials_item_id = itemId
tauschen (egal wohin)   ▶ alter Eintrag nur gelöscht, wenn von holzi angelegt und ohne weitere Verbindung
Verbindung entfernen    ▶ wie tauschen
Eintrag gelöscht        ▶ Zustand missing; Nutzer wählt anderen oder gibt neue ein
```
