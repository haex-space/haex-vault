# Quickstart: External Bridge prüfen

**Spec**: [spec.md](./spec.md) | **Verträge**: [contracts/](./contracts/)

## Automatische Prüfungen

Alle Befehle laufen im devShell. Für cargo gilt `nix develop --command
scripts/with-nix-host-bridge.sh cargo …` (siehe Konstitution, holzi-devshell-commands).

| Was                                           | Befehl                                      | Belegt                               |
| --------------------------------------------- | ------------------------------------------- | ------------------------------------ |
| Umschläge, Herkunft, Kennung, Site, Erklärung | `cargo test --lib browser_bridge`           | FR-004 bis FR-006, FR-019            |
| Protokoll gegen eine Test-Vault               | `cargo test --test browser_bridge`          | SC-002 bis SC-004, SC-006 bis SC-009 |
| Wand zum Modell                               | `cargo test --test browser_bridge_contract` | FR-041                               |
| Passkey-Dienst mit UP/UV und neuem Eintrag    | `cargo test --test passwords_passkeys`      | FR-031, FR-033, FR-034               |
| Interoperabilität mit WebCrypto               | `pnpm check:browser-bridge`                 | R18                                  |
| Dialoge, Einstellungen, Widerruf im Fenster   | e2e-Szenario `browser-bridge`               | US1, US5, US4 (Dialog)               |

Danach folgt die übliche Runde: `cargo fmt --check`, `cargo clippy`, `pnpm typecheck`,
`pnpm lint`, `pnpm format:check`.

## Handprüfungen mit der echten Erweiterung

**Voraussetzungen**:

- holzi-Build aus dem Branch.
- Die Erweiterung aus haextension in der gepinnten Revision, gebaut für Chromium
  (`pnpm --filter haex-pass-browser build`, entpackt laden) und für Firefox
  (`EXTENSION=firefox`, als temporäres Add-on laden). Für M6 und M7 braucht es den Stand mit dem
  Begleit-PR (R19); ohne ihn landet jede Passkey-Anmeldung beim Browser.
- Eine Test-Vault mit diesen Einträgen:
  - `https://example.de` mit Tag `web`;
  - `https://www.example.de/login` mit Tag `web` und TOTP;
  - `https://bad-example.de` mit Tag `web`;
  - einer davon zusätzlich ohne Tag;
  - einer im Papierkorb.

