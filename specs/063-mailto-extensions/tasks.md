---
description: 'Aufgabenliste: Extensions als Öffner für E-Mail-Links'
---

# Aufgaben: Extensions als Öffner für E-Mail-Links

**Input**: `/specs/063-mailto-extensions/` mit plan.md, spec.md, research.md, data-model.md, contracts/, quickstart.md

**Tests**: Ja. Das Projekt prüft jede Regel mit einem Test (`*_tests.rs` neben dem Modul, Node-Checks, E2E).

**Organisation**: nach Geschichten. Geschichte 3 (Anmeldung) steht vor Geschichte 1, weil es ohne Anmeldung keinen Öffner
gibt; Geschichte 2 baut auf dem Deep-Link-Weg aus Spec 060 auf (gemergt).

## Format: `[ID] [P?] [Story] Beschreibung`

- **[P]**: kann parallel laufen (andere Dateien, keine Abhängigkeit von offenen Aufgaben)
- **[US1–US3]**: Geschichte der Spec
- Aufgaben in anderen Repos tragen `[vault-sdk]` oder `[haextension]` vor dem Pfad. Verweise auf `haextension`
  zitieren `haex-space/haextension@b05d0e039f00122bb4abb7079149fcdb1d663e39:<Pfad>`, auf `vault-sdk` die Revision
  `280cebf63eba72422d1ddd62e1edeb5ad7e7d3cc`.

---

## Phase 1: Setup (Spike)

**Zweck**: die zwei Annahmen prüfen, die den Umfang der Registrierung ändern (research R5).

- [ ] T001 Spike zur Registrierung von `mailto`: (a) Android: nimmt das Plugin einen Link der Aktion `ACTION_SENDTO` an
      `onOpenUrl`, mit `adb shell am start -a android.intent.action.SENDTO -d mailto:a@example.org`? Wenn nicht, ist ein
      eigener Filter und die Weitergabe in `src-tauri/plugins/holzi-android` nötig (T019). (b) Windows: schreibt der
      Installer (NSIS/WiX) für `mailto` Einträge unter `Software\Classes\mailto` (macht holzi zum Standard, FR-016
      verletzt) oder unter `RegisteredApplications`? (c) Linux: trägt das Paket `x-scheme-handler/mailto` als mögliches,
      nicht als Standardprogramm ein? Ergebnis in `research.md` R5 unter „Ergebnis des Spikes“ festhalten; bei (b) mit
      Eintrag in `Classes\mailto` fällt `mailto` auf Windows aus dem Paket, und die Spec bekommt eine Ausnahme in
      FR-016. Danach den Spike-Zweig verwerfen.

---

## Phase 2: Grundlagen (blockiert alle Geschichten)

- [x] T002 [P] `src-tauri/src/error.rs`: `HolziError::OpenerLinkInvalid { reason }`, `OpenerLinkTooLarge`,
      `OpenerNotAvailable` mit stabilen Codes; in `src/composables/useErrorString.ts` abbilden und die Texte in
      `src/i18n/locales/{de,en}.json` unter `errors.openers.*` anlegen (Ton wie `errors.share.*`).
- [x] T003 [P] `src-tauri/src/openers/mod.rs` (neu, in `lib.rs` als `pub mod openers` deklarieren) und
      `src-tauri/src/openers/mail.rs` (neu): `MailRequest { to, cc, bcc, subject, body }` (ts-rs als
      `MailRequestView`, `#[ts(export, export_to = "../../src/types/bindings/")]`) und
      `parse(link: &str) -> Result<MailRequest>` nach `research.md` R2 (Schema ohne Rücksicht auf Schreibweise;
      `to`/`cc`/`bcc` hängen an, `subject` und `body` gelten zuerst, andere Namen ignoriert; UTF-8-Dekodierung mit
      `percent_encoding`; `\r`/`\n` in Empfänger, Kopie, Blindkopie und Betreff lehnen ab, im Text `\r\n` → `\n`;
      Adressprüfung; Grenzen 50 / 998 / 16 KiB / 32 KiB).
- [x] T004 [P] `src-tauri/src/openers/mail_tests.rs` (neu): Tabelle der Fälle (SC-004): ein Empfänger; mehrere mit
      Komma und `to=`; `cc`/`bcc`; Betreff und Text prozent-kodiert; nicht-ASCII (`%C3%BC`, `%E2%82%AC`); ohne Empfänger
      (`mailto:?subject=x`); Großschreibung des Schemas; unbekannte Felder (`in-reply-to`) ignoriert; Ablehnung: `%0d`,
      `%0a` und `%0D%0A` im Betreff und in Empfängern, kaputtes `%zz`, ungültiges UTF-8, Adresse ohne `@`, mit
      Leerzeichen; Grenzen bei 50/51 Empfängern je Feld, 998/999 Zeichen, 16 KiB/+1 Byte, ganzer Link 32 KiB/+1;
      Zeilenumbrüche im Text bleiben als `\n`.
