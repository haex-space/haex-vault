# Vertrag: Zuordnung beim Import

**Spec**: [../spec.md](../spec.md) (FR-023, US7) | **Research**: [../research.md](../research.md) R12 | **Commands**: [tauri-commands.md](./tauri-commands.md)

Grundsatz: **Es geht nichts still verloren.** Was ein Feld von holzi hat, kommt dorthin. Was kein
Gegenstück hat, wird ein eigenes Feld (`haex_passwords_item_key_values`, Name wie unten) oder ein
Tag. Was nicht ankommen kann, steht im Bericht (`needsAttention`, ohne Werte von Geheimnissen).
Die Feldnamen der Quellen sind nach dem bekannten Aufbau der Formate eingetragen; die Tests
prüfen sie gegen Beispieldateien, die zur Laufzeit erzeugt oder erfunden sind, und T003 hält fest,
was sich mit den Crates und Beispieldaten belegen ließ. Namen eigener Felder aus diesem Import
stehen auf Deutsch, damit sie in der Oberfläche lesbar sind.

## Gemeinsam für alle Formate

| Quelle                                         | Ziel in holzi                                                                                                                                                                     |
| ---------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Titel, Benutzername, Passwort, Adresse         | `title`, `username`, `password`, `url`                                                                                                                                            |
| Notiz                                          | `note`                                                                                                                                                                            |
| Erstellt, zuletzt geändert                     | `created_at`, `updated_at` (nicht der Importzeitpunkt)                                                                                                                            |
| Ordner                                         | `haex_passwords_groups`, geschachtelt wie in der Quelle                                                                                                                           |
| Gelöschter Eintrag, Papierkorb der Quelle      | Inhalt der Papierkorb-Zeile `trash`; nennt die Quelle den früheren Ordner (Bitwarden `folderId`, KeePass 4.1 `PreviousParentGroup`), ist er `trashed_from_group_id`, sonst Wurzel |
| Verlauf                                        | `item_snapshots` mit den Zeitpunkten der Quelle (und `snapshot_binaries`)                                                                                                         |
| TOTP (jede Form)                               | `otp_secret` und, wo angegeben, Ziffern, Periode, Algorithmus; **ungültig bleibt wie es ist**                                                                                     |
| Passkey                                        | `haex_passwords_passkeys`, jeder Algorithmus (research R12, Punkt 8); schon vorhandene Credential-ID: nicht doppelt, Bericht `passkey_duplicate`                                  |
| Anhang                                         | Anhang; über 25 MiB abgelehnt und im Bericht genannt (Dateiname, Größe, Eintrag, Ordnerpfad)                                                                                      |
| Symbol                                         | Standardsymbol als Symbolname von holzi, eigenes Symbol als Binärzeile `icon` und `binary:<hash>`                                                                                 |
| Tag, Favorit                                   | Tag (Favorit: Tag „Favorit“)                                                                                                                                                      |
| Doppelter Eintrag (`title`, `username`, `url`) | wie vom Nutzer gewählt: überspringen oder trotzdem anlegen (FR-023)                                                                                                               |

## KeePass (kdbx)

| Quelle                                                                                           | Ziel                                                                                 |
| ------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------ |
| Gruppe: Name, Notizen, Symbol                                                                    | Ordner: `name`, `description`, `icon`                                                |
| Papierkorb-Gruppe (`RecycleBinUUID`)                                                             | die Zeile `trash`; ihre Untergruppen und Einträge liegen darunter                    |
| Einträge: `Title`, `UserName`, `Password`, `URL`, `Notes`                                        | `title`, `username`, `password`, `url`, `note`                                       |
| Ablaufzeit (wenn aktiv)                                                                          | `expires_at` (`YYYY-MM-DD`)                                                          |
| `Tags`                                                                                           | Tags                                                                                 |
| `otp`, `OTP`, `TOTP Seed`, `TOTP Settings`, `otpauth://` in Notizen                              | TOTP-Felder; die Notiz bleibt unverändert                                            |
| `KPEX_PASSKEY_*` (KeePassXC)                                                                     | Passkey (Credential-ID, Relying Party, Benutzer, privater Schlüssel, Algorithmus, …) |
| Alle übrigen benutzerdefinierten Felder                                                          | eigene Felder mit demselben Namen                                                    |
| Vorder- und Hintergrundfarbe                                                                     | eigene Felder „KeePass: Vordergrundfarbe“, „KeePass: Hintergrundfarbe“               |
| URL überschreiben                                                                                | eigenes Feld „KeePass: URL überschreiben“                                            |
| Auto-Type (Aktivierung, Standardfolge, Fensterzuordnungen)                                       | eigene Felder „KeePass: Auto-Type …“                                                 |
| Benutzerdefinierte Daten (`CustomData`)                                                          | eigene Felder „KeePass: Zusatzdaten <Schlüssel>“                                     |
| Anhänge (`Binaries`)                                                                             | Anhänge                                                                              |
| `History`                                                                                        | Verlaufsstände mit eigenen Anhängen                                                  |
| Symbol: Standard (0–68) / eigenes                                                                | Symbolname von holzi (Tabelle) / Binärzeile `icon`                                   |
| Einstellungen der Anwendung (Suche und Auto-Type je Gruppe aktiv, aufgeklappt, Qualitätsprüfung) | **eine** Zeile im Bericht „Einstellungen der Quelle, nicht übernommen“               |

## Bitwarden (JSON, unverschlüsselt)

