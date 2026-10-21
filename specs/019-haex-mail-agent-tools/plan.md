# Implementation Plan: haex-mail für den Agenten, Regeln und Regel-Oberfläche

**Branch**: `019-haex-mail-agent-tools` | **Date**: 2026-10-20 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/019-haex-mail-agent-tools/spec.md`

## Summary

haex-mail erklärt 15 Werkzeuge im signierten Manifest und stellt sie über den MCP-Baustein des vault-sdk
bereit (018). Dazu kommen dauerhafte Regeln mit eigener Oberfläche in den Einstellungen. Drei Mail-Funktionen
fehlen dem Host und werden in holzi und vault-sdk ergänzt. Die Arbeit liegt in drei Repositories und ist in
drei Etappen geschnitten, jede mit eigenem Pull Request (FR-034).

Technischer Ansatz (Begründungen in [research.md](./research.md)):

- **Etappen** (R1): E1 vault-sdk veröffentlicht den MCP-Baustein; E2a holzi bekommt drei Bridge-Methoden,
  E2b vault-sdk die Gegenstücke; E3 haex-mail.
- **Host** (R2–R4): `extension_mail_search` (Kriterienobjekt, UIDs, höchstens 1000), `extension_mail_create_mailbox`
  (`created`-Flag), `extension_mail_delete_messages` (nur mit `UIDPLUS`, nur die genannten UIDs). Berechtigung
  `fetch`. Verträge: [contracts/host-mail-commands.md](./contracts/host-mail-commands.md).
- **Werkzeuge** (R5–R9a, R11, R12): Handler in `app/tools/` nach Themen, Registrierung in einem Plugin, Warten
  auf `initializeAsync()`, dieselben Stores wie die Oberfläche. Server statt Cache beim Suchen und bei den
  Zählern. Entwurf als Ordner-Eintrag, Senden und Löschen mit Eingabe, die der Handler gegen die
  Wirklichkeit prüft, damit die Zustimmungsabfrage verständlich bleibt. Verträge:
  [contracts/manifest-tools.md](./contracts/manifest-tools.md).
- **Regeln** (R10): reines Modul `lib/rules.ts`, Tabelle `rules` und Spalte `mailboxes.rules_uid_through`
  (Migration 0003), Auswertung im Watch-Handler und im normalen Abruf, nur INBOX. Modell:
  [data-model.md](./data-model.md).
- **Oberfläche** (FR-027 bis FR-031): Abschnitt „Regeln“ in `pages/settings.vue`, Komponenten `RulesSection.vue`
  und `RuleDialog.vue` nach dem Muster der Konten, Texte in `<i18n>`-Blöcken (de/en).
- **Prüfen** (R12, R13): Rust-Tests gegen den erweiterten Testserver, vitest in vault-sdk und haex-mail, Eval-Sätze
  für SC-001 bis SC-003.

## Technical Context

**Language/Version**: Rust (Edition des Crates `src-tauri`) für E2a; TypeScript (strict) für E1, E2b, E3; Vue 3.5,
Nuxt 4 (SPA) in haex-mail

**Primary Dependencies**: vorhanden — `async-imap`, `imap-proto`, `mail-parser`, `tokio` (holzi);
`drizzle-orm`, `drizzle-kit`, Pinia, vue-i18n, vitest (haex-mail); keine neue Laufzeitabhängigkeit

**Storage**: Erweiterungs-DB von haex-mail: neue Tabelle `rules`, neue Spalte `mailboxes.rules_uid_through`
([data-model.md](./data-model.md)); holzi und vault-sdk ohne Speicheränderung

**Testing**: `cargo test --manifest-path src-tauri/Cargo.toml --no-default-features --lib mail` (neue Fälle in
`extensions/mail/mail_tests.rs`, Testserver `test_server.rs` erweitert), `bridge/allowlist.rs`-Test; vault-sdk:
vitest `src/api/__tests__/mail.test.ts`; haex-mail: vitest (`lib/rules.test.ts`, `tools/*.test.ts`,
Manifest-Test); Modellprüfung `cargo test --manifest-path src-tauri/Cargo.toml --test model_tool_eval -- --ignored --nocapture`

**Target Platform**: wie 018: Linux, macOS, Windows, Android, iOS. haex-mail läuft in holzi als Rahmen (iframe)

**Project Type**: Desktop- und Mobil-App (Tauri, holzi), Bibliothek (vault-sdk), Erweiterung (haex-mail, Nuxt)

**Performance Goals**: `find_messages` mit Suche über einen Ordner von 10 000 Mails und eine Seite von 25
Umschlägen in ≤ 5 s bei erreichbarem Server; `list_mailboxes` für 3 Konten in ≤ 5 s; ein lesender Aufruf bei
geschlossenem haex-mail ≤ 3 s langsamer als bei offenem (SC-009); Regelauswertung für 50 neue Mails mit 20
Regeln in ≤ 50 ms (ohne Serveraufruf)

**Constraints**: Schema nur `type`, `description`, `properties`, `required`, `items`, `enum` (holzi
`schema_in_subset`); Ergebnis eines Werkzeugs ≤ 64 KiB; höchstens 1000 Umschläge je `fetch_envelopes`;
höchstens 100 Mails je Aufruf der ändernden Werkzeuge; 60 s je Host-Aufruf; Dateien < 500 Zeilen;
Rust-Tests in `*_tests.rs`; Regeln laufen nur bei geladenem haex-mail (Clarification 2026-10-20)

**Scale/Scope**: bis 10 Konten, Ordner bis 10 000 Mails, bis 50 Regeln

## Constitution Check

_GATE: Must pass before Phase 0 research. Re-check after Phase 1 design._

| Prinzip                                          | Bewertung                                                                                                                                                                                                                                                                                                                               |
| ------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| I. Keine Geheimnisse in Git                      | Besteht. Kein Werkzeug nimmt ein Passwort an (FR-018, FR-019); Test-Passwörter in Tests sind erkennbare Marker, keine Zugangsdaten. Der Testserver nutzt feste Test-Logins ohne Außenwirkung.                                                                                                                                           |
| II. Keine lokalen absoluten Pfade                | Besteht. Alle Verweise in dieser Spec und in den Artefakten sind repository-relativ oder nennen Repository, Commit und Pfad.                                                                                                                                                                                                            |
| III. Projektidentität ist geräteunabhängig       | Besteht. Regeln und `rules_uid_through` liegen in der Vault-DB und gelten geräteübergreifend; keine Pfade im Zustand.                                                                                                                                                                                                                   |
| IV. Fremde Verweise nennen feste Commits         | Besteht. [research.md](./research.md) nennt haextension `e2622bae…` und vault-sdk `cf2dbcb8…`/`ee0a0306…` mit vollem Hash (inzwischen ist der Baustein als Release 4.6.0 `74a8ff96…` veröffentlicht). Die Pins in Etappe 2 und 3 (`haex-bundle`, `@haex-space/vault-sdk`) verweisen auf veröffentlichte Versionen mit dem Commit im PR. |
| V. Externe Quellen nur per Allowlist             | Besteht. Es kommt keine Quelle zu `.haex-hive.json` hinzu.                                                                                                                                                                                                                                                                              |
| VI. Selbstverändernde Anweisungen nur mit Review | Besteht. Die Änderung am Marker in `CLAUDE.md` (Speckit-Verweis auf diesen Plan) ist Teil des Pull Requests dieser Spec; Skills, Berechtigungen und Constitution bleiben unverändert.                                                                                                                                                   |
| VII. Relay ist nie Voraussetzung                 | Besteht. Regeln und Werkzeuge nutzen nur lokale Daten und die Mailserver des Nutzers, kein Relay.                                                                                                                                                                                                                                       |
| VIII. Keine verdeckten Anweisungen               | Besteht. Werkzeuge liefern Mailtext als Daten; die Spec verbietet, dass Mailtext Wirkungsart oder Freigabe ändert (US1/4). Beschreibungen im Manifest enthalten keine versteckten Anweisungen an Dritte.                                                                                                                                |
| Arbeitsweise                                     | Pull Request je Etappe aus einem Themen-Branch, Rebase- oder Merge-Commit, Conventional Commits. Spec 019 in holzi deckt E2a und die Verträge ab; E1, E2b, E3 laufen in vault-sdk und haextension.                                                                                                                                      |

### Prüfung gegen die spaex-Constitution (`.spaex/constitution.md`)

| Regel                                                                                           | Bewertung                                                                                                                                                                                                                                                                           |
| ----------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Änderungen nur in einem Worktree auf einem Themen-Branch; Hauptcheckout nur zum Lesen           | Besteht für Plan und Umsetzung (Worktree `.worktrees/019-haex-mail-agent-tools`). Die ersten zwei Commits der Spec (`59a3d0ce` Entwurf, `7d813aa6` Spec) entstanden im Hauptcheckout und wurden in den Worktree übernommen (`3de0b3bd`); die Abweichung ist dem Betreiber gemeldet. |
| Tests in eigenen Dateien                                                                        | Besteht: Rust-Tests in `*_tests.rs`, vitest in `*.test.ts` neben dem Modul.                                                                                                                                                                                                         |
| Dateien ≤ 500 Zeilen                                                                            | Besteht: Handler nach Themen geteilt, Regelmodul getrennt von Store und Oberfläche. `imap.rs` hat heute 460 Zeilen; die drei neuen Funktionen gehören in ein neues Modul `imap_manage.rs`, damit `imap.rs` unter 500 bleibt (Anpassung der Struktur oben).                          |
| Graphify vor neuen Artefakten                                                                   | Besteht: R14 (Kandidaten, ein grenzwertiger Fall mit Vorschlagspflicht).                                                                                                                                                                                                            |
| Pflichtprüfungen vor Übergabe (`cargo fmt --check`, `clippy`, `test`; Typprüfung, Lint, vitest) | In `tasks.md` je Etappe als Aufgabe; Prettier auf jeder berührten Markdown- und JSON-Datei (`pnpm exec prettier --write`).                                                                                                                                                          |
| Keine Verweise auf Agenten, Modelle oder Laufzeit in Artefakten und Commits                     | Besteht: Commit-Nachrichten ohne Trailer, Artefakte ohne solche Verweise.                                                                                                                                                                                                           |
| Verweise auf fremde Repositories mit Repository, vollem Commit und Pfad                         | Besteht (siehe oben, `research.md` Kopf).                                                                                                                                                                                                                                           |
| Pull Requests ohne Squash-Merge, Conventional Commits                                           | Besteht (Arbeitsweise).                                                                                                                                                                                                                                                             |
| Keine Geheimnisse, keine lokalen Pfade                                                          | Besteht; `research.md` und Verträge nutzen repository-relative Pfade.                                                                                                                                                                                                               |

Ergebnis vor und nach Phase 1: keine Verstöße gegen beide Constitutionen. Die Tabelle „Complexity Tracking“
entfällt.

## Project Structure

### Documentation (this feature)

```text
specs/019-haex-mail-agent-tools/
├── plan.md              # Dieser Plan
├── research.md          # Phase 0: Entscheidungen R1 bis R13
├── data-model.md        # Phase 1: Tabelle rules, Spalte rules_uid_through, Ergebnisformen
├── quickstart.md        # Phase 1: manuelle Prüfung
├── contracts/
│   ├── host-mail-commands.md   # drei neue Bridge-Methoden (E2a/E2b)
│   └── manifest-tools.md       # die 15 Werkzeuge (E3)
├── checklists/requirements.md
└── tasks.md             # Phase 2 (speckit-tasks, nicht Teil dieses Befehls)
```

### Source Code

```text
# E2a — holzi (dieses Repository)
src-tauri/src/extensions/mail/
├── commands.rs          # + search, create_mailbox, delete_messages
├── imap.rs              # unverändert bis auf `pub(super)`-Zugriff auf `with_session!` und `imap_err`
├── imap_manage.rs       # search, create_mailbox, delete_messages (UIDPLUS-Prüfung zuerst)
├── types.rs             # + SearchCriteria, SearchResult
├── test_server.rs       # + SEARCH mit Kriterien, CREATE, UID STORE \Deleted, UID EXPUNGE
└── mail_tests.rs        # + Fälle laut contracts/host-mail-commands.md
src-tauri/src/extensions/bridge/allowlist.rs    # + drei Methoden
specs/017-extension-host/contracts/bridge.md     # + drei Namen in der Mail-Zeile
src-tauri/src/chat/eval/{eval_set,extension_tools}.json   # + Sätze und Werkzeuge von haex-mail (FR-035)

# E1, E2b — vault-sdk (haex-space/vault-sdk)
src/commands/mail.ts                 # + search, createMailbox, deleteMessages
src/api/mail.ts                      # + searchMessagesAsync, createMailboxAsync, deleteMessagesAsync
src/types/mail.ts                    # + MailSearchCriteria, MailSearchResult
src/api/__tests__/mail.test.ts       # vorhanden; + Methoden gegen gespielten Client

# E3 — haextension (haex-space/haextension), apps/haex-mail
haextension/manifest.json            # + tools (15), neu signieren
app/database/schemas/index.ts        # + rules, mailboxes.rulesUidThrough
app/database/migrations/0003_*.sql   # drizzle-kit generate
app/lib/rules.ts (+ rules.test.ts)   # Prüfung, Auswertung, Zusammenfassung
app/stores/rules.ts                  # Regeln lesen und schreiben (Oberfläche und Werkzeuge)
app/stores/mail/rules.ts             # Regeln auf neue INBOX-Mails (Watermark)
app/lib/messageQuery.ts (+ .test.ts)   # Suchkriterien, Seiten, Zusammenführen über Konten
app/tools/{accounts,mailboxes,messages,readMessage,messageChanges,drafts,sendMail,deleteMessages,rules}.ts (+ *.test.ts)
app/plugins/agent-tools.client.ts    # registriert die Handler
app/components/{RulesSection,RuleDialog}.vue
app/pages/settings.vue               # + Abschnitt „Regeln“
app/locales/{de,en}.json             # + Texte
```

**Structure Decision**: Ein Plan in holzi, weil holzi die Spec-Verzeichnisse führt und die Host-Seite hier
liegt. Die beiden anderen Repositories führen eigene Pull Requests und verweisen auf diese Spec; eine
gemeinsame Aufgabenliste (`tasks.md`) markiert je Aufgabe das Repository (wie 018).

## Etappen

| Etappe | Repository  | Inhalt                                                                                                                                                                                                               | Abhängigkeit                                                                 |
| ------ | ----------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------- |
| E1     | vault-sdk   | **erledigt**: Release 4.6.0 (`74a8ff96…`), MCP-Baustein aus PR #64                                                                                                                                                   | keine                                                                        |
| E2a    | holzi       | **erledigt**: drei Bridge-Methoden, Testserver, Tests, Verträge (haex-space/haex-vault, Merge `a1f3ae20b6538cdddb66b2c718ccf0660c04d68e`, PR #507)                                                                   | keine                                                                        |
| E2b    | vault-sdk   | **erledigt**: drei Methoden und Typen, Tests (haex-space/vault-sdk, Commit `3dde451472376ee7bcd834b690431e712b01743d`, PR #78), veröffentlicht als Release 4.7.0 (Commit `ccdaadd06e9a0dd3bd392a31e45f3f0d99ac4f2e`) | E1 veröffentlicht; Namen aus E2a festgelegt                                  |
| E3     | haextension | Manifest, Migration, Regeln, Werkzeuge, Oberfläche; SDK anheben                                                                                                                                                      | E1 und E2b veröffentlicht, E2a in der holzi-Version, gegen die getestet wird |

E2a und E2b gelten erst zusammen. Tests von E3 laufen gegen eine holzi-Version aus E2a (Entwicklungsmodus
von haex-mail, `haex dev`).

## Risiken

1. **15 Werkzeuge gegen ein Angebot von höchstens drei je Nachricht (018 R6).** Zweischritt-Aufgaben
   („suche und verschiebe“) hängen davon ab, dass beide Werkzeuge in den drei liegen. Die Beispielsätze sind die
   Stellschraube (R12). Zeigt die Messung (SC-001) Probleme, ist das ein Befund für 018, nicht für diese Spec.
2. **Nicht-ASCII-Suche** (R2): Fällt das Literal im `UID SEARCH` aus, gilt der lokale Rückfall; SC-004 gilt dann
   nur für ASCII. Die erste Aufgabe von E2a klärt es und wird in `research.md` nachgetragen.
3. **`open_account_setup` bei geschlossenem Fenster** (R11): Der Assistent erscheint erst, wenn der Nutzer das
   Fenster öffnet. Das Ergebnis sagt es dem Agenten. Zeigt die Erprobung, dass das nicht reicht, gehört eine
   Host-Fähigkeit „Fenster in den Vordergrund“ in eine eigene Spec.
4. **Regeln ohne geöffnetes haex-mail laufen nicht** (Clarification 2026-10-20): bewusst, in den Annahmen
   der Spec festgehalten.
5. **Zustimmung zeigt nur die Eingabe** (R9a): Wir lösen es mit Eingaben, die der Handler prüft. Weicht das
   Modell im Text leicht ab, scheitert das Senden mit Fehler und neuer Versuch; die Messung (FR-035) deckt auf,
   ob kleine Modelle das schaffen.
6. **Doppelte Kopie in „Gesendet“** (E3, `send_mail`): Die Spec verlangt die Kopie (FR-014), und der Dialog zum
   Verfassen legt keine an. Server, die gesendete Mails selbst ablegen (zum Beispiel Gmail), zeigen sie danach
   zweimal. Zeigt die Erprobung (Quickstart Schritt 6) das, braucht es eine Erkennung dieser Server oder eine
   Einstellung je Konto; das ist dann eine eigene Entscheidung.
7. **Angebot der Werkzeuge** (E3): Die Beispielsätze der vier Werkzeuge für Suche, Konten und Zweischritt-Aufgaben
   wurden mit dem Angebotsvergleich (`matched_offer`) abgestimmt, so dass jeder der 30 Sätze für haex-mail das
   erste Angebot erreicht. Ob das Modell dann richtig aufruft, zeigt erst die Messung mit Qwen3-4B (T056).
