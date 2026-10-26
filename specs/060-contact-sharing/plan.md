# Implementation Plan: Kontakte teilen und der Kontakt „Ich“

**Branch**: `060-contact-sharing` | **Date**: 2026-10-26 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/060-contact-sharing/spec.md`

## Summary

Jede Vault bekommt einen festen Ich-Kontakt. Jeder Kontakt lässt sich über denselben Teilen-Dialog weitergeben:
der Nutzer wählt vorher jede Angabe einzeln ab, holzi baut daraus einen Text `haex-vault://contact/…` (ein Deep
Link; Link, Zwischenablage und QR-Code tragen denselben Text). Beim Einlesen zeigt eine Vorschau, was neu ist, und
der Nutzer wählt je Angabe „Hinzufügen“, „Aktualisieren“ oder „Nicht übernehmen“; an einem vorhandenen Kontakt geht
nie etwas verloren. Agenten teilen über eine zerstörerische Aktion mit einer Abfrage je Aufruf.

Technischer Ansatz (Begründungen und verworfene Alternativen in [research.md](./research.md)):

- **Ich-Kontakt** (R1, R2): Migration `0042_contacts_self` legt die Zeile mit fester Kennung an; Schutz gegen
  Löschen, gesperrter Anzeigename, leer erlaubt, keine Abgleichsziele, kein Kandidat des Telefonabgleichs; die
  Oberfläche zeigt „Ich“ als Anzeige, Agenten sehen den echten Namen mit `isSelf`.
- **Format** (R3, `contracts/share-format.md`): `ShareEnvelope` (postcard, base64url) mit Name, Namensteilen,
  gewählten Angaben und optional der unveränderten Karte; Grenzen 16 384 Zeichen gesamt, QR bis 2 048.
- **Eigene Karte** (R4): `own_card::build_for_share` unterschreibt pro Teilen eine frische Karte mit der
  Auswahl an Geräten und Relays, ohne zu speichern.
- **Karten speichern** (R5): `haex_contacts_cards` wird mit dem Hash des unterschriebenen Inhalts als
  Primärschlüssel neu aufgebaut (Migration `0043`, `HOLZI_TRIGGER_VERSION` 28); mehrere Karten je Vault, Adressen
  als Vereinigung, Schwellwert = neueste Karte, Prüfung beim Lesen.
- **Einlesen** (R6): `contacts_share_preview`/`contacts_share_accept` mit Posten, Wahlen und einer
  Zielrevision gegen veraltete Vorschauen; die
  Vault-Identität eines Kontakts wird nie ersetzt, der Ich-Kontakt ist nie Ziel.
- **Agent** (R8): Aktion `contacts.share` (`destructive`, `runner: native`), eigene Abfrage mit änderbarer Auswahl
  (`ContactShareConfirm`), neues Merkmal `Tool::confirms_itself` gegen die doppelte Abfrage.
- **Deep Link** (R9): `tauri-plugin-deep-link`, Intent-Filter auf Android, Zwischenspeicher im Frontend bis zur
  offenen Vault, auf dem Desktop ein Posteingangsverzeichnis zwischen den Prozessen.
- **Entfernen** (R10): Einstellungsblock, Seite „Eigene Visitenkarte“, Befehle und Einstellungen der alten Wahl.

## Technical Context

**Language/Version**: Rust (Tauri 2, Edition wie `src-tauri/Cargo.toml`); TypeScript (strict), Vue 3, Nuxt 4 (SPA)

**Primary Dependencies**: neu `tauri-plugin-deep-link` (2.6.2, `rust-ini` in `Cargo.lock` auf 0.21.1) und `@tauri-apps/plugin-deep-link`; vorhanden `postcard`,
`serde`, `base64`, `sha2`, `qrcode`, `ts-rs`, `vue-i18n`, haex-ui, `@tauri-apps/plugin-barcode-scanner`

