# Contract: Oberfläche

## Seitenleiste „Laufwerke“ (`components/files/Sidebar.vue`)

Je Laufwerk eine Zeile, `:key="drive.id"`, `data-testid="files-drive-<label ?? id>"` (E2E).

| Zustand     | Symbol                                                              | Rechts                                    | Klick                           |
| ----------- | ------------------------------------------------------------------- | ----------------------------------------- | ------------------------------- |
| `mounted`   | `system`/`internal`: `lucide:hard-drive`; `removable`: `lucide:usb` | frei (wie heute) und Knopf nach `release` | öffnet `path`                   |
| `unmounted` | wie oben, abgeschwächt                                              | —                                         | `files_drive_open`, Spinner     |
| `locked`    | `lucide:lock`                                                       | —                                         | `files_drive_open`, ggf. Dialog |

- Knopf `release == eject`: `lucide:arrow-up-from-line`, Titel „Auswerfen“,
  `data-testid="files-drive-eject-<…>"`. Bei `unmount`: gleicher Knopf, Titel „Aushängen“.
- Laufende Aktion: Spinner statt Symbol, Zeile gesperrt.
- Hervorhebung `current`: das Laufwerk mit dem längsten `path`-Präfix des offenen Ordners (statt
  `current === drive.path`).

## Dialog „Laufwerk entsperren“ (`components/files/DriveUnlockDialog.vue`)

- Titel: „<Name> entsperren“.
- Hinweiszeile je nach `reason`:
  - `savedFailed`: „Mit dem gespeicherten Passwort ging es nicht: <message>“
  - `several`: „Mehrere gespeicherte Passwörter passen – bitte eins wählen“
- Auswahl: „Passwort eingeben“ (Feld, `type=password`) oder „Aus Passwortmanager wählen“ (Liste wie
  `CredentialsEntryPicker.vue`, bei `several` vorbefüllt mit den Treffern).
- Checkbox „Im Passwortmanager merken“, Vorgabe aus. Beim Eintippen heißt sie so; bei einem gewählten
  Eintrag „Mit diesem Laufwerk verknüpfen“.
- Bei `urlTaken` eine Rückfrage: „Der Eintrag hat schon die Adresse <url>. Durch die Laufwerks-Adresse
  ersetzen?“ Ja → `replaceUrl`; Nein → `remember: no`.
- Fehler `unlockFailed`: Text unter dem Feld, Dialog bleibt offen. Das Passwortfeld wird nach jedem
  Versuch geleert (FR-017).
- `noVault`: Auswahl und Checkbox ausgegraut mit Hinweis „Passwortmanager nur mit offenem Vault“.

## Auswerfen mit laufenden Transfers

Bestätigungsdialog „<n> Übertragungen laufen noch. Abbrechen und auswerfen?“ → `files_drive_release(id,
true)`.

## Hinweise (Toast)

| Anlass                    | Text                                                  |
| ------------------------- | ----------------------------------------------------- |
| `released`, `ejected`     | „<Name> kann abgezogen werden“                        |
| `released`, nicht ejected | „<Name> ausgehängt“                                   |
| `driveCancelled`          | „Einhängen abgebrochen“                               |
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
