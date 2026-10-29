# Quickstart: Dateibrowser mit zwei Panels

**Feature**: [spec.md](./spec.md) | **Verträge**: [split-location](./contracts/split-location.md),
[panel-context](./contracts/panel-context.md), [shortcuts-and-menu](./contracts/shortcuts-and-menu.md)

## Voraussetzungen

- Worktree `.worktrees/062-files-split-view`, darin `nix develop --command pnpm install --frozen-lockfile`.
- Zwei Testordner außerhalb des Repositorys: `~/split-a` mit `eins.txt`, `zwei.txt`, `drei.png` und
  einem Unterordner `unter/`; `~/split-b` leer.
- Für M6 ein S3-kompatibler Testspeicher (der lokale Server des E2E-Rigs, Spec 038) als Speicher
  eingerichtet.

## Automatische Prüfungen

```sh
nix develop --command pnpm check:files           # inkl. check-files-split, -shortcuts, -registry
nix develop --command pnpm typecheck
nix develop --command pnpm lint
nix develop --command pnpm test:e2e --grep files-split   # files-split, -storage, -restore, -large
nix develop --command pnpm test:e2e --grep files-keyboard
```

Erwartet: alles grün. `files-keyboard` prüft zusätzlich, dass F5 außerhalb des Splits weiter neu lädt.
Die Szenarien `files-split*` laufen nur auf Linux (Desktop); `files-split-storage` braucht Docker.
`check:files`, `typecheck` und `lint` laufen in CI einmal auf Linux (Job „Documentation consistency“,
`ci.yml`); die geprüften Module sind reines TypeScript ohne Betriebssystemzugriff, Pfade anderer
Systeme prüfen sie als Zeichenketten. M13 und A1 bleiben von Hand.
Es gibt keinen Rust-Code in dieser Spec; `cargo`-Läufe sind nur nötig, wenn ein anderer Branch sie verlangt.

## Von Hand (Desktop)

| #    | Schritt                                                                                                                                                                       | Erwartet                                                                                                                                                                 |
| ---- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| M1   | Dateibrowser in `~/split-a` öffnen. In der Werkzeugleiste „Zwei Panels“ einschalten.                                                                                          | Zwei Panels, rechts ebenfalls `~/split-a`; das linke hat den Rahmen der Aktiven (Rahmen und kräftige Pfadleiste).                                                        |
| M2   | Rechts nach `~/split-b` wechseln. Links `eins.txt` und `zwei.txt` auswählen.                                                                                                  | Nur links ist etwas ausgewählt; Pfad und Auswahl rechts unverändert.                                                                                                     |
| M3   | F5 drücken.                                                                                                                                                                   | Transferleiste, dann liegen beide Dateien in `~/split-b` und rechts erscheinen sie; links bleiben sie.                                                                   |
| M4   | Links `drei.png` wählen, F6.                                                                                                                                                  | Datei verschwindet links, erscheint rechts.                                                                                                                              |
| M5   | Tab drücken, Tab nochmals.                                                                                                                                                    | Fokus und Rahmen wechseln links↔rechts; Umschalt+Tab führt aus der Liste zur Werkzeugleiste.                                                                             |
| M6   | Rechts in der Seitenleiste den S3-Speicher wählen, links `unter/` (Ordner) wählen, F5.                                                                                        | Hochladen mit Fortschritt; Abbruch (Transferleiste) lässt kein halbes Objekt zurück.                                                                                     |
| M7   | Links eine Datei auf einen Ordner rechts ziehen; dann mit gehaltener Strg-Taste (macOS Alt) innerhalb derselben Quelle; dann auf die freie Fläche rechts.                     | Innerhalb einer Quelle verschiebt ohne, kopiert mit Strg; zwischen Gerät und Speicher kopiert es immer; freie Fläche = geöffneter Ordner.                                |
| M8   | Rechts einen schreibgeschützten Ort (eine Freigabe ohne Schreibrecht oder einen Ordner unter holzis eigenen Daten) öffnen, eine Datei von links darauf ziehen und F5 drücken. | Beim Überfahren kein Abwurf; F5 zeigt den Grund; nichts wird verändert.                                                                                                  |
| M9   | Beide Panels in denselben Ordner stellen, links eine Datei auswählen, F6 und danach F5.                                                                                       | Hinweis „derselbe Ordner“, nichts geschieht. F5: Namenskonflikt-Frage „beide behalten“.                                                                                  |
| M10  | Split ausschalten (rechts war aktiv), wieder einschalten.                                                                                                                     | Nach dem Ausschalten bleibt der Ort des aktiven Panels einzige Ansicht; nach dem Einschalten steht das zweite Panel am gemerkten Ort.                                    |
| M11  | Split an, beide Panels in verschiedene Ordner (mit Schritten zurück), holzi neu starten, Sitzung wiederherstellen (Spec 022).                                                 | Tab steht wieder im Split, beide Orte und das aktive Panel stimmen; Zurück geht im aktiven Panel; das andere beginnt mit leerem Verlauf.                                 |
| M12  | Datei im linken Panel öffnen (Bild).                                                                                                                                          | Viewer überdeckt beide Panels; „nächste Datei“ springt im linken Ordner; nach Schließen steht der Split wie vorher.                                                      |
| M13  | Fenster schmaler als etwa 720 px Panelfläche ziehen, wieder breit.                                                                                                            | Nur das aktive Panel; Schalter deaktiviert mit Grund; beim Verbreitern kommen beide zurück.                                                                              |
| M14  | Strg+R, Strg+C, Tab, Strg+V.                                                                                                                                                  | Neuladen im Panel mit Fokus; Einfügen in den Ordner des Panels mit Fokus.                                                                                                |
| M14a | Links `unter/` und darin einen Unterordner öffnen, rechts einen Ordner öffnen. Rechts aktiv: Alt+←. Tab, Alt+←.                                                               | Rechts geht zurück, links bleibt; nach Tab geht links einen Ordner zurück, rechts bleibt. Schaltflächen und Verlaufsliste zeigen jeweils den Verlauf des aktiven Panels. |
| M14b | Wie M14a, dann Split ausschalten und wieder einschalten.                                                                                                                      | Das übrige Panel behält seinen Verlauf; das zweite Panel beginnt mit leerem Verlauf.                                                                                     |
| M15  | Ordner mit sehr vielen Einträgen (50 000) rechts, links navigieren und auswählen.                                                                                             | Links reagiert unter 200 ms.                                                                                                                                             |

## Von Hand (Android, wenn ein Gerät oder Emulator da ist)

| #   | Schritt              | Erwartet                                                                                                                                                                                         |
| --- | -------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| A1  | Dateibrowser öffnen. | Kein Split-Schalter. Eine Sitzung mit `split=1` aus dem Desktop zeigt nur ein Panel. Das E2E-Szenario `files-split` läuft deshalb nur auf dem Desktop; `files-basic` prüft die Android-Variante. |
