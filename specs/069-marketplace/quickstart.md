# Quickstart: Marketplace prüfen

Zeigt, dass die Funktion von Anfang bis Ende trägt. Vorbedingungen: der Worktree dieser Spec, die
nix-Devshell (`cargo` nur dort, Memory `holzi-cargo-in-worktrees`), ein Test-Marketplace (unten).

## Automatisiert

Immer gezielt bauen (`-j 4`, nur das betroffene Ziel), nie den ganzen Baum:

```sh
nix develop --command cargo test -j 4 -p holzi marketplace::                  # Kern, Download, Zugangsdaten
nix develop --command cargo test -j 4 -p holzi extensions::registry::install  # Quelle der Installation
pnpm lint:rust                                                                # Clippy mit -D warnings
pnpm typecheck && pnpm lint && pnpm check:templates && pnpm format:check
```

Die Logik (Zusammenführen, Fassungen, Kompatibilität, Zugangsdaten) liegt im Host; das Frontend hat
keine eigenen Einheitstests, sondern wird von Typprüfung, Lint und e2e abgedeckt.

E2E (Spec 016; Desktop mit Xvfb, Android im Emulator `holzi-emulator`):

```sh
nix develop --command scripts/with-nix-host-bridge.sh pnpm test:e2e --grep marketplace
```

Die Szenarien heißen `marketplace-*.test.ts`; `marketplace-two-devices.test.ts` deckt SC-007, die
übrigen SC-008.

## Test-Marketplace

Der Rig startet ihn selbst (`scripts/e2e/lib/marketplace.ts`, R15). Von Hand: ein Node-Server, der
`/health`, `/categories`, `/extensions`, `/extensions/{slug}`, `/extensions/{slug}/download` nach
[contracts/marketplace-api.md](contracts/marketplace-api.md) liefert, mit einem signierten Wegwerf-Bundle
(`haextension-test-bundles`). Zugangsdaten für den privaten Fall sind Zufallswerte beim Start.

## Manuell (Prüfpunkte, T-Nummern folgen in `tasks.md`)

| #   | Schritt                                                                                                                                                    | Erwartung                                                                                                                                                           |
| --- | ---------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| M1  | Neuer Vault, App „Erweiterungen entdecken“ öffnen                                                                                                          | Katalog der Voreinstellung sichtbar, Kategorien, kein Einrichten nötig (SC-006)                                                                                     |
| M2  | Suchen, Kategorie wählen, blättern                                                                                                                         | Liste folgt nach kurzer Pause, Seite springt auf 1 (US1-2, -3, -4)                                                                                                  |
| M3  | „Installieren“ bei einer kleinen, signierten Erweiterung aus dem Test-Marketplace                                                                          | Fortschritt, dann Vorschau mit Herausgeber, Berechtigungen und Quelle; nach „Installieren“ läuft sie                                                                |
| M4  | Vorschau abbrechen                                                                                                                                         | nichts installiert, Kachel unverändert                                                                                                                              |
| M5  | Ältere Fassung wählen, die neuere installiert ist                                                                                                          | Downgrade-Bestätigung                                                                                                                                               |
| M6  | Test-Marketplace mit falscher Prüfsumme / ohne Prüfsumme / Datei über 64 MiB                                                                               | Abbruch mit Meldung, nichts installiert (US2)                                                                                                                       |
| M7  | Zweiten Marketplace anlegen (auch mit Pfadanteil), ausschalten, einschalten, nach oben schieben, löschen; die Voreinstellung löschen                       | Katalog folgt sofort, beide Quellen gleichzeitig, jede Kachel nennt ihre Quelle; die Voreinstellung kommt nicht wieder, der Leerzustand bietet sie an (US4)         |
| M8  | Marketplace mit Bearer-Token und gewähltem Passwort-Eintrag; Eintrag im Passwortmanager löschen                                                            | Warnung vor dem Löschen nennt den Marketplace; Quelle zeigt „Zugangsdaten fehlen“ (US5)                                                                             |
| M9  | Adresse `http://` mit Anmeldung                                                                                                                            | Speichern wird abgelehnt (US5-6)                                                                                                                                    |
| M10 | Netzwerk trennen                                                                                                                                           | „Keine Verbindung“, Datei-Install bleibt benutzbar                                                                                                                  |
| M11 | Zwei Geräte: Marketplace auf A, danach B; Voreinstellung auf A ausschalten                                                                                 | erscheint auf B (SC-007); der Ausschalter bleibt auf B ausgeschaltet                                                                                                |
| M14 | Zwei Test-Marketplaces mit derselben Erweiterung (anderer Herausgeber)                                                                                     | zwei Kacheln mit Quelle; die Vorschau nennt den Herausgeberschlüssel aus dem Bundle (SC-009)                                                                        |
| M15 | Eine Erweiterung installieren, deren neuere Fassung der Test-Marketplace führt; im Katalog nach etwas anderem suchen; Einstellungen → Erweiterungen öffnen | die Zeile zeigt „Update verfügbar“, obwohl die Erweiterung auf der angezeigten Katalogseite nicht steht (US3-7)                                                     |
| M12 | Android: Katalog, Installieren, Zurück-Geste                                                                                                               | Raster einspaltig, Installation läuft; Zurück schließt zuerst die Vorschau                                                                                          |
| M13 | Windows und macOS einmal von Hand                                                                                                                          | wie M1 bis M4                                                                                                                                                       |
| M16 | Im öffentlichen Marketplace eine Fassung zum Installieren auswählen                                                                                        | mindestens eine Fassung besteht die Spec-017-Vorschau und lässt sich installieren; falls keine vorhanden ist, kompatible signierte Fassung vor PR C veröffentlichen |

## Erfolgsmaße im Blick

SC-001 (unter einer Minute), SC-002 und SC-004/SC-005 sind automatisierbar und gehören in die Rust-Tests;
SC-006, SC-007, SC-008 in e2e; M-Punkte decken Oberfläche und Plattformen.