- [x] T005 `src-tauri/src/extensions/bundle/manifest.rs`: `Manifest.openers: Vec<String>` (aus `openers`, nur wenn ein
      Array von Zeichenketten, sonst leer) und `OpenerKind::MailCompose` mit `from_str` für `mail.compose`;
      `Manifest::opens(kind)`; Tests in `manifest_tests.rs`: Block fehlt → leer, zwei Arten gelesen in Reihenfolge,
      unbekannte Art bleibt stehen, falscher Typ → leer.
- [x] T006 `src-tauri/src/extensions/registry/list.rs` (und den Typ von `ExtensionSummary`): `openers: Vec<String>` mit
      den **bekannten** Arten, nur wenn die Extension installiert und aktiviert ist (FR-012); Tests in
      `list_tests.rs`: aktiviert → genannt, deaktiviert → leer, unbekannte Art → nicht genannt, nach `remove` weg.
      Bindings neu erzeugen und die Leerzeichen am Zeilenende entfernen
      (`sed -i -E "s/[[:space:]]+$//" src/types/bindings/*.ts`).

**Checkpoint**: Parser, Manifest und Liste stehen; keine Oberfläche, keine Übergabe.

---

## Phase 3: Geschichte 3 – Eine Extension meldet sich als Öffner an (Priorität: P2)

**Ziel**: Anmeldung im Manifest, geprüft, gelesen, angezeigt; die Extension erscheint und verschwindet mit ihrem Zustand.

**Unabhängiger Test**: Testbündel mit `openers` installieren: es steht in `ExtensionSummary.openers`; ohne Block nicht;
deaktiviert nicht mehr.

- [ ] T007 [P] [US3] [vault-sdk] `crates/haex-bundle/src/openers.rs` (neu): `check_openers(&JsonValue)` nach
      `contracts/manifest-openers.md` (Array von Zeichenketten, höchstens 8, Form und Länge, keine Doppelten, unbekannte
      Art mit richtiger Form gültig); in `format.rs` an derselben Stelle wie `check_tools` aufrufen, im WebAssembly-Bau
      einschließen; Tests und Testvektoren in `test-vectors/` (gültig, zu viele, falsche Form, zu lang, doppelt,
      unbekannte Art).
- [ ] T008 [P] [US3] [vault-sdk] `src/types.ts`: `ExtensionManifest.openers?: string[]`; `src/client.ts` und `src/api/`:
      `client.openers.take()` und `client.openers.onRequest(handler)` (ruft `extension_opener_take` und hört auf
      `opener-request`, `contracts/bridge.md`), Typ `OpenRequest`; Test mit einer Aufzeichnung wie
      `src/api/__tests__/transcripts/`; `manifest.ts` lässt den Block durch (`...parsed`).
- [x] T009 [US3] `src-tauri/src/extensions/registry/declared_view.rs` und die Installationsansicht: bei `mail.compose` die
      Zeile „kann E-Mail-Links öffnen“ (FR-014), keine Berechtigungsabfrage; Texte unter
      `extensions.install.openers.mailCompose` in `src/i18n/locales/{de,en}.json`; Test in der vorhandenen
      Ansicht-Prüfung (`declared_view`-Tests), die Zeile erscheint nur mit der Anmeldung (Szenario 1 und 2).
- [x] T010 [P] [US3] Testbündel `src-tauri/tests/fixtures/extension_e2e/mail-opener.xt` (neu, Quelle `mail-opener/` daneben):
      Manifest mit `"openers": ["mail.compose"]`, eine Seite, die beim Start `extension_opener_take` aufruft und das
      Ergebnis in die Seite schreibt (für T016 und T017); gebaut wie `model-jobs.xt` (Testschlüssel, Bytes in
      `tests/extension_e2e_fixtures.rs` geprüft), mit dem Verfahren der Notiz zum Neubau über eine eigenständige Kiste.
- [ ] T011 [US3] Nach dem Merge von T007: `src-tauri/Cargo.toml` und `Cargo.lock` auf die neue Revision von
      `haex-bundle` heben (nur die Revision, Kommentar daneben anpassen); `cargo test` der Bündel-Tests
      (`extensions::bundle`, `extensions::registry`) laufen lassen; ein Bündel mit falsch geformtem `openers` wird
      abgelehnt (Test in `bundle/manifest_tests.rs`).