**Storage**: Vault-Datenbank (haex-crdt): feste Zeile in `haex_contacts` (0042), neu aufgebaute
`haex_contacts_cards` (0043, Trigger-Version 28); Posteingangsverzeichnis `<AppLocalData>/link-inbox/` (flüchtig)

**Testing**: gezieltes `cargo test -j 4` (`contacts::`, `identity::migrations_contacts_*`, `sync::foreign_auth`,
`chat::tools`); `node --test` (`pnpm check:contacts`, `check:agent-actions`); E2E `contacts-share`,
`agent-contacts`, Android-Deep-Link; Tests in `*_tests.rs` neben dem Modul

**Target Platform**: Linux, macOS, Windows, Android (Deep Link braucht je Plattform eigene Verdrahtung)

**Project Type**: Tauri-Desktop-/Mobil-App (Rust-Backend, Nuxt-Frontend)

**Performance Goals**: Dialog und Vorschau ohne spürbare Verzögerung; Abholen aus dem Posteingang binnen 1 s

**Constraints**: nichts geht an einem vorhandenen Kontakt verloren (SC-005a); Link, Zwischenablage und QR tragen
denselben Text (SC-009); Öffnen eines Links speichert nichts (FR-018a); jede Karte wird vor dem Speichern und beim
Verwenden geprüft (FR-025a); keine Dauer-Erlaubnis für den Agenten (FR-030)

**Scale/Scope**: ein Ich-Kontakt je Vault; bis 200 Angaben je Weitergabe; ein paar Karten je Vault

## Constitution Check

_GATE: vor Phase 0 und nach Phase 1 geprüft._

- I Keine Geheimnisse in Git: Karten und Weitergaben tragen keinen Schlüssel und kein Geheimnis (nur öffentliche
  Schlüssel, Unterschrift); Testdaten sind erzeugt, keine echten Schlüssel.
- II Keine lokalen Pfade: Spec, Plan und Verträge nennen nur Pfade im Repo; in `tauri.conf.json` und im Code
  steht kein lokaler Pfad. Die Legacy-App wird als Repository `haex-space/haex-vault-legacy` genannt.
- III–V Identität und Quellen: nicht berührt.
- VI Selbstverändernde Anweisungen: keine; die `CLAUDE.md` gibt es im Repo nicht, der Agentenkontext wird nicht
  angefasst.
- VII Relay-Ausfall blockiert lokale Arbeit nicht: Teilen, Vorschau und Speichern arbeiten auf lokalen Daten; die
  Weitergabe braucht kein Netz.
- Konventionelle Commits, PR nach `main`, kein Squash: Arbeit in `.worktrees/060-contact-sharing`, Commits auf
  Englisch, ohne Claude-Zusätze (Vorgabe des Betreibers).
- ADR für Entwurfsbeschlüsse: Die Kombination aus Umschlag, „nie überschreiben“, Hash-Schlüssel der Karten und
  Deep-Link-Schema ist ein Entwurfsbeschluss, der Spec 051 (FR-009, FR-012) ändert: **ADR 0017 „Kontakte werden
  über eine Weitergabe geteilt“** im selben PR (Aufgabe in `tasks.md`).
- Eine Umsetzung je Regel: Namensregel nur in `share::name_for`; Prüfung der Karten nur in
  `received::verified`; Schutz des Ich-Kontakts nur im Dienst (`service/contacts.rs`), nie in der Oberfläche.
- Jedes Gerät und jedes System: Rust-Zweige mit `cfg` auch für `aarch64-linux-android` linten.

Nach Phase 1: bestanden, mit dem ADR. Keine Komplexitätsabweichung außer
dem Posteingang (siehe unten).

## Project Structure

### Documentation (this feature)

```text
specs/060-contact-sharing/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── share-format.md       # Format der Weitergabe (neu)
│   └── tauri-commands.md     # ändert Spec 051 contracts/tauri-commands.md (Visitenkarten), fügt contacts.share hinzu
├── checklists/requirements.md
└── tasks.md                  # durch /speckit-tasks
```