| Quelle                                                                                                                                             | Ziel                                                                                                                 |
| -------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------- |
| `folders[]`                                                                                                                                        | Ordner (ein Name mit `/` wird geschachtelt)                                                                          |
| Typ 1 (Anmeldung): `username`, `password`, `uris[0]`, `totp`                                                                                       | `username`, `password`, `url`, TOTP                                                                                  |
| weitere `uris[]`                                                                                                                                   | eigene Felder „URL 2“, „URL 3“ …; die Zuordnungsart (`match`) als „URL 2 Zuordnung“ …                                |
| Typ 1: `fido2Credentials[]`                                                                                                                        | Passkeys (`keyValue` = privater Schlüssel, `rpId`, `rpName`, `userHandle`, `userName`, `counter`, …)                 |
| Typ 2 (Notiz)                                                                                                                                      | `note`; Tag `secure-note`                                                                                            |
| Typ 3 (Karte): Karteninhaber, Marke, Nummer, Ablaufmonat, -jahr, Prüfnummer                                                                        | eigene Felder „Karteninhaber“, „Marke“, „Kartennummer“, „Ablaufmonat“, „Ablaufjahr“, „Prüfnummer“; Tag `credit-card` |
| Typ 4 (Identität): alle Angaben (Anrede, Namen, Firma, E-Mail, Telefon, Adresse, Sozialversicherungs-, Pass-, Führerscheinnummer, Benutzername, …) | je ein eigenes Feld; Tag `identity`                                                                                  |
| Typ 5 (SSH-Schlüssel): privater, öffentlicher Schlüssel, Fingerabdruck                                                                             | eigene Felder „SSH privater Schlüssel“, „SSH öffentlicher Schlüssel“, „SSH Fingerabdruck“; Tag `ssh-key`             |
| unbekannter Typ                                                                                                                                    | alle nicht zugeordneten Eigenschaften als eigene Felder; Tag „Bitwarden-Typ <Zahl>“                                  |
| `fields[]` (Text, verborgen, Ja/Nein, verknüpft)                                                                                                   | eigene Felder (Name, Wert; ein verknüpftes Feld als „Verknüpft: <Ziel>“)                                             |
| `favorite`                                                                                                                                         | Tag „Favorit“                                                                                                        |
| `reprompt`                                                                                                                                         | eigenes Feld „Bitwarden: Passwort erneut abfragen“                                                                   |
| `collectionIds`, `organizationId`                                                                                                                  | Tags „Sammlung: <Name>“ (Name aus `collections[]`, sonst die Kennung)                                                |
| `deletedDate`                                                                                                                                      | Eintrag liegt im Papierkorb                                                                                          |
| `passwordHistory[]` (Passwort, Zeitpunkt)                                                                                                          | Verlaufsstände (Passwort der Zeit, Zeitpunkt der Quelle)                                                             |
| `creationDate`, `revisionDate`                                                                                                                     | `created_at`, `updated_at`                                                                                           |

Ein verschlüsselter Bitwarden-Export wird mit `ImportFailed { reason: "encrypted_export" }` abgelehnt
(ohne dessen Kennwort lässt sich nichts lesen).

## Bitwarden (CSV)

Spalten `folder`, `favorite`, `type`, `name`, `notes`, `fields`, `reprompt`, `login_uri`,
`login_username`, `login_password`, `login_totp` wie bei JSON (mehrere Adressen in `login_uri`, jede
Zeile eine; `fields` als Zeilen „Name: Wert“ zu eigenen Feldern). Die CSV-Form enthält keine Karten-,
Identitäts-, Passkey- und Verlaufsdaten; das ist eine Eigenschaft der Quelle, kein Verlust des Imports.

## LastPass (CSV)

| Quelle                        | Ziel                                                                                                                               |
| ----------------------------- | ---------------------------------------------------------------------------------------------------------------------------------- |
| `url`, `username`, `password` | `url`, `username`, `password` (`http://sn` bedeutet sichere Notiz: `url` bleibt leer)                                              |
| `totp`                        | TOTP                                                                                                                               |
| `extra`                       | `note` (unverändert); zeilenweise „Schlüssel: Wert“ zusätzlich als eigene Felder; `NoteType` als eigenes Feld „LastPass: Notiztyp“ |
| `name`                        | `title`                                                                                                                            |
| `grouping`                    | Ordner, geschachtelt an `/` und `\`                                                                                                |
| `fav`                         | Tag „Favorit“                                                                                                                      |

## haex-vault 2.x (Legacy) (Spec 037)

Die Vault-Datei von haex-vault 2.x ist eine eigene Quelle (`legacyhaexvault`); ihre Abbildung steht in
[037 contracts/haex-vault-mapping.md](../../037-haex-vault-import/contracts/haex-vault-mapping.md).

Seit Spec 037 (FR-017) gilt für **alle** Quellen: Ein Ordner der Quelle nimmt einen Ordner, den
die Vault schon vor dem Import mit demselben Namen am selben Ort hatte, statt einen zweiten
anzulegen; ein Rückbau entfernt ihn nicht.

## Bericht (`needsAttention`)

Jede Zeile nennt Eintragstitel, Ordnerpfad, die Art der Stelle und den Namen des Feldes oder der
Datei (bei Anhängen mit Größe in MiB und dem Limit), nie einen Wert. Arten: `attachment_too_large`,
`attachment_unreadable`, `passkey_key_unreadable`, `passkey_public_key_missing`, `passkey_duplicate`, `totp_invalid`,
`icon_not_mapped`, `value_not_storable`, `source_setting`. Der Bericht lässt sich als Textdatei
speichern.
