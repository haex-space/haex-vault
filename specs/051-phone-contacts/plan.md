# Implementation Plan: Adressbuch in holzi mit Abgleich zum Telefon-Adressbuch (Android)

**Branch**: `051-phone-contacts` | **Date**: 2026-10-14 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/051-phone-contacts/spec.md`

## Summary

holzi bekommt ein eigenes Adressbuch als Kerndaten der Vault, eine eingebaute App „Kontakte“ und eine
Kategorie „Kontakte“ in den Einstellungen. Auf Android gleicht es sich in beide Richtungen mit dem
Telefon-Adressbuch ab, aber nur für Kontakte, die das Telefon als Abgleichsziel haben; in holzi
angelegte Kontakte bleiben standardmäßig in holzi. Visitenkarten bringen bestätigte holzi-Adressen,
Erweiterungen lesen mit Berechtigung, Agenten mit Freigabe. Einladungen aus dem Adressbuch (US7)
werden hier nur festgelegt und mit 027/028 gebaut.

Technischer Ansatz (Begründungen und verworfene Alternativen in [research.md](./research.md)):

- **Dienst** (R1, R2): `ContactsService` nach dem Muster von `PasswordsService`, einziger Zugang zu
  `haex_contacts_*`; `Caller` zieht nach `src-tauri/src/caller.rs` und wird gemeinsam genutzt.
- **Daten** (R3, R4, data-model.md): Kopf-Zeile plus eine Zeile je Angabe mit offener `kind`;
  Herkunft unveränderlich; Abgleichsziele, Zuordnungen und Issues als synchronisierte Tabellen mit
  `vault_device_uuid` (ADR 0001). Migration `0033_contacts`.
- **Android** (R5–R9, contracts/android-plugin.md): dünne Kotlin-Befehle im Plugin `holzi-android`
  über den Contacts Provider; Abgleich in Rust hinter `PhoneBook`: Index über `RawContacts.VERSION`,
  Drei-Wege-Vergleich mit gespeicherter Basis, zusammengefasste Kontakte über `LOOKUP_KEY`,
  Zusammenführen nach R8, Massenlösch-Sperre. Läuft nach Vault-Öffnen, bei `Resumed` und nach
  eingehenden Kontaktänderungen.
- **Fotos** (R10): ≤ 512 × 512 JPEG, ≤ 200 KiB, mit `image`/`fast_image_resize` wie die Vorschaubilder
  im Dateibrowser.
- **Visitenkarte** (R12, R13, contracts/contact-card.md): postcard, Schnorr-Unterschrift der
  Vault-Identität mit neuem Domain-Tag, Text `holzi:card:…` als Link und QR-Code;
  Scannen mit `tauri-plugin-barcode-scanner` auf Mobilgeräten.
- **Erweiterungen** (R14, contracts/bridge.md): `PermissionKind::Contacts` (nur `read`, Ziel `*` oder
  Gruppe), Brückenmethoden wie bei Passwörtern, neue API im vault-sdk.
- **Agenten** (R15): Scopes `contacts.read`/`contacts.write`, Aktionen in `contactsActions.ts`;
  Rückfrage mit Merken über die Agent-Freigaben aus Spec 044 (Art `contacts`).
- **Einladungen** (R16, contracts/nostr-invite.md, ADR 0014): gespeicherte NIP-59-Gift-Wraps je
  Empfängergerät; ADR 0014 löst D33 des Sync-Entwurfs in diesem Punkt ab.

## Technical Context

**Language/Version**: Rust (Tauri 2.12, Edition wie `src-tauri/Cargo.toml`); Kotlin im Plugin
`holzi-android` (Android minSdk 26, wie Spec 043); TypeScript 6 (strict), Vue 3.5, Nuxt 4.5 (SPA)

**Primary Dependencies**: neu `tauri-plugin-barcode-scanner` 2.x (nur `cfg(mobile)`, R13) und
`@tauri-apps/plugin-barcode-scanner`; vorhanden `haex-crdt`, `nostr` 0.45 (`nip44`, `nip59`),
`nostr-sdk` 0.45, `qrcode` 0.14, `image` 0.25, `fast_image_resize` 6.1, `unicode-normalization`,
`postcard`, `uuid` (v5), `ts-rs`; Frontend haex-ui und holzis `Settings{Group,Row,OptionRow}`

**Storage**: Vault-Datenbank über haex-crdt, zehn neue Tabellen ([data-model.md](./data-model.md));
Geräteeinstellungen in `preferences`; nichts außerhalb der Vault

**Testing**: `cargo test` (Lib-Tests in `*_tests.rs`, `FakePhoneBook` für den Abgleich), Vitest,
`pnpm typecheck`/`lint`/`format:check`, E2E Desktop und Android-Emulator (`PhoneControls` um
Kontakte erweitert); von Hand M1–M7 ([quickstart.md](./quickstart.md))

**Target Platform**: Adressbuch, Karten, Erweiterungen, Agenten: Linux, macOS, Windows, Android, iOS;
Abgleich mit dem Telefon-Adressbuch: Android (Fähigkeit `phoneContacts`); QR-Scannen: Android, iOS

**Project Type**: Desktop- und Mobil-App (Tauri) mit Erweiterungs-Host

**Performance Goals**: erster Abgleich 2.000 Kontakte < 60 s (SC-001); Telefonänderung < 5 s nach
`Resumed` (SC-002); Schreiben ins Telefon < 5 s nach Sync (SC-003); Suche in 5.000 Kontakten < 200 ms
(SC-007)

**Constraints**: kein Hintergrundlauf (Spec 043 FR-018); keine Angabe verlieren (FR-022, FR-023);
keine Massenlöschung ohne Bestätigung (FR-016, FR-026); holzi-Adressen und eigene Angaben nie ins
Telefon (FR-014); kein Zugriff ohne Prüfung (FR-028); kein UNIQUE in synchronisierten Tabellen;
Antworten des Plugins ≤ 1 MiB je Aufruf

**Scale/Scope**: bis 5.000 Kontakte je Vault, wenige Telefone je Vault, Fotos ≤ 200 KiB

## Constitution Check

_GATE: Must pass before Phase 0 research. Re-check after Phase 1 design._

| Regel                                                          | Stand                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| -------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Worktree auf Topic-Branch, PR nach `main`                      | ✅ `.worktrees/051-phone-contacts`, Branches `051-phone-contacts` (Spec, PR #418) und `051-plan` (Plan, PR #426); Rebase- oder Merge-Commit, kein Squash                                                                                                                                                                                                                                                                                                           |
| Speckit-Ablauf                                                 | ✅ specify → clarify (10 Klärungen) → Freigabe des Betreibers (2026-10-14, „dann mach das“) → plan; danach tasks, analyze, implement                                                                                                                                                                                                                                                                                                                               |
| ADR für Grundsatzentscheidungen                                | ✅ ADR 0014 (Einladung an Kontakte mit Karte löst D33 in einem Punkt ab); D33 im Sync-Entwurf verweist darauf                                                                                                                                                                                                                                                                                                                                                      |
| Tests in eigenen Dateien                                       | ✅ jede neue Rust-Datei mit `*_tests.rs` daneben (`#[cfg(test)] #[path]`); `FakePhoneBook` in `contacts/phone/test_support.rs`                                                                                                                                                                                                                                                                                                                                     |
| 500 LoC je Datei                                               | ✅ Aufteilung unten (Dienst nach Bereichen, Abgleich in Index/Vergleich/Schreiben/Zusammenführen)                                                                                                                                                                                                                                                                                                                                                                  |
| Graphify zuerst / Wiederverwendung                             | ✅ Graph des Abzweigs abgefragt (Passwortmanager, Android-Plugin, Geräteliste, Nostr, Erweiterungs-Berechtigungen). Wiederverwendet: Muster und `Caller` aus `passwords/`, `VaultDb`, `signing::{sign,verify}`, `presence::wrap_payload`/`unwrap_rumor`, `ServerConfig`, `qrcode`-SVG wie `LinkCode::qr_svg`, Vorschaubild-Verkleinerung aus `files/thumbnails.rs`, Agent-Freigaben und Dialog aus Spec 044, `PhoneControls`, `voice.rs::ensure_microphone`-Muster |
| Laziness Ladder / keine unnötige Abhängigkeit                  | ✅ eine neue Abhängigkeit (Scanner, R13); Telefonnummern ohne `phonenumber` (R8); kein FTS5 (R11)                                                                                                                                                                                                                                                                                                                                                                  |
| Eine Implementierung je Regel                                  | ✅ Abgleichsregeln nur in Rust (ADR 0010); Suchnormalisierung mit gemeinsamen Testfällen für Rust und Frontend (R11); Agent-Rückfrage über den vorhandenen Mechanismus (R15); `Caller` gemeinsam (R2)                                                                                                                                                                                                                                                              |
| Kein `unwrap`/`expect` auf Eingaben, Fehler nicht verschlucken | ✅ Karten, Plugin-Antworten und Eingaben geprüft (contracts); Fehler des Providers als Issue mit Grund, ohne persönliche Daten im Log                                                                                                                                                                                                                                                                                                                              |
| I. No Secrets in Git                                           | ✅ Karten und Testkontakte werden in Tests erzeugt; keine Schlüssel eingecheckt                                                                                                                                                                                                                                                                                                                                                                                    |
| II./III. Keine lokalen Pfade, Identität geräteunabhängig       | ✅ Zuordnungen hängen an `vault_device_uuid`, nicht an Pfaden                                                                                                                                                                                                                                                                                                                                                                                                      |
| IV. Fremde Inhalte mit fester Revision                         | ✅ vault-sdk-Änderung wird mit vollem SHA referenziert, sobald gemergt                                                                                                                                                                                                                                                                                                                                                                                             |
| VII. Ohne Relay weiterarbeiten                                 | ✅ Adressbuch, Abgleich und Karten funktionieren ohne Nostr-Relay und Sync-Server; nur die Zustellung von Einladungen (US7) braucht ein Nostr-Relay und meldet „ausstehend“                                                                                                                                                                                                                                                                                        |
| Keine Hinweise auf Agents in Commits und PRs                   | ✅ Commits und PR ohne Trailer                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| Phasen-Reihenfolge                                             | ✅ US1–US4 bauen auf 024 und 043 (in Benutzung); US5 auf 017 (in Benutzung); US6 nutzt den eingebauten Agenten jetzt, externe Agenten mit 021; US7 wartet auf 027/028                                                                                                                                                                                                                                                                                              |

