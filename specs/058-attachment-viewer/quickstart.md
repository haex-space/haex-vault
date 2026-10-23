# Quickstart: Dateianhänge im Passwortmanager anzeigen und öffnen

**Feature**: [spec.md](./spec.md) | **Contract**: [contracts/tauri-commands.md](./contracts/tauri-commands.md)

## Voraussetzungen

- Worktree `.worktrees/058-attachment-viewer`, darin
  `nix develop --command pnpm install --frozen-lockfile`.
- Testdateien (außerhalb des Repositorys anlegen):
  - `notiz.txt` mit ein paar Zeilen, darunter ein Umlaut
  - `gross.log` mit mehr als 5 MB Text
  - `kaputt.txt` mit einem NUL-Byte am Anfang (`printf 'a\0b' > kaputt.txt`)
  - `police.pdf` mit mindestens zwei Seiten, `defekt.pdf` (beliebige Bytes mit Endung pdf)
  - `tabelle.ods` (oder eine andere Datei, die holzi nicht anzeigt)

## Automatische Prüfungen

```sh
nix develop --command scripts/with-nix-host-bridge.sh cargo test -j 4 --manifest-path src-tauri/Cargo.toml --lib passwords::attachment_view
nix develop --command scripts/with-nix-host-bridge.sh cargo test -j 4 --manifest-path src-tauri/Cargo.toml --lib files::streaming
nix develop --command pnpm check:passwords
nix develop --command pnpm check:files
nix develop --command pnpm test:e2e --grep passwords-attachments
```

Erwartet: alle grün. Danach `cargo fmt --check`, `pnpm lint:all` und `pnpm typecheck` wie im CI.

## Von Hand (Desktop)

| #   | Schritt                                                                        | Erwartet                                                                          |
| --- | ------------------------------------------------------------------------------ | --------------------------------------------------------------------------------- |
| M1  | Eintrag anlegen, alle Testdateien anhängen, Tab „Extra“                        | Karten wie bisher; `notiz.txt` zeigt das Text-Symbol                              |
| M2  | Auf `notiz.txt` tippen                                                         | Anhang-Ansicht mit dem Text, Umlaut richtig, Name in der Kopfzeile (US1)          |
| M3  | Eine Zeile markieren, kopieren, Escape                                         | Zwischenablage hat die Zeile; Fokus auf der Karte (US1 S2, S6)                    |
| M4  | `gross.log` öffnen                                                             | Hinweis „abgeschnitten“, Text bis 5 MB (US1 S3)                                   |
| M5  | `kaputt.txt` öffnen                                                            | Hinweis „kein Text“ mit Speichern unter (US1 S4); System-App folgt in PR C         |
| M6  | `police.pdf` öffnen, blättern, zoomen, schließen                               | PDF-Viewer wie in der Dateien-App (US2 S1, S3)                                    |
| M7  | `defekt.pdf` öffnen                                                            | Hinweis, Speichern unter, System-App (US2 S2)                                     |
| M8  | Auf `tabelle.ods` tippen                                                       | Speichern unter wie bisher (US1 S7)                                               |
| M9  | Auf der Karte von `tabelle.ods` „Mit System-App öffnen“, abbrechen             | Hinweis erscheint; danach keine Kopie unter `files-opened-encrypted` (US3 S1, S2) |
| M10 | Noch einmal, Haken „Nicht mehr fragen“, Öffnen                                 | Datei öffnet in der System-App (US3)                                              |
| M11 | Einen anderen Anhang mit der System-App öffnen                                 | Kein Hinweis mehr (US3 S3)                                                        |
| M12 | Vault sperren, den App-Cache-Ordner `files-opened-encrypted` ansehen           | Ordner fehlt oder ist leer (US3 S4, SC-004)                                       |
| M13 | Datei öffnen, holzi hart beenden (`kill -9`), neu starten                      | Kopie beim Start entfernt (US3 S4)                                                |
| M14 | Eintrag in den Papierkorb, dort `notiz.txt` antippen                           | Ansicht öffnet; Umbenennen und Entfernen fehlen (US1 S5)                          |
| M15 | Fenster auf 360 px Breite                                                      | Ansicht füllt das Fenster, nichts scrollt seitwärts                               |
| M16 | Auf einem zweiten Gerät derselben Vault einen Anhang mit der System-App öffnen | Hinweis erscheint dort (FR-010)                                                   |

In PR C zusätzlich: Einen Anhang mit dem Namen `a:b?.txt` auf einem System anlegen, das diesen
Namen erlaubt, und ihn unter Windows mit der System-App öffnen; die Kopie heißt `a_b_.txt` und
öffnet. Alternativ liefert ein Test-Fixture den Anhang mit diesem Namen.

## Von Hand (Android)

| #   | Schritt                               | Erwartet                                      |
| --- | ------------------------------------- | --------------------------------------------- |
| A1  | `notiz.txt` und `police.pdf` antippen | Ansicht wie am Desktop                        |
| A2  | Karte und Ansicht ansehen             | Keine Aktion „Mit System-App öffnen“ (US3 S5) |