**Checkpoint**: Geschichte 3 steht: Anmeldung gelesen, angezeigt, geprüft.

---

## Phase 4: Geschichte 1 – Eine Mail an einen Kontakt mit haex-mail schreiben (Priorität: P1) 🎯 MVP

**Ziel**: Mail-Knopf mit Auswahl, Wahl öffnet den Schreib-Dialog der Extension; ohne Öffner alles wie heute.

**Unabhängiger Test**: Testextension installieren, Mail-Knopf, Öffner wählen: `take` liefert die Felder; ohne Öffner öffnet
der Knopf beim System.

- [ ] T012 [P] [US1] `src-tauri/src/openers/requests.rs` (neu) und `requests_tests.rs`: `OpenRequests` nach
      `data-model.md` §3 (`park`, `take`, `TAKE_TIMEOUT` 15 s, in Tests abgekürzt), eine Anfrage je Extension, eine neue
      ersetzt die alte; Tests: parken und abholen genau einmal, ersetzen, andere Extension holt nichts, Verfall sendet
      `opener-request-expired`, nach dem Verfall `None`.
- [ ] T013 [US1] `src-tauri/src/extensions/bridge/openers.rs` (neu) und Eintrag in `bridge/allowlist.rs`:
      `extension_opener_take` nach `contracts/bridge.md` (nur die Anfrage der aufrufenden Extension, nur wenn
      installiert, aktiviert und angemeldet, sonst `null`; Felder als Text, kein roher Link); `emit_to_frames` mit
      `opener-request` beim Parken; Tests in `bridge/dispatch_tests.rs`-Art: holt die eigene Anfrage, `null` ohne
      Anmeldung, eine andere Extension bekommt die Anfrage nicht, zweites `take` gibt `null`, ohne Berechtigung
      aufrufbar (FR-010).
- [ ] T014 [US1] `src-tauri/src/openers/commands.rs` (neu): `opener_mail_check` (liest den Link, gibt `MailRequestView`
      oder den Fehler) und `opener_mail_open` (liest neu, prüft Installation, Aktivierung und Anmeldung, parkt;
      `OpenerNotAvailable` sonst); `OpenRequests` als Zustand und beide Befehle in `lib.rs` anmelden; die Frist ruft
      `opener-request-expired` über den vorhandenen Emitter. Tests: abgelehnter Link parkt nichts, nicht angemeldete
      Extension → `OpenerNotAvailable`, Link wird in `open` neu gelesen (nicht dem Fenster geglaubt).
- [ ] T015 [P] [US1] `src/lib/mail/openers.ts` (neu, rein, relative Importe): `openerRows(extensions)` (aktiviert, mit
      `mail.compose`, nach Name sortiert), `mailTargetBehavior(rows)` (`'system'` ohne Öffner, `'menu'` sonst) und
      `systemAllowed(source: 'button' | 'external')` (nie für `external`, FR-018); `scripts/check-openers-menu.ts`
      (neu): ohne Öffner `'system'` (FR-001), zwei Öffner zwei Zeilen mit Namen, deaktivierte nicht, `external` nie mit
      System, Reihenfolge stabil.
- [ ] T016 [US1] `src/composables/useMailOpeners.ts` (neu: `openers` aus dem Speicher der Extensions,
      `openWithAsync(extensionId, link)` ruft `opener_mail_open` und `openExtension`, hört auf
      `opener-request-expired` und sagt es mit einer Meldung), `src/components/contacts/MailOpenMenu.vue` (neu,
      Dropdown wie das „+“-Menü; Zeile „Mit dem System öffnen“ und je Öffner eine Zeile; Testids
      `contacts-open-mail`, `contacts-open-system`, `contacts-open-with-<extensionId>`), und
      `src/components/contacts/ValueActions.vue`: der Mail-Knopf wird `MailOpenMenu` nur bei `'menu'`, sonst bleibt der
      Knopf wie heute (FR-023: andere Aktionen unverändert); Texte `mail.openers.*` in de/en; Antwort auf
      „Schließen ohne Wahl“: nichts (FR-004).
- [ ] T017 [P] [US1] [haextension] `apps/haex-mail/haextension/manifest.json`: `"openers": ["mail.compose"]`;
      `apps/haex-mail/app/plugins/openers.client.ts` (neu, Muster von `agent-tools.client.ts`): beim Start und bei
      `client.openers.onRequest` die Anfrage holen und `ComposeDialog` (`app/components/ComposeDialog.vue`) mit
      Empfänger, Kopie, Blindkopie, Betreff und Text öffnen, ohne zu senden (FR-015); Test des Plugins mit einer
      gefälschten Anfrage; haex-mail hebt dafür die SDK-Version (nach T008).
