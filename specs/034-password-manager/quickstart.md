# Quickstart: Passwortmanager

**Spec**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md)

Validierungsleitfaden, kein Bauplan. Jeder Abschnitt nennt Voraussetzung, Aufruf und
erwartetes Ergebnis. Rust-Aufrufe brauchen die Umgebung aus dem Memory „cargo in HaexVault
worktrees“ (`nix develop`, Host-Bridge, `-j 4` und nur gezielte Testziele); im Worktree gilt
ein echtes `pnpm install` (kein Symlink auf `node_modules`). Beispieldateien und Geheimnisse in
Tests sind erfunden; echte Passwörter gehören nie in Fixtures (Constitution I).

## 1. Automatische Prüfungen (CI-gleich)

```bash
pnpm check:passwords          # neu: Generator, Suche, Orte und Sitzungs-Stichprobe, Aktionen
pnpm check:agent-actions      # Regression: Namensregel für Geheimnisse, Schnappschuss tools.json
pnpm check:wm-navigation
pnpm check:templates
pnpm typecheck && pnpm typecheck:scripts && pnpm lint && pnpm format:check
cargo test --manifest-path src-tauri/Cargo.toml passwords
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features passwords
cargo test --manifest-path src-tauri/Cargo.toml --test passwords_roundtrip --test passwords_access --test passwords_sync --test passwords_import
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
pnpm generate:ts-types        # danach darf git diff src/types/bindings leer sein
pnpm export:eval-tools        # danach darf git diff src-tauri/src/chat/eval/tools.json leer sein
python3 scripts/ci/check-docs.py
```

Erwartet: alles grün. `check:passwords` schlägt fehl, wenn ein Ort einen Titel oder Wert in
Pfad oder Abfrage trägt, wenn der Generator eine gewählte Klasse auslässt oder wenn die Suche
ein Geheimnis oder eine Notiz berücksichtigt.

## 2. Datenmodell und Migration (FR-036, FR-037)

`cargo test … migrations` und `--test vault_upgrade`: Eine frische Vault und eine Vault vor
`0022` haben danach alle zwölf Tabellen und die Trigger (`HAEX_TRIGGER_VERSION` 14); die
Spalten entsprechen [data-model.md](./data-model.md), `binaries.data` ist `BLOB`, kein
UNIQUE auf Tagname, Credential-ID und Tag-Paar.

## 3. Einträge, TOTP, Zwischenablage (US1)

Automatisch: `totp_tests.rs` rechnet die Testvektoren der RFC 6238 für SHA-1, SHA-256 und
SHA-512 mit 8 Ziffern; ungültige Secrets, Ziffern, Perioden und Algorithmen liefern `InvalidInput`
mit Feldnamen (SC-003). `passwords_roundtrip.rs` prüft Anlegen, Ändern und Lesen mit den
Geheimnis-Regeln (Teil-Update lässt ein nicht gesendetes Passwort unverändert). Ein ungültig in
die Tabelle geschriebenes TOTP-Secret (simulierter Sync) meldet `otpState: invalid`, der Code-Aufruf
liefert `InvalidInput`, und Ersetzen sowie Entfernen funktionieren; `NULL` bei Ziffern, Periode und
Algorithmus gilt als 6, 30, `SHA1`.

Manuell (`pnpm tauri:dev`, Vault öffnen, App „Passwörter“ aus dem Starter):

1. Eintrag mit Titel, Benutzername, Passwort und TOTP-Secret `JBSWY3DPEHPK3PXP` anlegen.
   Erwartet: in der Liste, nach Neustart von HaexVault wieder da, Passwort verdeckt.
2. Eintrag öffnen: Code mit Restzeit wechselt ohne Zutun; „Kopieren“ beim Code und beim
   Passwort; nach der eingestellten Zeit ist die Zwischenablage leer (Wert vorher in ein
   Textfeld einfügbar). Einstellung auf 15 s stellen: gilt ohne Speichern-Knopf.
3. Tab-Verlaufsliste und Fenstertitel ansehen: kein Titel eines Eintrags, kein Wert.
4. „Abgelaufen“-Kennzeichnung: Ablaufdatum in der Vergangenheit setzen.
5. Ein TOTP-Secret mit Tippfehler (`JBSWY3DPEHPK3PX!`) speichern: abgelehnt mit Meldung am Feld.
6. Passkeys eines Eintrags (aus einem Import, siehe §9) werden mit Relying Party und Nutzer
   angezeigt, der Spitzname ist änderbar, das Löschen entfernt den Passkey.

