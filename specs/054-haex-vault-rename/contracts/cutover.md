# Vertrag: Umzug des Repos (Stufe S11)

Setzt FR-019 bis FR-022 um. Voraussetzung: S0–S10 gemergt, alle Pflichtprüfungen auf `main` grün,
keine offenen PRs und keine Arbeit in anderen Worktrees, die auf alte Namen baut. Jeder Schritt
steht hier einzeln, weil er von außen sichtbar und nicht von selbst umkehrbar ist; **der Betreiber
bestätigt vor Schritt 2 und vor Schritt 4.**

## Vorher (Prüfliste)

1. Die Kopie der alten Datei öffnet sich in der umbenannten App (S9, SC-003); die Sicherung liegt
   unverändert vor.
2. `git grep -n -i holzi` ist außerhalb der Ausnahmeliste leer; die Prüfung des alten Namens ist
   grün auf `main`.
3. Tag `holzi-final` ist im Remote vorhanden.
4. Einstellungen des alten Repos notiert: Branch-Schutz von `main`/`develop`, Pflichtprüfungen,
   Secrets (nur die Namen, nie die Werte), Variablen, Dependabot, Webhooks, Pages, Deploy-Keys.
5. Einstellungen von `haexmas/holzi` notiert: Branch-Schutz von `main` (Pflichtprüfungen:
   Documentation consistency, Frontend build, Rust checks (default), Rust checks (no-default),
   End-to-end suite), Secrets (der Android-Signaturschlüssel, sobald er besteht), Auto-Merge.
6. Rechte: Der Account, der überträgt, ist Eigentümer von `haexmas/holzi`; der Empfänger
   (`haex-space`) gewährt dem Konto Administratorrechte zum Annehmen der Übertragung.

## Ablauf

| #   | Schritt                                                                                                                                                                                                        | Wirkung                                                                                                                          | Umkehrbar?                                        |
| --- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------- |
| 1   | Offene Arbeit abschließen oder verwerfen; kurzes Einfrieren der Merges                                                                                                                                         | verhindert Konflikte während der Umstellung                                                                                      | ja                                                |
| 2   | **Bestätigung des Betreibers.** `haex-space/haex-vault` → `haex-space/haex-vault-legacy` umbenennen                                                                                                            | die alte App liegt unter neuer Adresse, GitHub leitet die alte Adresse bis zur Neuvergabe weiter                                 | ja, solange die Adresse `haex-vault` unbelegt ist |
| 3   | Legacy-Repo: README-Hinweis auf das neue Repo, Archivieren                                                                                                                                                     | schreibgeschützt (FR-019)                                                                                                        | ja (Archiv aufheben)                              |
| 4   | **Bestätigung des Betreibers.** `haexmas/holzi` nach `haex-space` übertragen, danach zu `haex-vault` umbenennen                                                                                                | Historie, PRs, Issues bleiben; die Adresse `haex-space/haex-vault` zeigt jetzt die umbenannte App; `haexmas/holzi` leitet weiter | nur durch erneutes Übertragen                     |
| 5   | Branch-Schutz für `main` und Pflichtprüfungen neu setzen (Liste aus „Vorher“ 5); Default-Branch `main` prüfen                                                                                                  | gleiche Schutzregeln wie vorher (US 5, Szenario 3)                                                                               | ja                                                |
| 6   | Secrets, Variablen und automatische Abläufe neu anlegen (Namen aus „Vorher“ 5); Dependabot und Auto-Merge prüfen                                                                                               | Release und Auto-Merge funktionieren                                                                                             | ja                                                |
| 7   | **PR „URLs“**: eigene Adresse in `Cargo.toml` (`repository`), Flake, README, Abzeichen, Doku (FR-021); die vier Links auf die alte App auf `haex-space/haex-vault-legacy` umstellen                            | Links führen an den richtigen Ort                                                                                                | ja                                                |
| 8   | Lokal: `~/Projekte/haex-vault` → `~/Projekte/haex-vault-legacy`, `~/Projekte/holzi` → `~/Projekte/haex-vault`; `git remote set-url origin https://github.com/haex-space/haex-vault.git`; `git worktree repair` | FR-022                                                                                                                           | ja                                                |
| 9   | Nachlauf: Claude-Speicherverzeichnis des Projekts und `graphify-out` an den neuen Pfad hängen; `.worktrees/` aufräumen                                                                                         | Werkzeuge finden den Stand                                                                                                       | ja                                                |

Schritt 7 geschieht **nach** Schritt 4, weil der PR gegen das neue Repo läuft. Zwischen 4 und 7
zeigen einzelne Links noch auf die alte App; das ist hinzunehmen.

## Prüfung nach dem Umzug

- `https://github.com/haexmas/holzi` leitet auf `haex-space/haex-vault` weiter; PR #474 und #476
  sind dort auffindbar (SC-006).
- Das Legacy-Repo ist archiviert und verweist auf das neue.
- Ein Test-PR gegen `main` läuft durch alle Pflichtprüfungen, und ein Push direkt auf `main` wird
  abgelehnt.
- `git ls-remote origin` im lokalen Checkout zeigt das neue Repo, `git worktree list` keine
  fehlerhaften Einträge.

## Rückfall

- Nach Schritt 2 ohne Schritt 4: Umbenennung des Legacy-Repos zurücknehmen; die Adresse wird frei.
- Nach Schritt 4: Das Repo lässt sich nicht ohne Weiteres zurück übertragen. Deshalb steht die
  Bestätigung davor, und die Liste „Vorher“ ist abgeschlossen. Geht danach etwas schief
  (Pflichtprüfungen, Secrets), wird es in den Einstellungen des neuen Repos behoben; die Quellen
  sind nicht betroffen.
- Der Tag `holzi-final` bleibt als Rückfallpunkt der Quellen.