- [ ] T018 [US1] E2E `scripts/e2e/scenarios/contacts-mail-openers.test.ts` (neu): ohne Extension öffnet der Mail-Knopf beim
      System (nicht auswählbar, Aufruf des Systems geprüft über die gefälschte Oberfläche des Hosts, wie die Szenarien
      der Dateifreigaben); mit `mail-opener.xt` zeigt er das Menü mit zwei Zeilen; Öffner wählen → Fenster der
      Extension, die Seite zeigt die Felder des Links; „Schließen ohne Wahl“ öffnet nichts; Extension deaktivieren → der
      Knopf öffnet ohne Menü; die Wahl geht nur nach dem Tipp (nichts erreicht die Extension vorher, SC-003).

**Checkpoint**: Geschichte 1 funktioniert allein, ohne Registrierung beim System.

---

## Phase 5: Geschichte 2 – Ein E-Mail-Link von außen öffnet holzi, und ich wähle den Öffner (Priorität: P2)

**Ziel**: `mailto:` von außen führt zum Dialog; gesperrte Vault wartet; kein Systemweg.

**Unabhängiger Test**: Android: `phone.openLink('mailto:…')` zeigt den Dialog, auch nach dem Entsperren.

- [ ] T019 [US2] Registrierung: `src-tauri/tauri.conf.json` (`plugins.deep-link.desktop.schemes` um `mailto`,
      `mobile` um `{ "scheme": ["mailto"] }`), `src-tauri/gen/android/app/src/main/AndroidManifest.xml` (den vom Plugin
      erzeugten Block neu erzeugen und einchecken; bei Befund aus T001 zusätzlich ein eigener Filter für
      `ACTION_SENDTO` und die Weitergabe in `src-tauri/plugins/holzi-android`);
      `src-tauri/src/deep_link/mod.rs`: `register_for_development` registriert nur `haex-vault`
      (`register("haex-vault")` statt `register_all()`, FR-016); `scripts/check-openers-registration.ts` (neu): liest
      `tauri.conf.json` und prüft beide Schemata, und dass der Quelltext von `register_for_development` `register_all`
      nicht enthält; bei Befund (b) aus T001 `mailto` nicht für Windows eintragen und die Spec anpassen.
- [ ] T020 [US2] `src-tauri/src/deep_link/mod.rs`: `contact_link` wird `known_link`, lässt `haex-vault://contact/…` und
      `mailto:…` durch (gleiche Grenze 32 KiB; `first_contact_link` → `first_known_link`, alle Aufrufer anpassen);
      `src-tauri/src/deep_link/inbox.rs`: `claim_next(dir, focused)`, ein Prozess ohne fokussiertes Fenster holt eine
      Datei erst, wenn sie älter als 1 s ist (`research.md` R6); `watch` fragt `window.is_focused()`; Tests in
      `mod_tests.rs` (`mailto:` wird durchgelassen, fremde Schemata nicht) und `inbox_tests.rs` (fokussiert holt
      sofort, nicht fokussiert nach 1 s, zwei Abholer genau einer).
- [ ] T021 [P] [US2] `src/lib/mail/link.ts` (neu, rein): `mailLinkText(url)` (nur `mailto:`, höchstens 32 KiB, sonst
      `null`) und `acceptMailLink(url, pending)` (der neueste ersetzt den wartenden, ein gleicher Link innerhalb von 3 s
      zählt einmal); `src/stores/mailInbox.ts` (neu, Pinia: `link`, `park`, `take`);
      `src/composables/useDeepLink.ts`: `handle` unterscheidet nach Schema (Kontakt-Link wie bisher an `shareInbox`,
      `mailto:` an `mailInbox`); `scripts/check-openers-link.ts` (neu): fremdes Schema, zu lang, ein zweiter Link ersetzt
      den ersten, derselbe zweimal einmal, `haex-vault`-Links gehen weiter an `shareInbox`.
- [ ] T022 [US2] `src/components/mail/MailLinkDialog.vue` (neu, `UiDrawerModal`): nimmt den Link aus `mailInbox`, ruft
      `opener_mail_check`; bei Fehler „Der Link kann nicht gelesen werden“ ohne Zeilen; sonst eine Zeile je Öffner
      (`openWithAsync`) und „Link kopieren“, ohne Öffner der Satz „Kein Öffner installiert“; **kein**
      `openUrl`/„Mit dem System öffnen“ (FR-018); öffnet nie selbst etwas (FR-020); Testids `mail-link-dialog`,
      `mail-link-open-<extensionId>`, `mail-link-copy`, `mail-link-error`, `mail-link-none`; in
      `src/pages/workspace/[instance].vue` nach `sessionRestored` einhängen; ein neuer Link ersetzt den Inhalt eines
      offenen Dialogs; `scripts/check-openers-menu.ts` ergänzen: `MailLinkDialog.vue` importiert `openUrl` nicht.
