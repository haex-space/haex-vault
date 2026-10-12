# Contract: Änderungen an den Tauri-Commands des Dateibrowsers (048)

Grundlage sind die Commands aus [044](../../044-file-browser/contracts/tauri-commands.md). Pfade
bleiben Pfade des Fensters: Ein verschlüsselter Ordner erscheint mit seinem entschlüsselten Namen,
`/Privat/Unterlagen/Belege/strom.pdf` geht also durch den verschlüsselten Ordner „Unterlagen“. Nur
`files/encrypted` kennt Präfixe und Objektnamen. Typen kommen wie in 044 über `ts-rs`.

## Neue Felder

`Entry` bekommt:

| Feld        | Typ                                                               | Bedeutung                                                               |
| ----------- | ----------------------------------------------------------------- | ----------------------------------------------------------------------- |
| `encrypted` | `null` \| `{ role: 'folder' \| 'inside', state: EncryptedState }` | `folder`: der verschlüsselte Ordner selbst; `inside`: ein Eintrag darin |
| `conflict`  | `boolean`                                                         | Konfliktkopie nach dem Format (Namensergänzung schon im `name`)         |

`EncryptedState`: `readable`, `otherVault`, `newerFormat`, `damaged`. Ein beschädigter Eintrag im
Ordner trägt `state: 'damaged'` und lässt sich nur löschen.

## Neue Commands

| Command                         | Eingabe              | Ausgabe                                                                  |
| ------------------------------- | -------------------- | ------------------------------------------------------------------------ |
| `files_create_encrypted_folder` | `source, path, name` | `Entry`; Fehler `inEncrypted`, `inExtensionArea`, `exists`, `noKey`      |
| `files_encrypted_notice`        | –                    | `{ shown: boolean }` (Hinweis aus FR-002 schon gezeigt, Gerätepräferenz) |
| `files_encrypted_notice_ack`    | –                    | –                                                                        |
| `files_encrypted_refresh`       | `source, path`       | – (lädt die Begleitdateien des verschlüsselten Ordners unter `path` neu) |

`noKey`: Das Gerät hat noch keinen Inhaltsschlüssel der Vault (vor dem ersten Datensync).

## Geänderte Commands

- `files_rename` auf einen verschlüsselten Ordner schreibt nur seinen Kopf; auf einen Eintrag darin nur
  dessen Begleitdatei.
- `files_open_system`: neue Eingabe `confirmed: bool`. Für einen Eintrag in einem verschlüsselten
  Ordner antwortet der Command ohne `confirmed` mit dem Fehler `needsNotice`, solange der Nutzer den
  Hinweis für diesen Ordner nicht mit „Nicht mehr fragen“ bestätigt hat. Neue Eingabe
  `dontAskAgain: bool` speichert das je Gerät und Ordner. Auf Android liefert er weiter `unsupported`,
  bis 044 das Öffnen dort baut (research R11).
- `files_transfer_start`: Das Ereignis `conflict` bekommt die Variante `leavesEncryption { count }`.
  Sie kommt einmal vor dem Start, wenn Einträge aus einem verschlüsselten Ordner in einen
  gewöhnlichen Ordner eines Speichers gehen (FR-019). Antworten über `files_transfer_answer` mit
  `choice: 'proceed' | 'cancel'`. Ziel Gerät fragt nicht.
- `files_transfer_start` mit Ziel in einem verschlüsselten Ordner verschlüsselt; Quelle in einem
  verschlüsselten Ordner entschlüsselt. Kopieren im selben verschlüsselten Ordner kopiert beim
  Anbieter (FR-018), zwischen zwei verschlüsselten Ordnern ebenfalls und verpackt den Dateischlüssel
  neu (FR-020).
- `files_create_folder` in einem verschlüsselten Ordner legt einen Eintrag `folder` an, kein
  Marker-Objekt.
- `files_search_start` durchsucht verschlüsselte Ordner über die entschlüsselten Namen; neue
  Fehlercodes gibt es nicht.
- `files_thumbnail` für Einträge in verschlüsselten Ordnern antwortet aus dem Arbeitsspeicher und legt
  nichts im Cache-Verzeichnis ab.

## Auf dem Gerät (User Story 7)

- `files_create_encrypted_folder` und `files_encrypted_refresh` nehmen auch `source: { kind: 'device' }`;
  Fehler `holziOwned` in holzis eigenen Daten, `inEncrypted` in einem verschlüsselten Ordner.
- Alle Commands, die einen Gerätepfad nehmen, behandeln einen Pfad in einem verschlüsselten Ordner wie
  im Speicher (research R17); `files_list` eines gewöhnlichen Ordners zeigt `<zufall>.hxef` als
  verschlüsselten Ordner mit Namen.
- `files_transfer_start` mit `op: 'delete'` in oder auf einen verschlüsselten Ordner des Geräts löscht
  endgültig; das Fenster fragt vorher wie bei einem Speicher.
- `EncryptedInfo.foreign`: Zahl fremder Dateien des Ordners; das Fenster zeigt einen Hinweis, wenn sie
  größer als 0 ist.

## Fehlercodes

Neu: `inEncrypted`, `inExtensionArea`, `noKey`, `needsNotice`, `damaged`, `otherVault`,
`newerFormat`. Meldungen nennen nie Namen, Pfade oder Objektnamen aus einem verschlüsselten Ordner
(FR-027); das Fenster setzt den Namen aus seiner eigenen Ansicht ein.