## 4. Ordnen (US2)

Zwei verschachtelte Ordner anlegen, drei Einträge verschieben, Tags vergeben, per
Mehrfachauswahl zwei verschieben, nach Tag filtern. Ordner in den eigenen Unterordner ziehen:
abgelehnt. Tag umbenennen in einen vorhandenen Namen: abgelehnt. Ordner mit „nach oben“ /
„nach unten“ und per Ziehen umsortieren: die Reihenfolge bleibt nach Neustart erhalten.
`groups_tests.rs`, `trash_tests.rs` und `tags_tests.rs` decken Zyklus, Reihenfolge, Tag-Kennungen
(`fold`, auch zerlegte Umlaute) und das Zusammenführen ab.

## 5. Generator (US3)

`pnpm check:passwords`: 1.000 Ausgaben je Konfiguration erfüllen Länge und Klassen (SC-004);
Muster- und Ausschlussfälle; nicht erfüllbare Auswahl liefert einen Fehlercode statt eines
leeren Wertes. Manuell: Voreinstellung speichern, als Standard setzen, Generator öffnen:
vorausgewählt.

## 6. Papierkorb und Verlauf (US4)

`trash_tests.rs`, `snapshots_tests.rs`: Löschen, Wiederherstellen an den früheren Ort (und
an die Wurzel, wenn der Ordner fehlt), endgültiges Löschen mit Kindern, Verlaufsstand nach
jeder Änderung, kein Stand bei unveränderten Werten, Wiederherstellen erzeugt einen neuen
Stand. Manuell: Passwort ändern, im Verlauf den alten Stand öffnen (Passwort verdeckt, per
„Anzeigen“ sichtbar), wiederherstellen.

## 7. Anhänge (US5)

`binaries_tests.rs`: Hash über Rohdaten, Deduplizierung, 25-MiB-Grenze (Datei vorher geprüft,
26 MiB wird abgelehnt, ohne zu lesen), Karenzzeit beim Aufräumen (`orphaned_at` wird beim
Verwaisen gesetzt: eine alte Binärzeile, die erst jetzt verwaist, bleibt, eine seit acht Tagen
verwaiste verschwindet; ein verspäteter Verweis setzt `orphaned_at` zurück; ein eigenes Symbol,
das ein Eintrag, ein Ordner, ein Passkey oder ein Verlaufsstand noch nennt, bleibt auch nach acht
Tagen, ein nicht mehr genanntes verschwindet). Manuell: dieselbe Datei an zwei Einträge hängen,
herunterladen und mit `cmp` gegen das Original vergleichen (SC-009), Bild-Vorschau, PDF nur
Herunterladen, 26-MiB-Datei zeigt die Meldung.

## 8. Zugriff und Berechtigungen (US6)

`access_tests.rs` (Tabelle der Fälle aus [contracts/access.md](./contracts/access.md)) und
`tests/passwords_access.rs`: Liste ohne Geheimnisse (der markierte Wert `SECRET-MARKER-…` darf in
keiner serialisierten Antwort stehen, SC-005), Freigabe `Read` für Tag `s3`, Schreiben und
Herausschreiben abgelehnt (SC-006), `BuiltinAgent` sieht nur `AgentHeader`; ein Eintrag im
Papierkorb mit Tag im Bereich ist für Aufrufer von außen nicht lesbar, änderbar oder löschbar und
bleibt im Papierkorb (Z13). Manuell im Chat
mit einem Modell: „Welche Einträge habe ich zu GitHub?“ → Treffer mit Titel und Tags; „Zeig mir
das Passwort“ → das Modell kann es nicht lesen, es gibt keine Aktion dafür.

## 9. Import (US7)

`passwords_import.rs` mit den Beispieldateien aus `src-tauri/tests/fixtures/passwords/` und der zur
Laufzeit gebauten KDBX-Datei (`tests/common/kdbx_fixture.rs`, Passwörter erfunden, Schlüsselpaare
im Test erzeugt):

- Zahl der Einträge und Ordner stimmt mit der Quelle, **der Papierkorb inbegriffen** (SC-010);
  nichts geht still verloren: jede Quelleinheit ist importiert, als Doppeltes übersprungen oder hat
  eine Zeile im Bericht.
