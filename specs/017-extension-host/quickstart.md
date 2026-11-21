# Quickstart: Erweiterungs-Host

**Spec**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md)

Validierungsleitfaden, kein Bauplan. Jeder Abschnitt nennt Voraussetzung, Aufruf und erwartetes Ergebnis und
die Lieferung (L1–L5, research R1), ab der er gilt. Rust-Aufrufe brauchen die Umgebung aus `nix develop` mit
Host-Bridge, `-j 4` und nur gezielte Testziele; im Worktree ein echtes `pnpm install`. Test-Bundles werden
mit einem eigens erzeugten Testschlüssel signiert, der nur in Fixtures vorkommt (Constitution I: kein
produktiver Schlüssel in Git).

## 0. Vorarbeiten (L0)

- haex-crdt: PR mit `write_guarded`/`read_guarded` (`SqlGuard`), Triggern nach Schemaänderungen,
  Schema-Modus, metadatentreuem Umbau, Spaltennamen ohne Zeilen und lokalem Modus gemerged; HaexVault pinnt die
  volle Revision in `src-tauri/Cargo.toml`.
- vault-sdk: PR mit Format v2 (`haex sign`, `haex verify`), Testvektoren und Prüfung von `event.source`
  gemerged; HaexVault verweist auf die volle Revision; Testvektoren liegen mit Herkunftskopf in
  `src-tauri/tests/fixtures/extension_bundles/`.

```bash
haex verify fixtures/good-minimal.xt          # gültig
haex verify fixtures/bad-moved-content.xt     # file_mismatch
```

## 1. Automatische Prüfungen (CI-gleich, ab L1)

```bash
pnpm check:extensions          # neu: Brücke, Warteschlange, App-Liste, Shim-Abbildung
pnpm check:wm-navigation && pnpm check:wm-state && pnpm check:settings
pnpm check:templates
pnpm typecheck && pnpm typecheck:scripts && pnpm lint && pnpm format:check
cargo test --manifest-path src-tauri/Cargo.toml -j 4 extensions
cargo test --manifest-path src-tauri/Cargo.toml -j 4 --no-default-features extensions
cargo test --manifest-path src-tauri/Cargo.toml -j 4 --test extension_bundle_format --test extension_sql_bypass \
  --test extension_sql_exec --test extension_migrations --test extension_bridge_contract
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
pnpm generate:ts-types         # danach git diff src/types/bindings leer
```

Erwartet: alles grün. `extension_sql_bypass` läuft jeden Fall dreimal (nur Vorprüfung, nur Authorizer, beide)
und scheitert, wenn eine Schicht allein einen Fall durchlässt ([contracts/sql-policy.md](./contracts/sql-policy.md)).
`extension_bridge_contract` scheitert, wenn eine Methode Chat- oder Modellcode erreicht (FR-009).

## 2. Installieren und öffnen (US1, L1)

1. Einstellungen → Erweiterungen → „Aus Datei installieren“ → `good-notes-like.xt`.
   Erwartet: Name, Fassung, Herausgeber-Kürzel, „Signatur gültig“, alle erklärten Berechtigungen, abwählbar.
2. Installieren, im Launcher öffnen. Erwartet: Tab mit der Erweiterung, Theme und Sprache wie HaexVault.
3. In der Erweiterung navigieren, in HaexVault Zurück. Erwartet: Ort im Verlauf, Zurück führt dorthin
   (Spec 020); Tastenkürzel von HaexVault wirken mit Fokus im Rahmen.
4. HaexVault neu starten. Erwartet: der Tab kommt am Ort zurück (Spec 022).
5. `bad-moved-content.xt`, `bad-extra-file.xt`, `legacy-format.xt` installieren. Erwartet: Ablehnung mit
   Grund, nichts installiert; beim alten Format der Hinweis auf `haex`.
6. Eine Datei im gespeicherten Bundle per Test-Haken verändern, Erweiterung öffnen. Erwartet: startet nicht
   (FR-003).

## 3. SQL und Migrationen (US2, L1)

`cargo test … --test extension_sql_exec --test extension_migrations`. Erwartet: Tabellen mit Präfix angelegt,
Einfügen mit `RETURNING`, Ändern, Lesen, Löschen; Transaktion ganz oder gar nicht; zweiter Start wendet nichts
erneut an; eine Migration mit `CREATE VIEW` oder einer Kerntabelle wird ganz abgelehnt und die Erweiterung
startet nicht; Trigger nach `ADD COLUMN` und Umbau vorhanden; `_no_sync`-Tabellen ohne Sync-Spalten und ohne
Löschmarke; Grenzen (Zeilen, Laufzeit) brechen mit 7000 ab und rollen zurück.

Manuell: haex-notes (gebaut mit `haex` v2) installieren, Notizbuch und Seite anlegen, neu starten, Daten da.

## 4. Berechtigungen (US3, L1)

Test-Bundle `perm-probe.xt` fragt eine nicht erklärte Berechtigung an.

1. „Erlauben“ ohne „Merken“ → gilt bis zum Schließen der Vault.
2. „Erlauben“ mit „Merken“ → gilt nach Neustart; in den Einstellungen als vault-weit sichtbar.
3. Dateisystem „Merken“ → in den Einstellungen als „alle Geräte“; Shell „Merken“ → als dieses Gerät. Keiner
   der Dialoge bietet eine Wahl des Geltungsbereichs.