### Source Code (repository root)

```text
src-tauri/src/
├── contacts/
│   ├── ids.rs                          # SELF_CONTACT_ID
│   ├── model.rs                        # is_self auf Contact und ContactHeader
│   ├── store.rs                        # is_self setzen
│   ├── share/                          # neu
│   │   ├── mod.rs
│   │   ├── envelope.rs                 # ShareEnvelope, Grenzen, Text codieren/lesen
│   │   ├── name.rs                     # name_for (FR-011)
│   │   ├── build.rs                    # Optionen und Bauen (eigener und fremder Kontakt)
│   │   ├── preview.rs                  # Posten, Ziele, Hinweise
│   │   ├── merge.rs                    # Wahlen anwenden (nie überschreiben)
│   │   └── *_tests.rs
│   ├── card/
│   │   ├── format.rs                   # sign_with(max_text), Hash des Inhalts
│   │   ├── own_card.rs                 # build_card herausziehen, build_for_share; OwnCard/Choice entfallen
│   │   └── received.rs                 # Hash-Schlüssel, stored/addresses/verified
│   ├── service/
│   │   ├── contacts.rs                 # Ich: delete/update/check_identity
│   │   ├── targets.rs                  # Ich: keine Abgleichsziele
│   │   ├── cards.rs                    # neue Dienste share_*; own_card/own_card_set/card_preview/card_accept entfallen
│   │   └── share.rs                    # neu: Dienst-Methoden
│   ├── commands/
│   │   ├── cards.rs                    # Befehle contacts_share_*; alte entfallen
│   │   └── agent.rs                    # isSelf in der Ausgabe
│   ├── agent.rs / agent_share.rs       # ContactsShareAgent (neu), grants(Read)
│   ├── agent_share_confirm.rs          # ContactShareConfirm (PromptPark), Ereignis, Befehl
│   └── phone/run.rs                    # Ich aus den Kandidaten
├── chat/tools/mod.rs, native_action.rs, turn/tool_round.rs   # Tool::confirms_itself
├── deep_link/                          # neu
│   ├── mod.rs                          # Plugin anmelden, URL an das Frontend (Ereignis)
│   └── inbox.rs                        # Posteingangsverzeichnis (ablegen, abholen, Zeitablauf)
├── identity/
│   ├── migrations.rs                   # 0042, 0043 anmelden; HOLZI_TRIGGER_VERSION = 28
│   ├── migrations_contacts_self.rs     # neu
│   ├── migrations_contacts_cards.rs    # neu (Hash-Schlüssel)
│   └── migrations_*_tests.rs
├── sync/foreign_auth.rs                # Schwellwert aus received::stored (neueste)
├── files/peer/connect.rs, extensions/peers/invite.rs   # Anwählen über received::addresses
└── lib.rs                              # Befehle, Plugin, ContactsShareAgent, Posteingang beim Öffnen der Vault

src-tauri/
├── Cargo.toml                          # tauri-plugin-deep-link
├── tauri.conf.json                     # plugins.deep-link
├── capabilities/default.json                # deep-link:default (gilt auch mobil)
└── gen/android/app/src/main/AndroidManifest.xml   # Intent-Filter

src/
├── components/contacts/
│   ├── ShareDialog.vue, ShareImport.vue, ShareConfirmDialog.vue    # neu (ersetzen OwnCard/CardImport)
│   ├── List.vue, EntryPage.vue, Toolbar.vue, Avatar-Anzeige        # Ich oben, „Teilen“, Menü
├── components/settings/ContactsCardSetting.vue, ContactsView.vue   # Block entfällt
├── components/wm/appRoutes.ts          # /import statt /card, /card/import
├── composables/useContactShare.ts, useDeepLink.ts                  # neu
├── stores/shareInbox.ts, contactsActionHandlers.ts                 # neu / Anpassung
├── lib/actions/contactsActions.ts      # contacts.share, isSelf in den Beschreibungen
├── lib/contacts/search.ts, registry.ts # Ich zuerst, Anzeige „Ich“
├── pages/workspace/[instance].vue      # Abspielen des Links, ShareConfirmDialog
├── i18n/locales/{de,en}.json
└── types/bindings/                     # ts-rs (Contacts…/ContactShare…)

scripts/check-contacts-share.ts                         # neu
scripts/e2e/scenarios/contacts-share.test.ts            # neu; agent-contacts.test.ts erweitert
docs/adr/0017-contacts-share-envelope.md                # neu
CONTEXT.md                                              # Begriffe Ich-Kontakt, Weitergabe
specs/051-phone-contacts/                               # Anhang: FR-009, FR-011, FR-015a, FR-012 verweisen auf 060
```