Nach Phase 1 erneut geprüft: keine Verletzung, Complexity Tracking leer.

## Project Structure

### Documentation (this feature)

```text
specs/051-phone-contacts/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── tauri-commands.md     # Commands und Ereignisse der App und Einstellungen
│   ├── android-plugin.md     # Kotlin-Befehle für den Contacts Provider
│   ├── bridge.md             # Erweiterungen: Berechtigung, Methoden, Fehler
│   ├── contact-card.md       # Format und Prüfung der Visitenkarte
│   └── nostr-invite.md       # US7: Zustellung über Nostr (mit 027/028 gebaut)
├── checklists/requirements.md
└── tasks.md                  # /speckit-tasks
docs/adr/0014-invitations-from-the-address-book.md
```

### Source Code (repository root)

```text
src-tauri/
├── Cargo.toml                                  # tauri-plugin-barcode-scanner (cfg(mobile))
├── src/
│   ├── caller.rs (+ _tests)                    # Caller aus passwords/access.rs (R2)
│   ├── error.rs                                # ContactsDenied, PhoneContactsUnavailable, Card*
│   ├── lib.rs                                  # Commands, Barcode-Plugin, Resumed → contacts wake
│   ├── identity/
│   │   ├── migrations.rs                       # 0033 registrieren, HOLZI_TRIGGER_VERSION +1
│   │   └── migrations_contacts.rs              # zehn Tabellen (data-model.md)
│   ├── platform/mod.rs (+ mod_tests)           # phone_contacts
│   ├── sync/
│   │   ├── signing.rs                          # Domain::ContactCard
│   │   ├── resume.rs                           # Wake → Kontakte-Abgleich
│   │   └── device_list.rs, link/host.rs, …     # nach dem Unterschreiben: own_card::refresh
│   ├── contacts/
│   │   ├── mod.rs                              # Typen: Contact, Value, Kind, Origin
│   │   ├── access.rs (+ _tests)                # Grant, Scope, authorize_* (FR-028–FR-031)
│   │   ├── ids.rs                              # UUIDv5-Ableitungen
│   │   ├── kinds.rs (+ _tests)                 # bekannte Arten, Prüfung, Telefon ja/nein (FR-007, FR-014)
│   │   ├── search.rs (+ _tests)                # Normalisierung, Teilzeichenfolge (R11)
│   │   ├── photos.rs (+ _tests)                # Verkleinern, Binaries, Aufräumen (R10)
│   │   ├── service/
│   │   │   ├── mod.rs                          # ContactsService
│   │   │   ├── contacts.rs (+ _tests)          # anlegen, ändern (nur geänderte Zeilen), löschen
│   │   │   ├── groups.rs (+ _tests)
│   │   │   ├── targets.rs (+ _tests)           # Abgleichsziele (FR-018b–d)
│   │   │   └── cards.rs (+ _tests)             # einlesen, ersetzen, blockieren
│   │   ├── card/
│   │   │   ├── format.rs (+ _tests)            # ContactCard, Text, Prüfung (contracts/contact-card.md)
│   │   │   └── own_card.rs (+ _tests)          # erstellen, refresh, gewählte Angaben
│   │   ├── phone/
│   │   │   ├── mod.rs                          # PhoneBook-Trait, Lauf, Zustände
│   │   │   ├── android.rs                      # PhoneBook über holzi-android
│   │   │   ├── index.rs (+ _tests)             # VERSION-Vergleich, neue/geänderte/gelöschte
│   │   │   ├── mapping.rs (+ _tests)           # Provider-Felder ↔ Angaben, Zusammenfassung (R7)
│   │   │   ├── merge.rs (+ _tests)             # erstes Zusammenführen (R8)
│   │   │   ├── reconcile.rs (+ _tests)         # Drei-Wege-Vergleich, Basis (R6)
│   │   │   ├── write.rs (+ _tests)             # Ops bauen, Issues (FR-021, FR-025)
│   │   │   ├── guard.rs (+ _tests)             # Massenlöschung, Konto weg (R9)
│   │   │   └── test_support.rs                 # FakePhoneBook
│   │   └── commands/
│   │       ├── mod.rs, contacts.rs, targets.rs, phone.rs, cards.rs, agent.rs
│   ├── extensions/
│   │   ├── permissions/model.rs, manifest_map.rs, target.rs  # PermissionKind::Contacts
│   │   ├── bridge/dispatch.rs                  # extension_contacts_*
│   │   └── contacts.rs (+ _tests)              # Prüfung und Abbildung (contracts/bridge.md)
│   └── files/agent/prompt.rs, files/permissions.rs # Art `contacts` für Agent-Freigaben (R15)
└── plugins/holzi-android/
    ├── src/{lib.rs, android.rs, other.rs}      # Permission::Contacts, Befehle
    └── android/src/main/
        ├── AndroidManifest.xml                 # READ/WRITE_CONTACTS
        └── java/space/haex/holzi/android/
            ├── HolziAndroidPlugin.kt           # @Command-Einstiege
            └── ContactsProvider.kt             # Lesen, applyBatch, Fotos (contracts/android-plugin.md)
src/
├── components/apps/ContactsApp.vue
├── components/contacts/                        # Liste, Eintrag, Editor, Zielauswahl, Karte, Scanner, Übersicht
├── components/settings/ContactsView.vue        # Kategorie „Kontakte“ (R17)
├── components/files/AgentPermissionDialog.vue  # Art `contacts`
├── composables/useContacts.ts, useContactsPhone.ts
├── stores/contacts.ts, contactsActionHandlers.ts
├── lib/contacts/{registry.ts, search.ts, kinds.ts}
├── lib/actions/{scopes.ts, contactsActions.ts, catalog.ts}
├── lib/settings/registry.ts                    # Kategorie contacts
├── lib/wm/apps.ts                              # system.contacts
├── components/wm/appRoutes.ts                  # contactsRoutes()
└── i18n/locales/{de,en}.json
scripts/e2e/
├── lib/instance.ts, lib/platform/android.ts    # PhoneControls: Kontakte
└── scenarios/contacts-*.test.ts, android-contacts-*.test.ts
```