| #   | Schritt                                                                                                                                                                                                                                | Erwartung                                                                                                                                                                                                                                               |
| --- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| M1  | Bridge in Einstellungen → Erweiterungen → Browser-Erweiterungen einschalten. In Chromium und in Firefox die Erweiterung öffnen. In holzi mit `RUST_LOG=holzi_lib::browser_bridge=debug` die angenommene Herkunft im Protokoll ablesen. | Zustand „läuft, Port 19455“. Der Dialog erscheint je Browser. Die Herkunft ist `chrome-extension://…` bzw. `moz-extension://…` (R2).                                                                                                                    |
| M2  | Mit „Einträge mit Tag web“, Standard-Tag `web` und „Erlaubnis merken“ zulassen. Browser und holzi neu starten.                                                                                                                         | Die Erweiterung zeigt „verbunden“, nach dem Neustart ohne Dialog (US1).                                                                                                                                                                                 |
| M3  | Seite `https://login.example.de` (lokal über `/etc/hosts` oder eine Testseite) mit Anmeldeformular und Einmalcode-Feld öffnen.                                                                                                         | Zwei Konten werden angeboten, `bad-example.de` und der Eintrag ohne Tag fehlen. Der Einmalcode wird gefüllt (US2).                                                                                                                                      |
| M4  | Im Popup ein Passwort erzeugen und einen Eintrag speichern.                                                                                                                                                                            | Die Standard-Voreinstellung wird benutzt. Der Eintrag erscheint sofort im Passwortmanager mit Tag `web` (US3).                                                                                                                                          |
| M5  | In den Einstellungen die Zulassung auf „Lesen“ ändern und erneut speichern. Einmal ablehnen, einmal erlauben und merken.                                                                                                               | Die Rückfrage erscheint. Nach Ablehnen scheitert das Speichern; nach Erlauben und Merken fragt holzi nicht mehr (US3 Szenario 4).                                                                                                                       |
| M6  | Auf `https://webauthn.io` (verlangt UP) mit haex-pass einen Passkey registrieren, dann anmelden; einmal mit „User Verification: required“.                                                                                             | Anwesenheitsbestätigung mit Gegenstelle und Herkunft. Registrierung und Anmeldung gelingen; bei „required“ wird das Vault-Passwort abgefragt (US4, SC-005).                                                                                             |
| M7  | Anwesenheitsbestätigung ablehnen, einmal 120 s verstreichen lassen.                                                                                                                                                                    | Die Seite bekommt einen Abbruch, kein Browser-Dialog. Kein Passkey entsteht.                                                                                                                                                                            |
| M8  | Bei offener Verbindung die Zulassung widerrufen. Danach eine Webseite mit `new WebSocket('ws://localhost:19455')` in der Konsole öffnen.                                                                                               | Die Erweiterung zeigt sofort „nicht verbunden“, und beim nächsten Verbinden fragt holzi (SC-007). Die Webseite bekommt keinen WebSocket, und kein Dialog erscheint (US6).                                                                               |
| M9  | haex-vault 2.x starten, dann in holzi die Bridge einschalten. Port auf 19456 ändern und in der Erweiterung ebenso.                                                                                                                     | „Port belegt“ mit Hinweis. Nach dem Wechsel verbindet die Erweiterung (US5 Szenario 4).                                                                                                                                                                 |
| M10 | Die Zeit von „Bridge einschalten“ bis zur ersten ausgefüllten Anmeldung messen.                                                                                                                                                        | Unter 2 Minuten (SC-001).                                                                                                                                                                                                                               |
| M11 | Die Vault schließen, während ein Dialog offen ist.                                                                                                                                                                                     | Der Dialog verschwindet, der Port ist in unter 2 s frei, und die Erweiterung zeigt „nicht verbunden“ (SC-008).                                                                                                                                          |
| M12 | In Firefox die Erweiterung zum ersten Mal verbinden, ohne gemerkte Zulassung. Dann einen Chromium-Build mit dem Namen der Erweiterung, aber einer anderen Kennung laden (zum Beispiel entpackt aus dem GitHub-Release).                | Firefox: Der Dialog sagt, dass holzi die Erweiterung dort nicht prüfen kann. Chromium: „nennt sich so“ mit Warnung, sobald T081 eine Herkunft in die Liste gebracht hat, sonst „nicht prüfbar“. Dieselben Wahlmöglichkeiten wie sonst (FR-045, SC-010). |
| M13 | Mit holzi nach PR G: haex-pass-browser 1.3.0 verbinden, dann 1.4.0.                                                                                                                                                                    | 1.3.0: kein Dialog; die Erweiterung zeigt nur „Verbindung geschlossen“, holzi zeigt den Hinweis zum Aktualisieren. 1.4.0: Dialog wie gewohnt (FR-048, FR-049).                                                                                          |
| M14 | Firefox auf Gerät A zulassen und merken. Dann Chrome auf Gerät A verbinden, dann einen Browser auf Gerät B mit derselben Vault. Den Fingerabdruck in den Optionen der Erweiterung mit dem Dialog vergleichen.                          | Für Chrome auf A und für B je ein eigener Dialog; der Fingerabdruck in den Optionen ist der im Dialog (FR-047, SC-012).                                                                                                                                 |

M1 gehört **vor** den Merge von PR B: Schickt ein Browser keinen `Origin`, wird der Plan angepasst
(R2).
