# Quickstart: holzi als MCP-Server prüfen

**Feature**: [spec.md](./spec.md) | **Contracts**: [contracts/](./contracts/)

Automatische Prüfungen laufen in CI; diese Anleitung beschreibt die Prüfungen von Hand (M1–M10)
mit echten Agenten. Kommandos laufen im devShell (siehe `CLAUDE.md`).

## Voraussetzungen

- holzi aus diesem Branch: `nix develop --command pnpm tauri:dev`
- eine Test-Vault mit ein paar Tabs, einer installierten Test-Erweiterung mit Agent-Werkzeugen
  (`haextension-test-bundles`), drei Passwort-Einträgen mit Tag `dev` und einem ohne Tag
- Claude Code und Codex in aktuellen Versionen im `PATH`
- für M9 ein Android-Emulator (`nix develop .#holzi-emulator`) mit einem MCP-Client, der HTTP und
  OAuth spricht

## Automatisch

```sh
nix develop --command scripts/with-nix-host-bridge.sh cargo test -p holzi --test mcp_server --test mcp_server_contract -j 4
nix develop --command scripts/with-nix-host-bridge.sh cargo test -p holzi mcp_server:: -j 4
nix develop --command pnpm check:agent-actions
nix develop --command pnpm typecheck && nix develop --command pnpm lint
```

Erwartet: alles grün; der Vertragstest listet je Rechtegruppe einen Fall (research R3).

## Von Hand

| #   | Schritt                                                                                                                                  | Erwartet                                                                                                    | Spec            |
| --- | ---------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------- | --------------- |
| M1  | „KI → Agenten“, Zugang einschalten, Anleitung für Claude Code kopieren und im Terminal ausführen, `claude` starten                       | Registrierungsdialog „Claude Code (Angabe des Agenten)“, ab Werk „holzi bedienen: lesen“, Manuell           | US1, FR-006/009 |
| M2  | Registrieren; in Claude Code „Welche Tabs sind offen?“                                                                                   | Werkzeuge erscheinen ohne Neustart; Rückfrage in holzi (Manuell); nach Erlauben stimmt die Antwort          | US1, US3        |
| M3  | Claude Code bitten, das Farbschema zu ändern und einen Passwort-Eintrag zu lesen                                                         | Werkzeuge fehlen in der Liste; erzwungener Aufruf ergibt `not_allowed`                                      | US2             |
| M4  | Für Codex dasselbe mit „Einstellungen: lesen und ändern“, Auto; Farbschema ändern, dann eine Unterhaltung löschen lassen                 | Farbschema ohne Rückfrage; Löschen fragt; Ablehnen → `user_declined`                                        | US3             |
| M5  | Claude Code die Test-Erweiterung freigeben und ein Werkzeug nutzen; zweite Erweiterung bleibt unsichtbar                                 | nur Werkzeuge der freigegebenen Erweiterung                                                                 | US4             |
| M6  | Passwörter „Lesen, Tag dev“ freigeben, Freigabestufe Auto; Einträge listen, Passwort eines dev-Eintrags abrufen                          | nur dev-Kopfdaten; Rückfrage mit Hinweis „geht an Agent und Anbieter“ trotz Auto                            | US5, FR-019     |
| M7  | Im Chat Claude Code als Delegate wählen (ohne Registrierung), dann „Rechte festlegen“, Delegate auf „Ungated“, Einstellung ändern lassen | erst Hinweis, nach Registrierung Werkzeuge; Rückfrage erscheint im Chat trotz Ungated                       | US6, FR-024/025 |
| M8  | Protokoll von Codex öffnen; Codex widerrufen, während es verbunden ist                                                                   | Einträge mit Ziel und Eingaben („Farbschema → dunkel“); Codex verliert die Verbindung in < 1 s              | US7, SC-004     |
| M9  | Android: Zugang einschalten, HTTP-Anleitung, MCP-Client verbinden                                                                        | Autorisierungsseite „In holzi bestätigen“, Dialog in holzi, danach Werkzeuge                                | FR-003, R1      |
| M10 | Im Browser eine Seite öffnen, die `fetch` auf den HTTP-Port macht; Vault schließen und Claude Code neu starten                           | `403`, kein Dialog; nach dem Schließen meldet Claude Code „holzi ist nicht offen“, kein Fenster öffnet sich | US8, FR-002/004 |

Ablauf nach 90 Tagen (R9) wird automatisch mit injizierter Uhr geprüft, nicht von Hand.