**Structure Decision**: Ein neues Modul `contacts/` neben `passwords/` mit derselben Schichtung
(Dienst, Prüfung, Commands). Alles Android-Spezifische liegt im Plugin `holzi-android` und in
`contacts/phone/android.rs`; der Rest des Abgleichs ist plattformunabhängig und auf dem Desktop
testbar.

## Lieferungen

| PR  | Inhalt                                                                                                                       | Stories |
| --- | ---------------------------------------------------------------------------------------------------------------------------- | ------- |
| A   | Doku: Spec, Plan, ADR 0014, Verweise in 027/028 und im Sync-Entwurf (#418, #426)                                             | —       |
| B   | `caller.rs`, Migration, `ContactsService`, Commands, App „Kontakte“, Suche, Fotos, Herkunft, Gruppen, Einstellungs-Kategorie | US1     |
| C   | Plugin-Befehle, Berechtigung, Fähigkeit, Index, Abbildung, Zusammenführen mit Übersicht, Übernahme ins Adressbuch, Sperre    | US2     |
| D   | Abgleichsziele (UI und Dienst), Schreiben ins Telefon, Issues, Löschungen in beide Richtungen, Zielkonto                     | US3     |
| E   | Visitenkarte: Format, eigene Karte, Einlesen, Scanner, blockierte Vaults in den Einstellungen                                | US4     |
| F   | Erweiterungen: Berechtigung, Brücke; PR im vault-sdk                                                                         | US5     |
| G   | Agenten: Scopes, Aktionen, Rückfrage mit Art `contacts`                                                                      | US6     |
| H   | Einladungen aus dem Adressbuch, mit oder nach dem ersten PR von 027/028                                                      | US7     |

E2E-Szenarien kommen mit dem PR ihrer Story.

## Complexity Tracking

Keine Verletzung der Constitution.
