# Quickstart: Adressbuch prüfen

Prüfschritte für Spec 051. Automatisch laufen die Rust-Tests, Vitest und die E2E-Szenarien; die
Schritte M1–M7 sind von Hand auf echter Hardware.

## Voraussetzungen

- devShell (`nix develop`), `corepack pnpm install` in der Worktree.
- Android: `holzi-emulator -no-window -no-snapshot-save` im Hintergrund, `ANDROID_SERIAL=emulator-5554`.
- Zwei Vaults für Visitenkarten (zwei Desktop-Instanzen oder Desktop + Emulator).

## Automatisch

```sh
# Rust, nur die betroffenen Ziele
cargo test -j 4 --manifest-path src-tauri/Cargo.toml --lib contacts::
cargo test -j 4 --manifest-path src-tauri/Cargo.toml --lib extensions::contacts
cargo test -j 4 --manifest-path src-tauri/Cargo.toml --lib platform::

# Frontend
corepack pnpm typecheck && corepack pnpm lint && corepack pnpm format:check
corepack pnpm check:contacts

# E2E Desktop
corepack pnpm test:e2e --grep contacts

# E2E Android
ANDROID_SERIAL=emulator-5554 corepack pnpm test:e2e --platform android --grep contacts
```

Unbekannte Arten (US1-6) prüfen der Rust-Test `values_of_unknown_kinds_survive_and_stay_read_only`
und `scripts/check-contacts-draft.ts`, weil kein Command eine unbekannte Art schreibt.

Erwartet: alles grün; `pnpm check:e2e-exclusions` ohne neue Ausnahme.

## Szenarien der E2E-Läufe (Bezug zur Spec)

| Szenario                       | Prüft                                                         |
| ------------------------------ | ------------------------------------------------------------- |
| `contacts-basic`               | US1: anlegen, eigene Angabe, suchen, ändern, löschen          |
| `contacts-two-devices`         | US1-4: Änderung und Löschung kommen auf dem zweiten Gerät an  |
| `android-contacts-import`      | US2-1/2: Übersicht, 2 Konten, Anzahl und Inhalt               |
| `android-contacts-denied`      | US2-6: Berechtigung verweigert, Erklärung                     |
| `android-contacts-two-way`     | US2-3/4/5, US3-1/2/3: Änderungen in beide Richtungen, Ziele   |
| `android-contacts-targets`     | US3-4/5: Ziel entfernen (löschen/behalten), Mehrfachauswahl   |
| `android-contacts-mass-delete` | FR-016: Konto weg, Berechtigung weg, > 10 % gelöscht          |
| `contacts-card`                | US4-1/2/3/4/5: Karte zeigen, einlesen, ungültig, älter, neuer |
| `extension-contacts`           | US5: ohne, mit, nur Gruppe                                    |
| `agent-contacts`               | US6-1/2/3 über den eingebauten Agenten                        |

Kontakte für Android-Szenarien legt `PhoneControls.insertContact` über
`adb shell content insert --uri content://com.android.contacts/raw_contacts …` an; jedes Szenario
räumt seine Roh-Kontakte am Ende wieder ab (`pm clear` löscht sie nicht).

## Von Hand (echte Geräte)

- **M1** Telefon mit Google-Konto (≥ 300 Kontakte) und SIM-Kontakten: Abgleich einschalten, Übersicht
  plausibel, unsichere Paare entscheiden; Dauer < 60 s bei 2.000 Kontakten (SC-001).
- **M2** Kontakt in der Kontakte-App des Telefons ändern, holzi in den Vordergrund holen: Änderung
  < 5 s sichtbar (SC-002). Danach in holzi ändern: steht in Google Kontakte im Browser.
- **M3** Messenger (Signal, WhatsApp) auf dem Telefon: Personen erscheinen nicht doppelt; Änderungen
  landen im Google-Roh-Kontakt, nicht im Messenger-Konto.
- **M4** Firmenkonto (Exchange) oder anderes nur lesbares Konto: Änderung aus holzi bleibt in holzi,
  Hinweis am Kontakt (FR-025).
- **M5** Google-Konto vom Telefon entfernen: keine Löschung im Adressbuch, Abgleich hält an (FR-026).
- **M6** Zweites Telefon mit demselben Google-Konto: keine Doppel (SC-005).
- **M7** Visitenkarte als QR-Code von Telefon zu Telefon scannen (SC-009, Kamera-Berechtigung).
