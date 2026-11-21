# Quickstart: Workspace-Shell validieren

**Feature**: 015-workspace-shell | **Date**: 2026-09-21

Ein Leitfaden zum Prüfen der fertigen Funktion. Details zu Datenformen und
Commands stehen in [data-model.md](./data-model.md) und
[contracts/](./contracts/); hier stehen nur Vorbereitung, Befehle und erwartete
Ergebnisse.

## Voraussetzungen

- Arbeit im Worktree `.worktrees/015-workspace-shell` (Branch `015-workspace-shell`).
- Abhängigkeiten dort **regulär installieren** (`pnpm install`), `node_modules` nicht
  aus einem anderen Checkout verlinken.
- Werkzeuge kommen aus dem Nix-Devshell (`direnv allow` bzw. `nix develop`);
  Rust-Befehle und `pnpm tauri:dev` laufen darin (Host-Bridge über
  `scripts/with-nix-host-bridge.sh`).
- Node 22.19 (`.nvmrc`); die Prüfskripte laufen per Type-Stripping ohne Build.

## 0. Baseline vor dem Chat-Split (T005)

`pnpm check:chat-state` vor jeder Änderung an `pages/chat/[instance].vue`
(2026-09-24, Commit `3301fdd`): **45 Replay-Tests, alle grün** (`# tests 45`,
`# pass 45`, `# fail 0`). Jeder Extraktionsschritt in Phase 2 muss dieselbe
Zahl grüner Tests melden — ein Rückgang oder Anstieg zeigt geändertes
Verhalten statt reiner Verschiebung an.

Bestätigt nach T006 (`useComposer.ts`, Commit `1a8a14a`), nach T008
(`useComposerAttachments.ts`, Commit `695881e`) und nach T010-T013
(Komponenten-Split, Commit `9ac6d20`, Seite 1109 → 492 Zeilen): weiterhin
45/45 grün. T014 zusätzlich `pnpm typecheck`, `pnpm lint`,
`pnpm check:templates` (33 Templates kompilieren) — alle grün.

## 1. Automatische Prüfungen (CI-Parität)

```bash
# Rust: Speicherschicht und Commands (getrennte *_tests.rs)
cargo test --manifest-path src-tauri/Cargo.toml shell
cargo test --manifest-path src-tauri/Cargo.toml            # gesamte Suite
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings

# ts-rs-Bindings erneuern und Drift prüfen
pnpm generate:ts-types && git diff --exit-code src/types/bindings

# Frontend: reine Module + Store mit gemocktem invoke
pnpm check:shell-state
pnpm check:chat-state          # Regressionsnetz für Chat-Split und Umzug (SC-007): Zahl der Replay-Tests unverändert
pnpm check:templates
pnpm typecheck && pnpm typecheck:scripts
pnpm lint && pnpm format:check
```

**Erwartet**: alles grün. `check:shell-state` deckt ab: Singleton über Fenster und
Tabs, Tab öffnen/wechseln/schließen (Nachbar, letzter Tab schließt Fenster),
Maximieren ohne Geometrieverlust, Kaskade, Klemmen, Kompaktrückkehr,
Speicher-Queue und `flushAsync` vor `close`, unbekannte App beim Wiederherstellen.

## 2. Manuelle Szenarien (`pnpm tauri:dev`)

Mit einer Vault mit abgeschlossenem Onboarding starten. Jede Zeile entspricht
einer User Story der [spec.md](./spec.md).