4. Zehn gleiche Anfragen schnell hintereinander → eine Anfrage, eine Entscheidung für alle.
5. In den Einstellungen auf „verweigert“ → die nächste Anfrage scheitert sofort, auch im offenen Tab.

## 5. Abschottung (FR-009–FR-011, L1)

End-to-End `extension-isolation`: Aus dem Rahmen scheitern Zugriffe auf `parent.document`,
`localStorage` von HaexVault, `fetch('https://…')`, `new WebSocket(…)`, `<img src="https://…">`,
`location = 'https://…'`, `window.open`, Zugriffe auf `__TAURI_INTERNALS__`/`ipc:`; ein Geschwisterrahmen,
der `port:init` an einen anderen Rahmen schickt, bekommt keinen Kanal; ein Rahmen, der zur Seite einer anderen
Erweiterung navigiert, wird nicht ausgeliefert (Start-Token).

## 6. Mehrere Geräte (US4, L2)

Mehrgeräte-Rahmen aus Spec 033. `cargo test … --test sync_extension_parking --test extension_lifecycle_sync`
und End-to-End `extension-two-devices`:

1. Auf A installieren, auf B synchronisieren → B prüft selbst und zeigt die Erweiterung; Daten von A da.
2. Zeilen vor den Tabellen ankommen lassen (Test-Haken) → B parkt die Gruppe, andere Daten kommen weiter an;
   nach den Migrationen sind die Zeilen da.
3. Bundle auf B per Test-Haken verfälschen → B startet nicht, zeigt den Grund, Sync läuft weiter.
4. Update auf A → B wendet die neuen Migrationen an; ein offener Tab lädt neu.
5. Shell-Berechtigung auf A → B fragt erneut; eine Dateiberechtigung von A gilt auf B.
6. Auf A „entfernen, Daten löschen“ → auf B Tabellen weg, keine späten Zeilen aus alten Gruppen.

## 7. Meldungen, Kontext, Speicher, fremde Tabellen, Lebenszyklus, Entwicklermodus (US5–US7, US12, L3)

- Zwei Tabs, ein zweites Gerät, eine zweite Erweiterung mit und eine ohne Leseberechtigung ändern Tabellen →
  nur die berechtigten Meldungen kommen an, ohne Namen fremder Tabellen.
- Theme umschalten → Kontext-Meldung. Speicherwert setzen, neu starten → Wert da, für andere unsichtbar.
- Wochenübersicht-Test-Bundle liest mit „Lesen“ Tabellen von haex-calendar, Schreiben fragt, Kerntabelle
  wird ohne Rückfrage abgelehnt.
- Update, Downgrade mit Bestätigung, Deaktivieren (Tabs zu, Funktionen 8002), Entfernen mit „Daten behalten“
  und Neuinstallation (Daten wieder da).
- Entwicklermodus an (Hauptfenster lädt neu), Projektordner wählen, Erweiterung von `localhost` laden →
  Tab gekennzeichnet, HMR wirkt, Konsolenausgabe sichtbar, Anfrage für Berechtigung erscheint; Tabellen ohne
  Sync-Spalten; Laden einer installierten Erweiterung wird abgelehnt.

## 8. Netzwerk, Benachrichtigungen, Dateien (US8, US9, L4)

`cargo test … --test extension_web` und `cargo test … --lib extensions::fs`: Weiterleitung auf ein nicht
berechtigtes Ziel fragt bzw. scheitert; Methode wird geprüft; Symlink und `..` aus einem freigegebenen Ordner
werden am Ziel geprüft; Vault-Datei und App-Daten immer gesperrt; Dialog-Auswahl ohne weitere Rückfrage;
Beobachten meldet jeden Pfad eines Bündels nur an die eigene Erweiterung. Manuell: haex-calendar zeigt eine Erinnerung, Klick bringt den Tab
nach vorn (Linux; macOS und Windows je nach Ergebnis der Machbarkeitsprüfung).

## 9. Passwörter, entfernter Speicher, Mail, Shell (US10, US11, L5)

- Mit 034: Freigabe für Tag `haex-calendar`, CalDAV-Konto anlegen und lesen; fremdes Tag abgelehnt. Ohne 034: 8001.
- Lokaler Test-Mailserver: Postfächer, Nachricht, Flags, Senden mit Berechtigung für `host:port`; anderer Port
  fragt. Beobachten endet mit dem letzten Tab.
- Shell mit Berechtigung für `/bin/sh`: `echo`, Größe ändern, Tab schließen → Prozessgruppe beendet; `/bin/bash`
  ohne Berechtigung fragt.

## 10. Android und iOS (FR-066, sobald die Ziele bestehen)

- Android-Emulator mit dem wry-Fork aus L0, iOS-Simulator: End-to-End `extension-isolation` mobil — im Rahmen
  fehlt `__TAURI_INTERNALS__`, ein Aufruf über `window.ipc` scheitert, alle übrigen Fälle aus Abschnitt 5
  gelten.
- haex-notes installieren und benutzen wie in Abschnitt 2 und 3; Datei über die System-Auswahl speichern;
  Shell und Beobachten von Ordnern antworten 8001.

## 11. Erfolgskriterien

SC-001 manuell mit haex-notes und haex-calendar (gebaut mit `haex` v2; haex-pass ist durch Spec 034 abgelöst); SC-002 über
`extension_sql_bypass` und `extension-isolation`; SC-003 über `extension-two-devices`; SC-005 und SC-006
grob im Betrieb (keine Messaufgaben); SC-007 über die Testtabelle in `extension_bridge_contract`.