- Papierkorb (KeePass, Bitwarden `deletedDate`) liegt im Papierkorb von HaexVault, Verlauf (KeePass,
  `passwordHistory`) steht als Verlaufsstände mit den Zeiten der Quelle da, Symbole sind gesetzt
  (Standardsymbol als Name, eigenes als Bild), Karten-, Identitäts- und SSH-Angaben und alles ohne
  eigenes Feld stehen als eigene Felder oder Tags am Eintrag.
- Passkeys mit ES256, EdDSA und RS256: der abgeleitete öffentliche Schlüssel stimmt mit dem
  erzeugten überein; ein anderer Algorithmus wird gespeichert, sein öffentlicher Schlüssel bleibt
  leer, der Bericht nennt ihn; ein unlesbarer Schlüssel stoppt den Eintrag nicht; ein Passkey mit
  schon vorhandener Credential-ID wird nicht doppelt angelegt und steht im Bericht
  (`passkey_duplicate`); ein gelöschter Bitwarden-Eintrag merkt seinen früheren Ordner.
- Ungültiges TOTP wird wie es ist importiert und am Eintrag als ungültig angezeigt.
- Ein Anhang von 26 MiB wird abgelehnt, der Eintrag ohne ihn importiert, der Bericht nennt Eintrag,
  Ordnerpfad, Dateiname und Größe.
- Falsches Passwort, beschädigte Datei und ein verschlüsselter Bitwarden-Export ändern nichts; ein
  Gesamtfehler (eingespritzt beim fünften Eintrag und bei einem Anhang) und ein Abbruch entfernen
  alles Angelegte; Doppelte erscheinen in der Vorschau, `onDuplicate: skip` überspringt sie;
  Zeilenumbrüche in Notizen bleiben erhalten; der Bericht enthält keinen `SECRET-MARKER-IMPORT`.

Manuell: den Assistenten mit einer eigenen Exportdatei ausprobieren; im Bericht „Hier musst du
nacharbeiten“ den Knopf „Eintrag öffnen“ und „Bericht als Textdatei speichern“ prüfen.

## 10. Sync zwischen eigenen Geräten (US8)

`tests/passwords_sync.rs` mit zwei und drei Geräten (Fixture `sync_helpers.rs`): Anlegen
erscheint auf dem anderen Gerät; gleichzeitige Änderung verschiedener Felder bleibt erhalten
(SC-008); Löschen eines Eintrags **mit Kindern** (Tags, Felder, Anhänge, Verlauf) kommt als
Löschung je Zeile beim Peer an und ein später eintreffender alter Stand erweckt ihn nicht;
dasselbe Tag unabhängig auf beiden Geräten angelegt ergibt eine Zeile; ein 25-MiB-Anhang
kommt vollständig an. Manuell nach Spec 033 mit dem Mehrgeräte-Rahmen: zwei Geräte, ein
Eintrag, Änderung sichtbar in wenigen Sekunden, grob unter zehn (SC-007).

Hinweis: Nach Migration `0022` synchronisieren Geräte erst, wenn **alle** auf dem neuen Stand
sind (Handshake, Spec 024).

## 11. Schmale Fenster und Sitzung (FR-039, FR-041)

Fenster auf 360 px verkleinern: Seitenleiste als Überlagerung, kein waagerechtes Scrollen,
alle Aktionen erreichbar. Sitzungswiederherstellung einschalten, Passwortmanager mit
aufgedecktem Passwort offen lassen, HaexVault neu starten: der Tab kommt mit Ort und Verlauf
zurück, das Passwort ist verdeckt, die gespeicherte Sitzung enthält keinen Wert
(`check-passwords-routes.ts` prüft die Stichprobe gegen `snapshotSession`).

## 12. End-to-End (SC-012)

```bash
pnpm test:e2e passwords-basic          # anlegen, suchen, TOTP-Code, Papierkorb
pnpm test:e2e passwords-sync-two-devices
pnpm test:e2e passwords-narrow-window
pnpm test:e2e passwords-session-restore
```

Erwartet: alle vier bestehen gegen die gebaute App (SC-012); sie brauchen weder Netz noch
Zeitmessung. Ordnen, Verlauf, Generator, Anhänge und Import prüfen §4 bis §9 über Integrationstests
und manuell; Anhänge und Import gehen über Dateidialoge und sind nicht Teil der E2E-Szenen.