| Story | Schritte                                                                                                                                                       | Erwartet                                                                                                                                                                    |
| ----- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| US1   | Vault öffnen → Launcher → „Chat“ → Launcher → „Einstellungen“                                                                                                  | Arbeitsbereich statt Stub; zwei Fenster gleichzeitig; Chat sendet eine Nachricht; Gerätename ändern klappt; `/chat/<vault>` im Adressfeld landet im Arbeitsbereich mit Chat |
| US1   | Vor dem Onboarding-Abschluss den Arbeitsbereich aufrufen                                                                                                       | Weiterleitung in den Wizard, keine Shell                                                                                                                                    |
| US2   | Fenster ziehen, an Kante/Ecke vergrößern, über den Rand schieben, minimieren, über die Fensterübersicht zurückholen, Doppelklick auf Titelleiste               | Fenster folgt dem Zeiger; Titelleisten-Ausschnitt bleibt erreichbar; Maximieren füllt den Bereich, Wiederherstellen kehrt zur alten Geometrie zurück                        |
| US2   | Im Chat einen Entwurf tippen, Antwort starten, Fenster minimieren und wiederherstellen                                                                         | Entwurf unverändert, Antwort lief weiter                                                                                                                                    |
| US2   | Chat-Antwort mit Tool-Freigabe auslösen, Chat minimieren                                                                                                       | Aufmerksamkeitspunkt an Fensterübersicht/Launcher; Antwort bleibt pausiert                                                                                                  |
| US3   | Im Chat-Fenster „+“ → „Einstellungen“ → über den Chevron zurück zu „Chat“ → Einstellungs-Tab schließen                                                         | „+“ direkt hinter dem letzten Tab; Chevron links neben Minimieren/Maximieren/Schließen; Wechsel per Chevron; Chat-Entwurf bleibt; nach dem Schließen ist der Chat-Tab aktiv |
| US3   | „+“ → „Chat“ im selben Fenster (Chat existiert schon)                                                                                                          | Kein zweiter Tab; vorhandener Tab wird aktiv                                                                                                                                |
| US3   | Alle drei Apps als Tabs öffnen, Fenster verschmälern                                                                                                           | Leiste scrollt (Pfeile), aktiver Tab bleibt sichtbar, „+“ bleibt rechts der Leiste sichtbar, Chevron listet alle Tabs                                                       |
| US3   | Mit Tab-Taste in die Leiste, Pfeiltasten benutzen                                                                                                              | Fokus wandert durch Tabs, Eingabe aktiviert; „+“ und Chevron sind erreichbar                                                                                                |
| US4   | Zwei weitere Arbeitsbereiche anlegen, Fenster in den zweiten verschieben, wechseln, den mittleren von drei löschen; den einzigen Arbeitsbereich löschen wollen | Fenster nur im gewählten Arbeitsbereich sichtbar; Löschen fragt nach; der dritte heißt danach „Arbeitsbereich 2“; letzter nicht löschbar                                    |
| US4   | Chat-Freigabe in Arbeitsbereich 2 auslösen, zu Arbeitsbereich 1 wechseln                                                                                       | Aufmerksamkeitshinweis an Arbeitsbereich 2                                                                                                                                  |
| US5   | Layout mit 2 Arbeitsbereichen, Fenster mit 2 Tabs, maximiertem Fenster einrichten → App beenden → Vault erneut öffnen                                          | Alles wiederhergestellt (Geometrie, Tabs, aktiver Tab, Maximierung, aktiver Arbeitsbereich); Chat-Tab beginnt mit neuer Unterhaltung                                        |
| US5   | Vault-Datei auf ein zweites Gerät/Profil kopieren und öffnen                                                                                                   | Nur ein Standard-Arbeitsbereich, keine Fenster                                                                                                                              |
| US5   | Anwendungsfenster verkleinern und neu öffnen (Geometrie passt nicht mehr)                                                                                      | Fenster liegt vollständig im sichtbaren Bereich                                                                                                                             |
| US6   | Anwendungsfenster unter 768 px Breite ziehen                                                                                                                   | Fenster füllen den Bereich, Tab-Leiste ist Titel + „+“ + Chevron, Maximieren/Ziehen entfallen; beim Vergrößern kehrt die alte Geometrie zurück; kein Inhalt geht verloren   |
| US7   | Nur automatisiert: `check:shell-state` mit Testapp `multiInstance: true` (zweimal als Tab, zweimal als Fenster)                                                | Unabhängige Instanzen, alle in Persistenz und Wiederherstellung                                                                                                             |
| alle  | Sprache DE/EN wechseln                                                                                                                                         | Alle neuen Texte übersetzt, kein roher Schlüssel (SC-008); `jq`-Vergleich der Schlüssel von `de.json`/`en.json` ohne Abweichung                                             |

## 3. Vor dem Merge

- `plans/README.md` enthält einen Eintrag für die Shell-Phase (research R17).
- Migrationsnummer und `HAEX_TRIGGER_VERSION` gegen `main` und die Worktrees 013/014
  abgleichen (research R3): wer zuerst mergt, behält `0019`/`11`.
- `CONTEXT.md` um Shell, App, Fenster, Tab und Launcher ergänzt.
