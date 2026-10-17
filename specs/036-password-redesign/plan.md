# Implementation Plan: Passwortmanager-Redesign

**Branch**: `036-password-redesign` | **Date**: 2026-10-03 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/036-password-redesign/spec.md`

## Summary

Der Passwortmanager aus Spec 034 bekommt die Bedienung von haex-vault 2.x (Legacy) und zwei Dinge, die es
dort nicht gibt: Verweise zwischen Einträgen (Platzhalter im Text, KeePass-Vorbild) und
Passkey-Funktionen im Dienst. Den Umfang bestimmt die Spec (US1–US7); die External Bridge
bleibt draußen. Es gibt **keine Änderung an vorhandenen Tabellen**; neu ist eine kleine
CRDT-Tabelle für Passkey-Verbindungen; synchronisierte Passkey-Zähler bleiben 0.

Technischer Ansatz (Begründungen und verworfene Alternativen in [research.md](./research.md)):

- **Tabs und Wischgeste** (R1, R2, R15, R16): `EntryTabs.vue` mit `ShadcnTabs` für die Leiste
  und `swiper` (Finger und Stift, nicht Maus; Eingabefelder und Codes gesperrt;
  „Bewegung reduzieren“ ohne Gleiten) für die Wischfläche. Der Tab gehört zum Ort
  (`?tab=`, `entry/:id/history` bleibt und öffnet Verlauf). `EntryEditor.vue` (645 Zeilen) wird
  beim Umbau in Details, Extra, Verlauf und Hülle zerlegt.
- **Verweise** (R3, R4, R11, R12): ein einziger Auflöser in Rust (`passwords/references.rs`),
  Grammatik `{$<Eintrag>:username|password|extra:<Schlüssel>}`, bis **12 Stufen** wie KeePass,
  aber Kreise beim Speichern abgelehnt und **nie** leerer Text. Aufgelöst wird beim Anzeigen,
  Kopieren und Benutzen im Dienst; gespeichert und im Verlauf bleibt der Platzhalter; Listen
  lösen nie auf; ein Aufrufer ohne Bereich an der Quelle bekommt das ganze Feld nicht. Das
  Frontend zerlegt nichts selbst (Commands `references_parse`, `reference_token`).
- **Kopieren** (R7): `passwords/copy.rs` in einer Transaktion, Dialog mit Titel, Verlauf und
  Verweisen. Ablage, Auswahlleiste, Brotkrumen, Menüs und Kürzel sind reine TS-Bausteine plus
  die vorhandenen Shadcn-Teile (R10); nichts davon berührt die Zwischenablage des
  Betriebssystems.
- **Passkeys** (R5, R6, R8, R9): drei Dienstmethoden (`create`, `confirm`, `list`) ohne
  Tauri-Commands; ES256 und EdDSA (Anlegen und Bestätigen), RS256 nur anzeigen; Herkunft gegen
  die Kennung der Gegenstelle mit öffentlicher Suffixliste; synchronisierte Passkey-Zähler
  bleiben 0 statt eine unzuverlässige globale Ordnung zu behaupten; Passkey per Verbindung
  statt Kopie.
- **Anhänge** (R14): Karten, Vorschaubilder im Frontend (sichtbarer Bereich, höchstens zwei
  gleichzeitig), PhotoSwipe als Lightbox.
- **KeePass-Verweise beim Import** (R13): zweiter Durchgang nach dem Schreiben,
  `import/references.rs`.
- **Neue Abhängigkeiten**: Frontend `swiper`, `photoswipe`; Rust `ciborium`, `psl`, `url`
  (`url` steht schon transitiv). ADR-0009 hält Verweise und Passkey-Dienst fest (R18).

## Technical Context

**Language/Version**: Rust (Tauri 2.12), TypeScript 6 (strict), Vue 3.5, Nuxt 4.5.2 (SPA),
Node 22.19 für die Prüfskripte

**Primary Dependencies**: vorhanden — `p256` 0.14, `ed25519-dalek` 3, `pkcs1`/`pkcs8`/`spki`,
`sha2`, `getrandom`, `base64`, `uuid`, `serde_json`, `thiserror`, `ts-rs`, `haex-crdt`
(`928d06a`), `keepass`; Frontend Pinia, `@nuxtjs/i18n`, `reka-ui`/haex-ui-Layer
(`ShadcnTabs`, `ShadcnContextMenu`, `ShadcnBreadcrumb`, `ShadcnDropdownMenu`,
`ShadcnDrawer`/`UiDrawerModal`, `ShadcnCheckbox`), `@vueuse/core` 15.
**Neu**: `swiper` 14 (MIT), `photoswipe` 5.4 (MIT, wie haex-vault 2.x); Rust `ciborium`, `psl`,
direktes `url`. RS256 wird nicht signiert, deshalb **kein** `rsa` zur Laufzeit (R8).

**Storage**: Migration `0026_passwords_refs` (geplant als `0024`; Spec 017 hat `0024` und `0025` zuerst belegt): eine CRDT-Tabelle
(`haex_passwords_passkey_links`), `HOLZI_TRIGGER_VERSION`
15 → 16. Keine Änderung vorhandener Tabellen. Details in [data-model.md](./data-model.md).

**Testing**: Rust — Einheitstests in `*_tests.rs` (`references`, `webauthn`, `copy`,
`passkeys_ops`, `import/references`), Integration in `src-tauri/tests/`
(`passwords_references.rs`, `passwords_copy.rs`, `passwords_passkeys.rs`); Frontend —
`pnpm check:passwords` mit neuen Skripten
(`-menus`, `-shortcuts`, `-breadcrumb`, `-clipboard`, `-tabs`), Regression
`check:agent-actions`, `check:wm-navigation`, `check:templates`, `typecheck`,
`typecheck:scripts`, `lint`, `format:check`; End-to-End neu `passwords-tabs`,
`passwords-organize`, `passwords-references`, `passwords-passkeys`, `passwords-attachments`,
dazu `passwords-narrow-window` und `passwords-session-restore` erweitert, die übrigen
vorhandenen unverändert;
manuell nach [quickstart.md](./quickstart.md) (Wischgeste, Lightbox-Gesten, fremde Programme)

**Target Platform**: Tauri-Desktop (Linux, macOS, Windows); Wisch- und Langdruck-Bedienung
zielt auf Berührung (Android/iOS sind Ziel der Plattform, der Passwortmanager selbst ist
dort noch durch die Dateipfade aus 034 R4 begrenzt). Neue Crates müssen in beiden
Cargo-Konfigurationen bauen (Standard und `--no-default-features`).

**Project Type**: desktop-app (Nuxt-SPA-Frontend + Rust-Backend in einem Tauri-Projekt)

**Performance Goals**: grobe Zielgrenzen, keine Messaufgaben: Tabwechsel ohne spürbare
Verzögerung; Kopieren und Verschieben von mehreren hundert Einträgen in einer
Transaktion in Sekunden; die Verwendungsabfrage für Verweise (Tabellenscan über wenige
Spalten) bei 5.000 Einträgen unmerklich; ein 5-MiB-Bild öffnet in der Lightbox in unter
einer Sekunde (SC-008)

**Constraints**: Dateien ≤ 500 Zeilen (vorhandene Überschreitungen: `items.rs` 687, `model.rs`
662, `snapshots.rs` 595, `import/apply.rs` 677, `identity/migrations.rs` 591,
`EntryEditor.vue` 645 — neue Logik geht in neue Dateien, `EntryEditor.vue` schrumpft);
Testcode in eigenen Dateien; `src/lib/passwords/*` bleibt reines TS mit relativen
`.ts`-Importen; kein lokalisierter Text im Backend (der Standardzusatz „Kopie“/„Copy“ kommt
vom Frontend); kein Geheimnis in Fehler, Protokoll, Ereignis, `Debug`-Ausgabe, Pfad,
Abfrage oder Fenstertitel; ein aufgelöster Verweis nie im Verlauf, nie in Listen; kein
Passkey-Schlüssel in einer Antwort; keine `unwrap`/`expect` auf Eingabedaten; Aufrufer nie
als Command-Argument; die Grammatik der Verweise genau einmal (Rust)

**Scale/Scope**: 2 Tabellen, 6 neue und 10 geänderte Commands, 3 Dienstmethoden ohne
Command, rund 16 neue Rust-Dateien (dazu Tests), rund 18 neue Vue-Komponenten, 5 neue reine
TS-Module, 6 Prüfskripte, 3 Rust-Integrationstests, 5 neue End-to-End-Szenen

## Constitution Check

_GATE: Muss vor Phase 0 bestehen. Nach Phase 1 erneut geprüft — Ergebnis unten._

Geprüft gegen `.specify/memory/constitution.md` (v1.4.0) und die spaex-Constitution
`.spaex/constitution.md`.

| Prinzip / Vorgabe                                                          | Status | Begründung                                                                                                                                                                         |
| -------------------------------------------------------------------------- | ------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| I Keine Geheimnisse in Git                                                 | ✅     | Testvektoren, Schlüssel und Fixtures sind erfunden (`SECRET-MARKER-…`, feste Testschlüssel ohne Bezug zu Konten); keine echten Passkeys                                            |
| II Keine lokalen absoluten Pfade in versionierter Konfiguration            | ✅     | Quickstart nutzt repo-relative Pfade; keine Capability ändert sich                                                                                                                 |
| III Projektidentität geräteunabhängig                                      | ✅     | Berührt nicht; keine Geräte-Kennung wird für den synchronisierten Signaturzähler benötigt                                                                                          |
| IV Cross-Repo-Referenzen an unveränderliche SHAs gepinnt                   | ✅     | haex-vault 2.x weiterhin `8dce379d94e18fcd42c3b73686a06f984ca3f574`; KeePass-Quelle nur als gelesener Beleg (Mirror), nicht als Abhängigkeit                                       |
| V Externe Quellen nur per Opt-in                                           | ✅     | Keine neue Harness-Quelle; neue Bibliotheken sind Abhängigkeiten, keine Harness-Inhalte                                                                                            |
| VI Selbstverändernde Anweisungen review-pflichtig                          | ✅     | Keine Änderung an Constitution, Skills oder Berechtigungen; Freigaben bleiben bei 017–019 und 021                                                                                  |
| VII Relay-Ausfall blockiert lokale Arbeit nicht                            | ✅     | Alles läuft lokal; der synchronisierte Zähler bleibt ohne Sync sicher bei 0                                                                                                        |
| VIII Keine Verheimlichung in Agent-Ausgaben                                | ✅     | Der eingebaute Agent bekommt keine neue Aktion; Fehlerzustände von Verweisen sind sichtbar und benannt, nie still leer                                                             |
| Workflow: speckit-Stufen, PR auf `main`, Conventional Commits, kein Squash | ✅     | specify → clarify → plan → tasks → implement; Topic-Branch im Worktree `.worktrees/036-password-redesign`; mehrere PRs (siehe Lieferung)                                           |
| ADR bei prinzipienrelevanter Entscheidung                                  | ✅     | ADR-0009 „Verweise im Dienst aufgelöst, Passkey-Dienst“ (Aufgabe in tasks), erweitert ADR-0007                                                                                     |
| Test-Code in separaten Dateien                                             | ✅     | `*_tests.rs` per `#[path]`, `src-tauri/tests/`, `scripts/check-passwords-*.ts`                                                                                                     |
| Worktree je Änderung                                                       | ✅     | `.worktrees/036-password-redesign`                                                                                                                                                 |
| 500-LoC-Grenze                                                             | ⚠️     | Vorhandene Dateien über 500 Zeilen wachsen nicht (neue Dateien); `EntryEditor.vue` (645) wird kleiner; `identity/migrations.rs` bleibt (nicht von dieser Spec)                     |
| Graphify vor neuen benannten Artefakten                                    | ✅     | Aufgabe T002: Abfragen für Tab-Rahmen, Karten, Menüs, Auflöser-Hooks, Kopieren, Passkey-Dienst, bevor die ersten Dateien entstehen                                                 |
| `ponytail:`-Kommentar bei bewusster Vereinfachung                          | ✅     | Geplant an: Marken unter dem Feld statt im Text (R11), Vorschaubilder im Frontend (R14), konstante Zähler und kein UP/UV ohne Presence-Nachweis (R5, R8), Beglaubigung `none` (R8) |
| Nicht-triviale Logik hinterlässt einen ausführbaren Check                  | ✅     | `references_tests`, `webauthn_tests`, `copy_tests`, `passwords_passkeys`, `passwords_sync`, `check:passwords`, End-to-End                                                          |

**Ergebnis nach Phase 1**: keine Verletzung. Das ⚠️ bei der 500-Zeilen-Grenze besteht schon,
diese Spec verschlechtert es nicht (neue Logik in neuen Dateien; die größte Datei, die sie
anfasst, wird kleiner).

## Lieferung in Stufen (jede ein PR, jede auslieferbar)

| Stufe | Inhalt                                                                                    | Stories             | Berührt                                                                   |
| ----- | ----------------------------------------------------------------------------------------- | ------------------- | ------------------------------------------------------------------------- |
| 1     | Eintrag in Tabs mit Wischgeste, Verlauf als Tab, Zerlegung des Editors                    | US1, US2            | Frontend; Registry (`tab`); keine Rust-Änderung                           |
| 2     | Brotkrumen, Auswahlleiste, Ablage (Ausschneiden, Einfügen), Kontextmenüs, Kürzel          | US3, US4            | Frontend; keine Rust-Änderung                                             |
| 3     | Verweise (Auflöser, Marken, Editor, Löschen), Kopieren mit Dialog, KeePass-Verweise       | US3 (Kopieren), US7 | Rust `references`, `copy`, `import/references`; Frontend; keine Migration |
| 4     | Passkeys: Dienstmethoden, konstanter Zähler 0, Verbindungen, Passkey-Ansicht im Tab Extra | US5                 | Rust `webauthn`, `passkeys_ops`; Migration `0026`; Frontend               |
| 5     | Anhänge als Karten mit Lightbox                                                           | US6                 | Frontend (`photoswipe`)                                                   |

Die Stufen 1 und 2 brauchen kein Rust; die Migration `0026` liegt einmal vor (Stufe 4) und
enthält die Passkey-Verbindungen, Stufe 3 nutzt von ihr nichts. Der Kopier-Dialog in Stufe 3 bietet
„Passkeys per Verweis“ erst an, wenn Stufe 4 da ist (bis dahin ausgeblendet).

## Project Structure

### Documentation (this feature)

```text
specs/036-password-redesign/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── references.md          # Grammatik, Auflösen, Speichern, Import, Testvektoren
│   ├── passkey-service.md     # anlegen, bestätigen, auflisten
│   └── tauri-commands.md      # neue und geänderte Commands
├── checklists/requirements.md
└── tasks.md                   # Phase 2 (/speckit-tasks)
```

### Source Code (repository root)

```text
src-tauri/src/
├── passwords/
│   ├── references.rs, references_tests.rs      # Grammatik, Auflösen, Kreise, Tiefe (rein)        [neu]
│   ├── references_db.rs                        # Abfragen: Quelle laden, Verwendung, Einsetzen     [neu]
│   ├── copy.rs, copy_tests.rs                  # Tiefenkopie in einer Transaktion                  [neu]
│   ├── webauthn.rs, webauthn_tests.rs          # authData, COSE, Signatur, Herkunft (rein)         [neu]
│   ├── passkeys_ops.rs                         # create/confirm/list gegen die Datenbank           [neu]
│   ├── passkey_links.rs, passkey_links_tests.rs # Verbindungen (Ziel/Quelle), Löschen, Kopie         [neu]
│   ├── model_references.rs, model_passkeys.rs  # RefMark, ReferenceUsage, PasskeyHeader (ts-rs)    [neu]
│   ├── service/copy.rs, service/references.rs  # nur Nutzer: copy, parse, token, usage, key_names  [neu]
│   ├── service/passkeys.rs                     # + create, confirm, list, unlink                    [ändern]
│   ├── commands/copy.rs, commands/references.rs# Tauri-Hüllen                                       [neu]
│   ├── reveal.rs                               # löst Platzhalter auf                               [ändern]
│   ├── items.rs                                # Kreisprüfung beim Speichern, headers_in_scope      [ändern, klein]
│   ├── trash.rs                                # purge: Verbindungen zuerst                         [ändern, klein]
│   └── import/references.rs, references_tests.rs, apply_references.rs (Kennungen vorab, Umwandeln vor dem ersten Schreiben), keepass.rs (source_ref) [neu / ändern]
├── identity/migrations_passwords_refs.rs       # 0026: Passkey-Verbindungen                         [neu]
├── identity/migrations.rs                      # Registrierung, Triggerversion 16                   [ändern]
├── Cargo.toml                                  # ciborium, psl, url                                 [ändern]
└── tests/passwords_references.rs, passwords_copy.rs, passwords_passkeys{,_scope,_sync}.rs, common/passkey_fixture.rs, fixtures/reference_vectors.json [neu]

src/
├── lib/passwords/
│   ├── menus.ts, shortcuts.ts, breadcrumb.ts   # reine Bausteine (Node-Tests)                       [neu]
│   ├── clipboard.ts                            # Ablage: Zustandsmaschine, Kreis- und Fehlprüfung   [neu]
│   ├── thumbnails.ts                           # Vorschaubilder (LRU, Warteschlange)                [neu]
│   ├── registry.ts                             # Abfrageschlüssel `tab`                             [ändern]
│   └── search.ts                               # Platzhalter wegfalten                              [ändern]
├── stores/passwordsClipboard.ts                # Ablage                                             [neu]
├── composables/usePasswords.ts                 # + copy, references_*, usage, unlink                [ändern]
├── composables/usePasswordsThumbnails.ts      # Vorschaubilder verkleinern (Canvas), ein Speicher  [neu]
├── composables/usePasswordsActions.ts          # eine Handler-Menge für Leiste, Menü und Kürzel     [neu]
├── composables/usePasswordsShortcuts.ts, usePasswordsListKeys.ts, usePasswordsMenuText.ts # Kürzel (Rahmen, Liste), Kürzeltext [neu]
├── components/passwords/
│   ├── EntryTabs.vue, EntryTabsSwiper.vue, ViewDetails.vue, ViewExtra.vue, EditorDetails.vue, EditorExtra.vue [neu]
│   ├── HistoryTimeline.vue, HistorySnapshot.vue                                                     [neu; HistoryView.vue entfällt]
│   ├── Breadcrumbs.vue, SelectionBar.vue (ersetzt SelectionToolbar.vue), EntryMenu.vue, EntryMenuButton.vue, ClipboardBar.vue, FolderRow.vue [neu / ersetzt]
│   ├── CopyDialog.vue, ReferenceValue.vue, ReferenceField.vue, ReferencePicker.vue, ReferenceUsageNote.vue [neu]
│   ├── AttachmentCard.vue, AttachmentLightbox.vue, PasskeyDeleteNote.vue                            [neu]
│   ├── EntryEditor.vue, EntryView.vue, EntryPage.vue, List.vue, ListItem.vue, Sidebar.vue, TreeItem.vue, TrashView.vue, Passkeys.vue, Attachments.vue, DeleteDialog.vue, EmptyTrashDialog.vue [ändern]
│   └── components/apps/PasswordsApp.vue        # Kürzel, Ablage-Lebensdauer                         [ändern]
├── i18n/locales/{de,en}.json                   # passwords.tabs, breadcrumb, menu, clipboard, copy, references, lightbox, errors [ändern]
├── composables/useErrorString.ts               # ReferenceError, ReferenceCycle, IntoTrash          [ändern]
└── types/bindings/*.ts                         # ts-rs                                              [generiert]

scripts/
├── check-passwords-menus.ts, -shortcuts.ts, -breadcrumb.ts, -clipboard.ts, -tabs.ts, -thumbnails.ts [neu]
└── e2e/scenarios/passwords-{tabs,organize,references,passkeys,attachments}.test.ts, lib/passwords.ts (+ Hilfen), lib/png.ts [neu / ändern]

docs/adr/0009-references-resolved-in-service-and-passkey-service.md                                  [neu]
package.json                                    # swiper, photoswipe                                 [ändern]
```

**Structure Decision**: Reine Logik (Grammatik, Herkunft, Menüs, Kürzel, Brotkrumen) getrennt
von Datenbank und Vue, damit sie ohne Datenbank und ohne Fenster prüfbar ist. Neue Logik in
neue Dateien (die großen vorhandenen wachsen nicht). Die Oberfläche ruft für Verweise nur
Commands; kein Teil der Grammatik liegt im Frontend.

## Complexity Tracking

| Verstoß / Aufwand                                                         | Warum nötig                                                                                                                                  |
| ------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------- |
| Eine neue CRDT-Tabelle und Migration                                      | Passkey-Verbindungen (FR-046, R6); synchronisierte Signaturzähler bleiben 0 (R5)                                                             |
| Drei neue Rust-Abhängigkeiten (`ciborium`, `psl`, direkt `url`)           | CBOR für COSE und Beglaubigung, öffentliche Suffixliste für die Herkunftsprüfung, URL-Zerlegung (R8, R9)                                     |
| Zwei neue Frontend-Abhängigkeiten (`swiper`, `photoswipe`)                | Wischgeste (Vorgabe) und Lightbox mit Zoom und Gesten; beides wie in haex-vault 2.x, nur bei Bedarf geladen (R1, R14)                        |
| Ein Auflöser mit Bereichsprüfung an der Quelle und Kreiserkennung         | FR-044 bis FR-049: ein Verweis darf kein Zugriffsrecht geben, nie leerer Text, nie ein aufgelöstes Geheimnis in Listen oder Verlauf (R3, R4) |
| Zerlegen von `EntryEditor.vue`, `HistoryView.vue`, `SelectionToolbar.vue` | Die Tabs verlangen es; zugleich bringt es die Datei unter 500 Zeilen (R16)                                                                   |

## Bewusste Grenzen (aus research.md)

- Synchronisierte Passkeys senden immer den Zähler 0; damit gibt es kein unzuverlässiges
  Duplikat eines Nicht-Null-Werts zwischen offline bestätigenden Geräten (R5).
- RS256-Passkeys erscheinen, werden aber nicht bestätigt (R8); `rsa` ist nur ein
  Release-Kandidat.
- Beglaubigung `none`; Gegenstellen, die eine Beglaubigung verlangen, werden nicht bedient (R8).
- Kein UP- oder UV-Flag ohne vertrauenswürdigen Presence-Nachweis; Gegenstellen, die diese
  Flags verlangen, lehnen ab (R8).
- Marken der Verweise stehen unter dem Feld, nicht im Text (R11).
- Vorschaubilder entstehen im Webview aus den vollen Bytes, höchstens zwei zugleich (R14).
- Die Wischgeste ist nur manuell geprüft (WebKit-Webview unter Linux und Android, R1, R17).
- Die Verwendungsabfrage für Verweise ist ein Tabellenscan, kein Index (R12).
- Ein Verweis in einem alten Stand des Verlaufs kann eine Quelle meinen, die es nicht mehr
  gibt; Wiederherstellen meldet es und stellt den Rest her.
- Die Passkey-Funktionen haben heute keinen Aufrufer außer Tests; Erweiterungen, MCP und die
  External Bridge docken über 017–019, 021 und die Bridge-Spec an.
- Keine Browser-Anbindung, kein Export, keine PDF-Vorschau, keine Inaktivitätssperre (Spec).