- [ ] T023 [US2] E2E `scripts/e2e/scenarios/android-mail-link.test.ts` (neu, Telefon): `phone.openLink('mailto:ben@example.org?subject=Hallo')`
      bei geöffneter Vault zeigt den Dialog ohne „Mit dem System öffnen“ und mit der Zeile des Testöffners
      (`mail-opener.xt`); Öffnen zeigt die Felder; bei gesperrter Vault wartet der Link und der Dialog erscheint nach
      dem Entsperren (Muster von `android-contacts-deeplink`); zwei Links nacheinander: der Dialog zeigt den zweiten;
      ein Link mit `%0d%0a` im Betreff zeigt „kann nicht gelesen werden“.

**Checkpoint**: alle drei Geschichten arbeiten.

---

## Phase 6: Abschluss

- [ ] T024 [P] `CONTEXT.md`: Begriffe **Öffner**, **Öffner-Anmeldung**, **Öffnen-Auswahl** neben dem Eintrag des
      Adressbuchs; Hinweis in `specs/051-phone-contacts/spec.md` an FR-006 („Ergänzt durch Spec 063: bei einem Öffner
      zeigt der Mail-Knopf eine Auswahl“) und in `specs/017-extension-host/` am Manifest-Vertrag (neuer Block
      `openers`), nur als Verweis.
- [ ] T025 Prüfung: gezielte `cargo test -j 4` (`openers`, `extensions::bundle`, `extensions::registry`,
      `extensions::bridge`, `deep_link`), alle `pnpm check:*`, `pnpm typecheck`, `pnpm typecheck:scripts`, `pnpm lint`,
      Clippy für den Host und `aarch64-linux-android --all-targets` mit `-D warnings`, Leerzeichen in
      `src/types/bindings/*.ts` entfernen; der Quickstart von Hand (`quickstart.md`, Schritte 1 bis 10).
- [ ] T026 PR-Hygiene: `git fetch`; Spec-Nummer 063 und ADR-Nummer noch frei (ein ADR nur, wenn T001 eine
      Ausnahme für Windows ergibt); im PR-Text die SDK-Revision, die Änderung von `register_all`, die Frist von 15 s und
      den Befund des Spikes nennen; keine
      Zuschreibungszeilen.

---

## Abhängigkeiten und Reihenfolge

- T001 zuerst; T019 braucht sein Ergebnis, der Rest nicht.
- Phase 2 blockiert alles: T002, T003, T004 parallel; T005 vor T006.
- US3: T007 ∥ T008 ∥ T010; T009 nach T005; T011 nach T007 (Merge im `vault-sdk`).
- US1: T012 ∥ T015 ∥ T017 (T017 nach T008); T013 nach T012 und T005; T014 nach T012, T013 und T003; T016 nach T006,
  T014 und T015; T018 nach T010, T013, T014 und T016.
- US2: T019 nach T001; T020 nach T003; T021 parallel zu T019 und T020; T022 nach T014, T016 und T021; T023 nach
  T019, T020, T022 und T010.
- Phase 6 zuletzt; T024 darf früher laufen.

## Parallel-Beispiele

```text
Nach T001:   T002 ∥ T003 ∥ T004
Nach T006:   T007 ∥ T008 ∥ T010 ∥ T012 ∥ T015 ∥ T021
Nach T016:   T017 (haextension) ∥ T022
```

## Umsetzungsstrategie

- **MVP** ist Geschichte 1 mit Geschichte 3: Anmeldung gelesen, Mail-Knopf mit Auswahl, Übergabe an die
  Testextension; ohne Registrierung beim System und ohne `vault-sdk` (T011 kann nachfolgen).
- Danach Geschichte 2 als eigener PR (braucht T001 und den Deep-Link-Weg).
- Die Arbeiten im `vault-sdk` (T007, T008) und in `haextension` (T017) laufen als eigene PRs in ihren Repos; holzi
  hebt die Revision erst nach dem Merge (T011).
- Vorschlag für Teile (PRs): A = Phase 2 und US3 (T002–T010), B = US1 (T012–T018), C = US2 (T019–T023), D = Abschluss.
