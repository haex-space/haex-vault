# Contract: Oberfläche

## Seitenleiste „Laufwerke“ (`components/files/Sidebar.vue`)

Je Laufwerk eine Zeile, `:key="drive.id"`, `data-testid="files-drive-<id>-<label ?? id>"` (E2E); die
sichtbare Beschriftung bleibt das Laufwerkslabel.

| Zustand     | Symbol                                                              | Rechts                                    | Klick                           |
| ----------- | ------------------------------------------------------------------- | ----------------------------------------- | ------------------------------- |
| `mounted`   | `system`/`internal`: `lucide:hard-drive`; `removable`: `lucide:usb` | frei (wie heute) und Knopf nach `release` | öffnet `path`                   |
| `unmounted` | wie oben, abgeschwächt                                              | —                                         | `files_drive_open`, Spinner     |
| `locked`    | `lucide:lock`                                                       | —                                         | `files_drive_open`, ggf. Dialog |

- Knopf `release == eject`: `lucide:arrow-up-from-line`, Titel „Auswerfen“,
  `data-testid="files-drive-eject-<…>"`. Bei `unmount`: gleicher Knopf, Titel „Aushängen“.
- Laufende Aktion: Spinner statt Symbol, Zeile gesperrt.
- Hervorhebung `current`: `driveOf` und `isCurrentDrive` verlangen eine Pfadkomponenten-Grenze und
  wählen unter den passenden Laufwerken das mit dem längsten Präfix des offenen Ordners (statt
  `current === drive.path`); `/mnt/a` darf also nicht zu `/mnt/ab` passen.

## Dialog „Laufwerk entsperren“ (`components/files/DriveUnlockDialog.vue`)

- Titel: „{name} entsperren“.
- Hinweiszeile je nach `reason`:
  - `savedFailed`: „Mit dem gespeicherten Passwort ging es nicht: {message}“; bei `message: null` wird
    eine generische Meldung zum fehlgeschlagenen gespeicherten Passwort angezeigt.
  - `several`: „Mehrere gespeicherte Passwörter passen – bitte eins wählen“
- Auswahl: „Passwort eingeben“ (Feld, `type=password`) oder „Aus Passwortmanager wählen“ (Liste wie
  `CredentialsEntryPicker.vue`, bei `several` vorbefüllt mit den Treffern).
- Checkbox „Im Passwortmanager merken“, Vorgabe aus. Beim Eintippen heißt sie so; bei einem gewählten
  Eintrag „Mit diesem Laufwerk verknüpfen“.
- Bei `rememberFailed: 'failed'` wird eine generische Fehlermeldung zum Speichern angezeigt; das
  Laufwerk bleibt offen und die Oberfläche verwirft den Fehler nicht.
- Bei `rememberFailed: 'urlTaken'` (das Laufwerk ist schon offen) eine Rückfrage: „Der Eintrag hat
  schon die Adresse {url}. Durch die Laufwerks-Adresse ersetzen?“ Ja → `files_drive_link_entry`; Nein →
  nichts.
- Fehler `unlockFailed`: Text unter dem Feld, Dialog bleibt offen. Das Passwortfeld wird nach jedem
  Versuch geleert (FR-017).
- `noVault`: Auswahl und Checkbox ausgegraut mit Hinweis „Passwortmanager nur mit offenem Vault“.

## Aushängen/Auswerfen mit laufenden Transfers

Zuerst `files_drive_release(id, false)` aufrufen. Liefert der Befehl `transfersRunning`, zeigt die
Oberfläche den Bestätigungsdialog „{n} Übertragungen laufen noch. Abbrechen und {releaseVerb}?“. Nur
nach Bestätigung erneut mit `true` aufrufen. Bei `release == 'unmount'` heißt die Aktion „Aushängen“,
sonst „Auswerfen“.

## Hinweise (Toast)

| Anlass                    | Text                                                  |
| ------------------------- | ----------------------------------------------------- |
| `released`, `ejected`     | „<Name> kann abgezogen werden“                        |
| `released`, nicht ejected | „<Name> ausgehängt“                                   |
| `driveCancelled`          | „Laufwerksaktion abgebrochen“                         |
| `driveBusy`               | „<Name> wird noch von einem anderen Programm benutzt“ |
| `driveNoFilesystem`       | „Der Inhalt von <Name> wird nicht unterstützt“        |
| `driveFailed`             | „<Name>: <message>“                                   |
| Rücksprung (R10)          | „<Ordner> gibt es nicht mehr – zeige <Ziel>“          |

## Tab-Hinweis (`components/files/DriveNotice.vue`)

Statt der Liste: „Laufwerk <Name> ist nicht eingehängt“ (oder „… ist gesperrt“) mit Knopf „Einhängen“
bzw. „Entsperren“. Der Klick läuft wie in der Seitenleiste. Bei Erfolg öffnet der Tab den
umgerechneten Pfad (data-model, Tab-Ort).

## i18n

Alle Texte in `src/i18n/locales/{de,en}.json` unter `files.drives.*`. Ersatzname
`files.drives.unnamed` = „Datenträger ({size})“.