**Structure Decision**: Alles Neue zum Teilen liegt in `contacts/share/` und `deep_link/`; `card/` behält die
Karte (Format, Speicher, eigene Karte), `share/` kennt sie nur über `format` und `received`. Jede Regel hat eine
Stelle: die Prüfung der Karten in `received::verified`, die Namensregel in `share::name_for`, die Wahlen in
`share::merge`.

## Reihenfolge und Risiken

1. **Spike Deep Link** (R9, zuerst): Plugin einbinden, ein Link öffnet die App auf Linux und im Android-Emulator
   und erreicht `useDeepLink`. Ergebnis entscheidet über den Posteingang (Desktop).
2. Migrationen 0042/0043 mit Tests (zeigt früh, ob das Stempeln des Seeds und der Neuaufbau tragen).
3. Karten: Hash, `verified`, `stored`/`addresses`, Anpassung von `foreign_auth` und der Anwähler.
4. Ich-Kontakt im Dienst, dann in der Oberfläche.
5. Format, Bauen, Dialog; dann Einlesen, Vorschau, Wahlen.
6. Agent-Aktion und `confirms_itself`.
7. Deep Link vollständig, Aufräumen, ADR, E2E.

| Risiko                                                     | Gegenmaßnahme                                                                   |
| ---------------------------------------------------------- | ------------------------------------------------------------------------------- |
| Migrations-Insert wird nicht gestempelt                    | Test mit zwei Vaults (R1); Rückfall: Seed im Bootstrap mit festen Werten        |
| Deep Link auf Linux nur mit installiertem Paket            | `register_all` im Entwicklungsbetrieb, E2E über Android und AppImage-/Deb-Probe |
| Neuaufbau der Kartentabelle verliert vorhandene Adressen   | im PR genannt, kein Kopieren möglich (R5); die Kontakte bleiben                 |
| `HOLZI_TRIGGER_VERSION` 28 trennt alte und neue Geräte     | bestehendes Verhalten (Handshake `crdtTrigger`); im PR genannt                  |
| Mehrere Prozesse: Link landet in der falschen Vault        | Vorschau nennt die Vault; Abbrechen immer möglich (R9)                          |
| Migrationsnummer 0042/0043 kollidiert mit anderen Branches | vor dem Push `git fetch`, Nummern neu prüfen                                    |

## Complexity Tracking

| Abweichung                                         | Warum nötig                                                                                                                           | Einfachere Alternative abgelehnt, weil                                                                                                                                          |
| -------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Posteingangsverzeichnis zwischen Desktop-Prozessen | Der Entwurf lässt mehrere Prozesse zu (eine Vault je Prozess); ein Tippen auf einen Link muss den Prozess mit offener Vault erreichen | Single-Instance-Plugin bräche den Mehrprozess-Entwurf; ein neuer Prozess allein führte in einen Vault-Wähler, in dem die offene Vault nicht noch einmal zu öffnen ist (Annahme) |
| `Tool::confirms_itself` im Chat-Kern               | Genau eine Abfrage mit änderbarer Auswahl statt zwei                                                                                  | Zwei Abfragen verwirren und die allgemeine kann die Auswahl nicht ändern                                                                                                        |
