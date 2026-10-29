# Implementation Plan: Extensions als Öffner für E-Mail-Links

**Branch**: `063-mailto-extensions` | **Date**: 2026-10-28 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/063-mailto-extensions/spec.md`

## Summary

Eine Extension meldet sich im Manifest mit einem neuen Block `openers` an („kann `mail.compose` öffnen“).
holzi liest einen E-Mail-Link (`mailto:`, RFC 6068) an einer einzigen Stelle in Rust in eine
**Öffnen-Anfrage** (Empfänger, Kopie, Blindkopie, Betreff, Text, mit Grenzen) und gibt sie nur auf Wahl des
Nutzers an die gewählte Extension: holzi hält die Anfrage im Speicher, öffnet das Fenster der Extension (oder
holt es nach vorn), und die Extension holt sie über eine Bridge-Methode ab. Der Mail-Knopf im Adressbuch
bekommt ein Menü, sobald es einen Öffner gibt; ohne Öffner bleibt er, wie er ist. Ein Link von außen kommt
über den Deep-Link-Weg aus Spec 060 (Schema `mailto` zusätzlich zu `haex-vault`) und zeigt einen Dialog mit
den Öffnern und „Link kopieren“, nie mit „Mit dem System öffnen“.

Technischer Ansatz (Begründungen und verworfene Alternativen in [research.md](./research.md)):

- **Anmeldung** (R1): Manifest-Block `openers: ["mail.compose"]`, geprüft in `haex-bundle` (Repo `vault-sdk`),
  gelesen von `bundle/manifest.rs`, sichtbar in der Installationsansicht und in `ExtensionSummary.openers`.
  Unbekannte Arten werden gelesen, aber nirgends angeboten (FR-013).
- **Link lesen** (R2): neues Modul `openers` mit `mail::parse(url) -> MailRequest`; ein Ort für Grenzen
  (50 Empfänger, 998 Zeichen Betreff, 16 KiB Text) und Ablehnung von Zeilenumbrüchen.
- **Übergabe** (R3): `OpenRequests` hält je Extension höchstens eine Anfrage im Speicher; Ereignis
  `opener-request` weckt offene Frames; Bridge-Methode `extension_opener_take` holt sie ab; nach 15 s ohne
  Abholung verfällt sie und das Fenster sagt es.
- **Mail-Knopf** (R4): `ValueActions.vue` zeigt bei mindestens einem Öffner ein Menü (`MailOpenMenu.vue`),
  sonst öffnet er wie heute beim System.
- **Von außen** (R5, R6): Schema `mailto` in der Deep-Link-Konfiguration (Desktop und Android), Eingang in
  `useDeepLink` und im Posteingang zwischen Prozessen, Speicher `mailInbox` (der neueste Link ersetzt den
  wartenden), Dialog `MailLinkDialog.vue` in `[instance].vue`. holzi macht sich nie selbst zum Standard.
- **Schleifenschutz** (R7): bei einem Link von außen gibt es „Mit dem System öffnen“ nie (Klärung 2026-10-28).
- **Über drei Repos** (R8): `vault-sdk` (Prüfung und Typen), `haextension` (haex-mail meldet sich an und
  öffnet seinen Schreib-Dialog), holzi (alles andere).

## Technical Context

**Language/Version**: Rust (Tauri 2, `src-tauri/`), TypeScript mit Vue 3/Nuxt 4 (`src/`); im Repo `vault-sdk`
Rust (`crates/haex-bundle`) und TypeScript; in `haextension` Nuxt (haex-mail).

**Primary Dependencies**: vorhanden `percent-encoding` 2.3, `tauri-plugin-deep-link` 2.6.2 (Spec 060),
`haex-bundle` (Rev 280cebf63eba72422d1ddd62e1edeb5ad7e7d3cc, später die neue), shadcn-Dropdown und
`UiDrawerModal`; keine neue Abhängigkeit in holzi.

**Storage**: keine. Anmeldungen kommen aus dem gespeicherten Manifest der Bündel, Anfragen leben nur im
Speicher (FR-017: nie auf Dauer).

**Testing**: `cargo test` (`openers/*_tests.rs`, Manifest-, Bridge- und Inbox-Tests), Node-Checks
(`scripts/check-extensions-openers-*.ts`), E2E-Szenarien `contacts-mail-openers` (Desktop) und
`android-mail-link` (Android); im `vault-sdk` Tests der Prüfung und der Testvektoren.

**Target Platform**: Linux, macOS, Windows, Android (Registrierung für `mailto` je Plattform, R5).

**Project Type**: Desktop- und Mobil-App (Tauri) plus zwei Nachbar-Repos.

**Performance Goals**: eine angemeldete Extension erscheint in der Auswahl innerhalb von 2 s nach dem
Aktivieren (SC-006), weil die Liste aus dem Speicher der Statusmeldungen kommt, nicht aus einem eigenen
Abruf.

**Constraints**: keine Mail wird ohne Wahl gesendet und kein Link ohne Wahl an eine Extension gegeben
(SC-003); die Extension bekommt nur Felder als Text, nie den rohen Link (FR-007); Texte in Deutsch und
Englisch (FR-021).

**Scale/Scope**: eine Art von Link (`mail.compose`), höchstens 8 Einträge in `openers`, ein wartender
Link von außen.

## Constitution Check

_GATE: vor Phase 0 und nach Phase 1._

| Grundsatz                                   | Stand                                                                                                                                                                                                                                         |
| ------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| I. Keine Geheimnisse in Git                 | erfüllt: keine Schlüssel oder Tokens; Testbündel sind mit dem vorhandenen Testschlüssel signiert, wie `model-jobs.xt`.                                                                                                                        |
| II. Keine lokalen absoluten Pfade           | erfüllt: Dokumente und Beispiele nennen nur Pfade im Repo.                                                                                                                                                                                    |
| III. Identität ist geräteunabhängig         | nicht berührt.                                                                                                                                                                                                                                |
| IV. Cross-Repo-Verweise pinnen Revisionen   | erfüllt: `vault-sdk` über die gepinnte Revision `280cebf63eba72422d1ddd62e1edeb5ad7e7d3cc` (neue Revision nach dem Merge, Aufgabe T002), `haextension` in Tasks als `haex-space/haextension@b05d0e039f00122bb4abb7079149fcdb1d663e39:<Pfad>`. |
| V, VI                                       | nicht berührt: keine Allowlist, keine Änderung von Anweisungen.                                                                                                                                                                               |
| VII. Relay-Ausfall blockiert nichts Lokales | nicht berührt (kein Netz).                                                                                                                                                                                                                    |
| VIII. Keine Verschleierung                  | nicht berührt.                                                                                                                                                                                                                                |

Nach Phase 1 unverändert erfüllt.

## Project Structure

### Documentation (this feature)

```text
specs/063-mailto-extensions/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── manifest-openers.md   # Block `openers`, Prüfung im vault-sdk
│   ├── bridge.md             # Methode `extension_opener_take`, Ereignis `opener-request`
│   └── tauri-commands.md     # Befehle und Ereignisse des Fensters
├── checklists/requirements.md
└── tasks.md
```

### Source Code (repository root)

```text
src-tauri/src/
├── openers/                              # neu
│   ├── mod.rs                            # MODUL-Konstanten, Fehler
│   ├── mail.rs                           # RFC 6068 lesen, Grenzen, MailRequest
│   ├── mail_tests.rs
│   ├── requests.rs                       # OpenRequests: parken, abholen, verfallen
│   ├── requests_tests.rs
│   └── commands.rs                       # opener_mail_check, opener_mail_open
├── extensions/
│   ├── bundle/manifest.rs                # Manifest.openers (bekannte und unbekannte Arten)
│   ├── registry/declared_view.rs         # Installationsansicht: „kann E-Mail-Links öffnen“
│   ├── registry/list.rs                  # ExtensionSummary.openers
│   └── bridge/{openers.rs,allowlist.rs}  # extension_opener_take; Ereignis opener-request
├── deep_link/{mod.rs,inbox.rs}           # mailto zulassen; im Vordergrund holt zuerst ab
└── lib.rs                                # State OpenRequests, Befehle; register_all → nur haex-vault
src-tauri/{tauri.conf.json,capabilities/default.json,gen/android/.../AndroidManifest.xml}
                                          # Schema mailto

src/
├── components/contacts/{ValueActions.vue,MailOpenMenu.vue}
├── components/mail/MailLinkDialog.vue    # Link von außen
├── composables/{useDeepLink.ts,useMailOpeners.ts}
├── stores/mailInbox.ts
├── lib/mail/openers.ts                   # reine Logik: Menü ja/nein, Zeilen, Ablauf
├── pages/workspace/[instance].vue        # Dialog einhängen
└── i18n/locales/{de,en}.json             # mail.openers.*

scripts/check-extensions-openers-*.ts, scripts/e2e/scenarios/{contacts-mail-openers,android-mail-link}.test.ts

# andere Repos
vault-sdk:   crates/haex-bundle/src/{openers.rs,format.rs}, src/types.ts, src/client.ts (openers.take)
haextension: apps/haex-mail/haextension/manifest.json, apps/haex-mail/app/plugins/openers.client.ts
```

**Structure Decision**: Alles Neue zu Öffnern liegt in `openers/` (Rust) und `lib/mail/` (Logik); bestehende
Dateien bekommen nur kleine Ergänzungen. Der Eingang von außen nutzt `deep_link/` aus Spec 060 mit einem
zweiten Schema statt eines zweiten Wegs.

## Phasen der Umsetzung

1. **Spike** (R5, zuerst): Android nimmt `ACTION_SENDTO` an? Was schreibt der Windows-Installer für
   `mailto`? Ergebnis entscheidet über Umfang der Registrierung, nicht über den Rest.
2. **Anmeldung** (Geschichte 3): Manifest-Block, Prüfung im `vault-sdk`, Lesen und Anzeigen in holzi.
3. **Link und Übergabe** (Geschichte 1): Parser, Anfrage, Bridge, Mail-Knopf-Menü; haex-mail öffnet.
4. **Von außen** (Geschichte 2): Schema, Eingang, Speicher, Dialog.
5. **E2E und Aufräumen**: Szenarien, `quickstart.md` von Hand.

## Complexity Tracking

Keine Verstöße gegen die Constitution; nichts zu rechtfertigen.
