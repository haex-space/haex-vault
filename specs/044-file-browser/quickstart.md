# Quickstart: Dateibrowser und Viewer prüfen (044)

Automatische Prüfungen laufen mit `cargo test`, `pnpm check:files`, `pnpm check:agent-actions` und der
E2E-Szene `files` (`pnpm test:e2e -- --scene files`). Diese Anleitung deckt ab, was nur ein echtes Gerät
zeigt.

## 0. Testordner

```sh
mkdir -p /tmp/haex-files/{a,b}
printf 'Hallo\n' > /tmp/haex-files/notiz.txt
cp <ein PDF> /tmp/haex-files/brief.pdf
cp <ein PNG/JPEG mit EXIF-Drehung> /tmp/haex-files/a/foto.jpg
cp <ein MP3> /tmp/haex-files/a/lied.mp3
cp <ein MP4 mit mindestens 4 GB> /tmp/haex-files/b/film.mp4
ln -s /tmp/haex-files /tmp/haex-files/a/schleife
```

## 1. Ansehen (US1)

1. Dateibrowser öffnen, `/tmp/haex-files` ansteuern; Liste und Raster wechseln. Das Foto steht im
   Raster aufrecht.
2. `notiz.txt`, `brief.pdf` (blättern, zoomen) und `foto.jpg` (zoomen, weiter) öffnen.
3. In einem Terminal `touch /tmp/haex-files/neu.txt`: erscheint ohne Neuladen.
4. Zweiten Tab in `a/` öffnen, HaexVault beenden, mit Sitzung wieder öffnen: beide Tabs stehen richtig.

## 2. Abspielen (US2, SC-002, SC-003)

1. `film.mp4` öffnen: startet in unter 2 s; an drei Stellen springen.
2. Speicherbedarf von HaexVault beobachten (`ps -o rss`): wächst um weniger als 100 MB.
3. `lied.mp3` öffnen: startet sofort.
4. Tab schließen, die zuvor kopierte Freigabe-URL mit `curl -I` abfragen: `404`.
5. Windows, macOS und Android prüft die CI mit der Plattform-Probe (tasks.md T007, T041); von Hand nur,
   wenn ein Probe-Job rot ist.

## 3. Verwalten (US3)

1. Ordner `c` anlegen; `notiz.txt` in `notiz2.txt` umbenennen; einen vorhandenen Namen wählen → abgelehnt.
2. `film.mp4` nach `c/` kopieren und mittendrin abbrechen: in `c/` keine Teil-Datei.
3. `a/` nach `c/` kopieren, dann noch einmal: Rückfrage zum Namen, „beide behalten“ für alle.
4. `a/` in `a/schleife/` kopieren → vor dem Start abgelehnt.
5. `c/` löschen: liegt im Papierkorb des Systems.
6. Eine Datei aus dem System-Dateimanager in `b/` ziehen: wird kopiert.

## 4. Suchen (US4)

1. In `/tmp/haex-files` nach `noitz` suchen: findet `notiz2.txt`.
2. Filter „Bilder“ und „letztes Jahr“: nur passende Treffer.
3. Die Suche endet trotz `schleife`.
4. Ab `/` suchen: `/proc`, `/sys`, eingehängte USB-Laufwerke kommen nicht vor.

## 5. Speicher (US5)

Voraussetzung: RustFS lokal wie in Spec 038 Quickstart §5, ein Speicher in den Einstellungen.

1. Speicher in der Seitenleiste wählen; Ordner wechseln.
2. `film.mp4` hochladen, mittendrin abbrechen: im Bucket kein Objekt und kein offener Multipart-Upload
   (`mc ls --incomplete`).
3. Hochladen bis zum Ende, dann aus dem Speicher abspielen und springen.
4. Zugangsdaten in 038 ungültig machen: Dateibrowser nennt den Grund und verweist auf die Einstellungen.

## 6. Agent (US6)

Im Chat mit einem Anthropic-Modell (nimmt Bilder an) und Freigabestufe „Auto“:

1. „Suche brief.pdf unter /tmp/haex-files und fasse ihn zusammen“: keine Rückfrage, Antwort aus dem PDF.
2. „Was ist auf foto.jpg?“: das Modell beschreibt das Bild.
3. „Zeig mir film.mp4“: Dateibrowser öffnet sich mit dem Video.
4. „Verschiebe notiz2.txt nach b/“: Rückfrage (Risky).
5. „Lies die Vault-Datei von HaexVault“: Ablehnung `files_blocked`, kein Inhalt.
6. „Liste den Speicher <Name> auf“: Rückfrage zur Speicher-Berechtigung; „Lesen erlauben“; erneut
   fragen: keine Rückfrage mehr. „Kopiere notiz2.txt dorthin“: abgelehnt (nur Lesen).
7. Mit einem lokalen Modell (nimmt keine Bilder an) Schritt 2: Hinweis statt Bild.

## 7. Android (US7)

1. Frisch installieren, Dateibrowser öffnen: Erklärung und Knopf.
2. Berechtigung erteilen, zurück: Speicher des Geräts erscheint ohne Neustart.
3. Ein Video aus `DCIM/` abspielen und springen; eine Datei löschen: Rückfrage, endgültig.
4. `DCIM/Camera` offen lassen und ein Foto aufnehmen: erscheint ohne Neuladen.
