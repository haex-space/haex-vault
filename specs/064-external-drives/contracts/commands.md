# Contract: Tauri-Befehle für Laufwerke

Alle Befehle liegen in `src-tauri/src/files/browser_commands/drives.rs`, laufen mit dem Aufrufer
`User` (Fenster) und machen blockierende Arbeit in `spawn_blocking` (Konstitution, Rust-Regeln).
Fehler sind `FilesError` mit den neuen Codes unten. Keiner dieser Befehle ist eine Agent-Action oder
ein Peer-Request (FR-021, FR-022).

## Liste und Änderungen

```ts
files_sources(): Sources // unverändert im Aufruf; Sources.drives: Drive[] jetzt mit allen Zuständen
files_drives_watch(channel: Channel<DrivesChanged>): number // Abo-Id; erste Nachricht sofort
files_drives_unwatch(id: number): void
```

- Der Monitor-Thread startet mit dem ersten Abo und endet mit dem letzten (R8).
- `on_page_load(Started)` beendet alle Abos (wie `unwatch_all`).

## Öffnen (Klick in der Seitenleiste oder im Tab-Hinweis)

```ts
files_drive_open(id: string): DriveOpened

type DriveOpened =
  | { kind: 'opened'; path: string; rememberFailed: 'urlTaken' | 'failed' | null } // eingehängt (ggf. vorher eingehängt/entsperrt)
  | { kind: 'needsPassword'; reason: 'none' | 'several' | 'savedFailed'; message: string | null; candidates: DriveEntry[] }
  | { kind: 'systemDialog' } // Windows/BitLocker: Dialog des Systems geöffnet (FR-019)
```

Ablauf in Rust:

1. Eingehängt → `opened`.
2. Nicht eingehängt → einhängen → `opened`.
3. Gesperrt, und das Backend kann kein Passwort annehmen (Windows) → Dialog des Systems → `systemDialog`.
4. Gesperrt, und das Backend kann ein Passwort annehmen: Einträge mit `haex:drive:<key>` suchen (nur
   mit offenem Vault).
   - Genau einer: Passwort in Rust lesen und entsperren. Gelingt das, einhängen → `opened`. Sonst
     `needsPassword { reason: 'savedFailed', message, candidates: [] }`.
   - Keiner oder mehrere: `needsPassword { reason: 'none' | 'several', candidates }`. Bei `several`
     sind das die Treffer, sonst eine leere Liste.

`DriveEntry = { itemId: string; title: string; username: string | null; url: string | null }`. Nie ein
Passwort.

## Entsperren mit Dialog

```ts
files_drive_unlock(id: string, secret: DriveSecret, remember: DriveRemember): DriveOpened // nur 'opened'

type DriveSecret =
  | { kind: 'typed'; password: string }
  | { kind: 'entry'; itemId: string }

type DriveRemember = 'no' | 'yes' // typed: neuer Eintrag; entry ohne URL: URL setzen

files_drive_link_entry(id: string, itemId: string): void // nach Zustimmung: URL des Eintrags ersetzen
```

- Gemerkt wird erst nach erfolgreichem Entsperren (FR-016). Scheitert das Merken, bleibt das Laufwerk
  offen; die Antwort trägt `rememberFailed: 'failed'` (etwa ein Konflikt beim Eintrag).
- `remember: yes` mit `entry`, dessen URL schon `haex:drive:<key>` ist: keine Änderung.
- `entry` mit einer anderen URL und `remember: yes`: Das Laufwerk ist offen, die Antwort trägt
  `rememberFailed: 'urlTaken'`. Die Oberfläche fragt und ruft bei Zustimmung `files_drive_link_entry`
  auf; so wird nicht zweimal entsperrt.

```ts
files_drive_unlock_candidates(query: string): DriveEntry[] // Einträge mit Passwort, wie storage_credentials_candidates
```

Ohne offenen Vault liefern `files_drive_unlock_candidates` und `secret.kind = 'entry'` den Fehler
`noVault`. Dann graut die Oberfläche den Teil aus (FR-018).

## Aushängen und Auswerfen

```ts
files_drive_release(id: string, cancelTransfers: boolean): DriveReleased

type DriveReleased =
  | { kind: 'released'; ejected: boolean } // ejected: ausgeschaltet oder ausgeworfen, „kann abgezogen werden“
  | { kind: 'transfersRunning'; count: number } // nur bei cancelTransfers = false
```

Reihenfolge:

1. Transfers unter dem Pfad prüfen, ggf. abbrechen.
2. Eigene Watches, Media-Tokens, Suchen und Erweiterungs-Watches unter dem Pfad freigeben (R9).
3. Aushängen.
4. Bei `encrypted`: sperren.
5. Bei `release == eject`: ausschalten bzw. auswerfen.

## Neue `FilesErrorCode`

| Code                | Wann                                                                | Oberfläche                                 |
| ------------------- | ------------------------------------------------------------------- | ------------------------------------------ |
| `driveCancelled`    | Abfrage des Systems abgebrochen oder abgelehnt (FR-007)             | kurzer Hinweis, kein Dialog                |
| `driveBusy`         | ein anderes Programm hält das Laufwerk (FR-012)                     | Hinweis „wird noch … benutzt“              |
| `driveGone`         | `id` ist nicht (mehr) in der Liste (abgezogen)                      | Hinweis, Liste aktualisiert sich ohnehin   |
| `driveFailed`       | System meldet einen anderen Fehler; `message` = Text des Systems    | Hinweis mit Text                           |
| `unlockFailed`      | Entsperren gescheitert; `message` = Text des Systems                | Dialog bleibt offen, zeigt Text (FR-015)   |
| `driveNoFilesystem` | entsperrt, aber kein lesbares Dateisystem (LVM u. a.) (FR-020)      | Hinweis „nicht unterstützt“                |
| `driveUnsupported`  | Plattform/Backend kann die Aktion nicht (kein udisks2, Android)     | Knopf erscheint gar nicht; nur Absicherung |
| `noVault`           | Passwortmanager nötig, aber kein Vault offen                        | Teil ausgegraut                            |
| `urlTaken`          | nur intern: `link_entry` ohne Ersetzen; nach außen `rememberFailed` | Rückfrage „URL ersetzen?“                  |

## Nebenwirkungen auf bestehende Befehle

- `drives()` (Rust) liefert weiter nur eingehängte Laufwerke mit Pfad (R11). Für Agent (`files.sources`)
  und Peer-Wurzeln (`WireRoot.name`) gilt als Name `label`, sonst der letzte Pfadteil wie bisher
  (`display_name`).
- `files_watch`, `files_open`, `files_search` merken sich ihre Wurzel, damit R9 sie nach Pfad findet.
  Ihre Aufrufe bleiben gleich.
