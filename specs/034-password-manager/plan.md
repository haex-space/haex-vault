# Implementation Plan: Passwortmanager

**Branch**: `034-password-manager` | **Date**: 2026-10-02 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/034-password-manager/spec.md`

## Summary

holzi bekommt einen festen Passwortmanager als App im Window Manager. Vorlage ist der
Passwortmanager von haex-vault 2.x (Legacy), dessen Datenmodell (zwölf Tabellen `haex_passwords_*`)
fast unverändert übernommen wird; Oberfläche, Datenzugriff und Zugriffsprüfung sind neu
gebaut, weil holzi weder Drizzle im Frontend noch ein Freigabesystem hat. Den
Umfang bestimmt die Spec (US1–US8); die External Bridge ist ausdrücklich nicht dabei.

Technischer Ansatz (Begründungen und verworfene Alternativen in
[research.md](./research.md)):

- **Datenmodell und Migration** (R1–R4): Migration `0022_passwords` (SQL in eigener Datei,
  `identity/migrations.rs` steht schon über 500 Zeilen), Triggerversion 14. Abweichungen von
  haex-vault 2.x, alle begründet: Binärdaten als BLOB, zwei Spalten für die Herkunft im
  Papierkorb, **keine UNIQUE-Constraints** (ein UNIQUE-Konflikt hält den Sync an) und
  stattdessen abgeleitete Kennungen (UUIDv5) für Tags, Tag-Zuordnungen und Passkeys,
  `ON DELETE RESTRICT` bei den Binärverweisen. Endgültiges Löschen entfernt Kinder
  ausdrücklich und zuerst, weil der Sync entfernte Löschungen ohne Fremdschlüssel anwendet.
- **Anhänge** (R4, R18): 25 MiB, Prüfung der Dateigröße **vor** dem Lesen, Übergabe über
  Pfade aus Datei-Dialogen statt Bytes durch den Webview, jeder Anhang in einem eigenen
  `write`, Aufräumen verwaister Binärdaten beim Öffnen mit Karenzzeit von sieben Tagen
  (sonst löscht ein Gerät, was ein anderes gleich verknüpft). Vorschau nur für Bilder.
- **Zugriff** (R6, R14): reines Rust-Modul `passwords/access.rs` (`Caller`, `Grant`, `Scope`)
  und ein `PasswordsService` als **einziger Eingang** für alles, auch die Oberfläche (Aufrufer
  `User`); für Funktionen jenseits von Eintrag lesen, anlegen, ändern und löschen sind andere
  Aufrufer gesperrt (Z11). Der Aufrufer ergibt sich aus dem Eingang, nie aus einem Argument. Der eingebaute Agent bekommt über
  eine einzige lesende Aktion `passwords.items.search` nur Titel, Tags und Ordnernamen.
  Verwaltung von Freigaben bleibt bei 017–019 und 021.
- **Geheimnisse bleiben im Backend** (R7–R9): Standardlesen ohne Geheimnisse, `reveal` auf
  Handlung, Kopieren und TOTP-Code in Rust, Zwischenablage mit Löschen in Rust
  (`tauri-plugin-clipboard-manager` nur Rust-seitig), Teil-Update beim Speichern.
- **Verlauf, Papierkorb, Konflikte** (R3, R5, R15): ein Verlaufsformat, Wiederherstellen neu
  gebaut (haex-vault 2.x kann es nicht), Papierkorb merkt den früheren Ort, Speichern mit
  `expectedUpdatedAt` erkennt Änderungen und Löschen durch ein anderes Gerät.
- **Import** (R12): in Rust und **verlustfrei**: alles aus der Quelle (Papierkorb, Verlauf,
  Symbole, alle Passkeys, ungültiges TOTP, alle Typen und Felder; was kein Feld hat, wird ein
  eigenes Feld oder Tag, siehe [contracts/import-mapping.md](./contracts/import-mapping.md)),
  Vorschau vorab, Schreiben in Schritten (jeder Eintrag, jeder Anhang für sich), bei Gesamtfehler
  oder Abbruch Rückgängigmachen, ein Bericht mit den Stellen zum Nacharbeiten (zum Beispiel
  Anhänge über 25 MiB). KeePass über `keepass`, Bitwarden und LastPass über `csv` und
  `serde_json`. Die Eignung von `keepass` prüft Aufgabe T003; zweite Wahl ist der Weg von
  haex-vault 2.x.
- **Oberfläche** (R10, R11, R13, R17): App `system.passwords`, Mehrfachinstanz, Orte im Tab nur
  mit Kennungen, Generator und Suche als reines TS unter `src/lib/passwords/` (Node-Tests),
  vorhandene Bausteine (`SettingsGroup`/`Row`, `WmRouterView`, `onVaultTablesChanged`).
- **Neue Abhängigkeiten**: Rust `keepass`, `csv`, `sha1`, `p256`, `ed25519-dalek`, `pkcs1`, `pkcs8` und `spki`
  (öffentlicher Schlüssel beim Passkey-Import), `tauri-plugin-clipboard-manager` und
  `unicode-normalization` (steht schon transitiv im `Cargo.lock`); Frontend keine. ADR-0007 hält „Geheimnisse unverschlüsselt in der Vault, Schutz durch
  Freigaben“ fest (R19).

## Technical Context

**Language/Version**: Rust (Tauri 2.12, Cargo-Features `llm-cpu` Standard, `llm-cuda`/`llm-metal`
optional); TypeScript 6 (strict), Vue 3.5, Nuxt 4.5.2 (SPA), Node 22.19 für die Prüfskripte

**Primary Dependencies**: vorhanden — `haex-crdt` (gepinnt auf `aeb26eb`), `rusqlite` über haex-crdt,
`hmac` 0.13, `sha2` 0.11, `getrandom` 0.4, `zeroize`, `base64`, `uuid` (v4, v5), `serde_json`,
`thiserror`, `ts-rs`, `tauri-plugin-dialog`; Frontend: Pinia, `@nuxtjs/i18n`, `reka-ui`/haex-ui-Layer.
**Neu (Rust)**: `keepass` (KDBX, Version und Features prüft T003), `csv` (RFC 4180; in
`Cargo.lock` nur über `llm-cpu`, daher direkte Abhängigkeit), `sha1` (RustCrypto), `p256`
(Features `pkcs8`, `pem`), `ed25519-dalek` (Feature `pkcs8`), `pkcs1`, `pkcs8` und `spki` (leiten den
öffentlichen Passkey-Schlüssel für ES256, EdDSA und RS256 ab, R12),
`tauri-plugin-clipboard-manager` (nur Rust-API), `unicode-normalization` (NFC für Tag-Kennungen,
R2; schon transitiv im Lock). Base32 wird selbst geschrieben (R8).

**Storage**: Migration `0022_passwords`: zwölf CRDT-Tabellen `haex_passwords_*`, 14 Indizes,
kein `_no_sync`. `HOLZI_TRIGGER_VERSION` 13 → 14. Eine Vault-Einstellung
`passwords.clipboard_clear_seconds`. Details in [data-model.md](./data-model.md).

**Testing**: Rust — Einheitstests in `*_tests.rs` (`access`, `totp`, `ids`, `tags`, `trash`,
`snapshots`, `binaries`, `import/*`), Integration in `src-tauri/tests/`
(`passwords_roundtrip.rs`, `passwords_sync.rs`, `passwords_access.rs`, `passwords_import.rs`),
Migrationstests (`migrations_tests.rs`, `vault_upgrade.rs`); Frontend — neues
`pnpm check:passwords` (Vorbild `check-wm-actions.ts`), Regression `check:agent-actions`,
`check:wm-navigation`, `check:templates`, `typecheck`, `typecheck:scripts`, `lint`,
`format:check`; End-to-End `passwords-basic`, `passwords-sync-two-devices`,
`passwords-narrow-window`, `passwords-session-restore`; manuell nach
[quickstart.md](./quickstart.md)

**Target Platform**: Tauri-Desktop (Linux, macOS, Windows). Mobile ist vorbereitet, aber nicht
Ziel: Dateipfade aus Dialogen setzen ein Dateisystem voraus (R4, Bewusste Grenzen). Alle neuen
Crates müssen in beiden Konfigurationen bauen (Standard und `--no-default-features`).

**Project Type**: desktop-app (Nuxt-SPA-Frontend + Rust-Backend in einem Tauri-Projekt)

**Performance Goals**: grobe Zielgrenzen, keine Messaufgaben: Suche und Übersicht fühlen sich bei
5.000 Einträgen sofort an (SC-002; die Übersicht ist eine Abfrage ohne BLOB- und Notizspalten);
ein Anhang von 25 MiB und ein Import von 1.000 Einträgen dauern Sekunden, nicht Minuten

**Constraints**: Dateien ≤ 500 Zeilen (`identity/migrations.rs` steht bei 577 → SQL in
`migrations_passwords.rs`; `lib.rs` 276 bekommt nur `use`/Registrierungen); Testcode in eigenen
Dateien; `src/lib/passwords/*` bleibt reines TS mit relativen `.ts`-Importen (Node-Harness);
keine `unwrap`/`expect` auf Eingabedaten; kein lokalisierter Text im Backend; kein Geheimnis in
Fehler, Protokoll, Ereignis, `Debug`-Ausgabe, Pfad, Abfrage oder Fenstertitel; eine
Schreibtransaktion unter 100 MiB (Anhänge einzeln); Tabellen- und Spaltennamen von haex-vault 2.x;
Aufrufer nie als Command-Argument

**Scale/Scope**: 12 Tabellen, rund 37 Commands, eine Aktion, rund 20 Rust-Module (dazu `service/`
mit 10, `import/` mit 8 und `commands/` mit 11 Dateien), rund 35 Vue-Komponenten, 8 reine
TS-Module, 8 Prüfskripte, 4 Rust-Integrationstests, 4 End-to-End-Szenen; Vorlage umfasst rund 12.000 Zeilen Vue/TS, von denen Logik und Struktur,
nicht Code, übernommen werden

## Constitution Check

_GATE: Muss vor Phase 0 bestehen. Nach Phase 1 erneut geprüft — Ergebnis unten._

Geprüft gegen `.specify/memory/constitution.md` (v1.4.0) und die spaex-Constitution
`.spaex/constitution.md`.

| Prinzip / Vorgabe                                                          | Status | Begründung                                                                                                                                                                                                                                                              |
| -------------------------------------------------------------------------- | ------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| I Keine Geheimnisse in Git                                                 | ✅     | Fixtures und Tests nutzen erfundene Werte (`SECRET-MARKER-…`, RFC-Testschlüssel); keine echten Konten in Beispieldateien; Prüfskript sucht Geheimnis-Muster in den Fixtures                                                                                             |
| II Keine lokalen absoluten Pfade in versionierter Konfiguration            | ✅     | Quickstart nutzt Platzhalter und repo-relative Pfade; die Capability enthält keine Pfade                                                                                                                                                                                |
| III Projektidentität geräteunabhängig                                      | ✅     | Berührt nicht                                                                                                                                                                                                                                                           |
| IV Cross-Repo-Referenzen an unveränderliche SHAs gepinnt                   | ✅     | Referenzen auf haex-vault 2.x tragen den SHA `8dce379d94e18fcd42c3b73686a06f984ca3f574`; haex-crdt bleibt auf `aeb26eb`                                                                                                                                                 |
| V Externe Quellen nur per Opt-in                                           | ✅     | Keine neue Harness-Quelle; neue Crates sind Abhängigkeiten, keine Harness-Inhalte                                                                                                                                                                                       |
| VI Selbstverändernde Anweisungen review-pflichtig                          | ✅     | Keine Änderung an Constitution, Skills oder Berechtigungen der Werkzeuge; die Freigaben von Agenten bleiben bei Spec 021; der eingebaute Agent bekommt keine neue Berechtigung                                                                                          |
| VII Relay-Ausfall blockiert lokale Arbeit nicht                            | ✅     | Der Passwortmanager arbeitet rein lokal; Sync ist Spec 024 und nicht Voraussetzung                                                                                                                                                                                      |
| VIII Keine Verheimlichung in Agent-Ausgaben                                | ✅     | Die Aktion erscheint im Chatverlauf wie jede; ihr Ergebnis nennt, was sie liefert; der Agent sieht keine Geheimnisse und der Nutzer sieht diese Einschränkung (Kopfdaten)                                                                                               |
| Workflow: speckit-Stufen, PR auf `main`, Conventional Commits, kein Squash | ✅     | specify → plan → tasks → implement; Topic-Branch im Worktree `.worktrees/034-password-manager`                                                                                                                                                                          |
| ADR bei prinzipienrelevanter Entscheidung                                  | ✅     | ADR-0007 „Geheimnisse unverschlüsselt in der Vault, Schutz durch Freigaben“ (Aufgabe in tasks); 0005 bleibt für Spec 021 reserviert                                                                                                                                     |
| Test-Code in separaten Dateien                                             | ✅     | `*_tests.rs` per `#[path]`, `src-tauri/tests/`, `scripts/check-passwords-*.ts`                                                                                                                                                                                          |
| Worktree je Änderung                                                       | ✅     | `.worktrees/034-password-manager`                                                                                                                                                                                                                                       |
| 500-LoC-Grenze                                                             | ⚠️     | `identity/migrations.rs` steht schon bei 577 (nicht von dieser Spec); neues SQL liegt deshalb in `migrations_passwords.rs`, `migrations.rs` bekommt nur eine Zeile; neue Dateien bleiben unter 500 (große Teile sind aufgeteilt: Commands je Bereich, Import je Format) |
| Graphify vor neuen benannten Artefakten                                    | ✅     | Aufgabe T002: Abfragen für Rahmen/Seitenleiste, `Query`/`VaultDb`, Zwischenablage, Dialoge, Suche vor dem ersten neuen Namen                                                                                                                                            |
| `ponytail:`-Kommentar bei bewusster Vereinfachung                          | ✅     | Geplant an: Zusammenführen doppelter Tags nur beim Öffnen (R2), Aufräumen mit fester Karenzzeit (R4), kein Gesamtbudget für Anhänge (R4), Millisekunden-Token der Konfliktprüfung (R15), Zwischenablage-Löschen bei Absturz (R9), Pfad-Übergabe nur Desktop (R4)        |
| Nicht-triviale Logik hinterlässt einen ausführbaren Check                  | ✅     | `access_tests`, `totp_tests`, `trash_tests`, `passwords_sync`, `check:passwords`, End-to-End                                                                                                                                                                            |
| Keine Selbstreferenzen von Agenten in Artefakten/Commits                   | ✅     | Commits und PR ohne Agent-Zusätze (holzi-Regel)                                                                                                                                                                                                                         |
| Phasen-Disziplin                                                           | ✅     | Baut auf 015, 020, 024 (gemerged und im Einsatz, offen nur das manuelle Quickstart T081) und 030; Spec 029 setzt diese Spec voraus, nicht umgekehrt; 017–019 und 021 folgen danach                                                                                      |
| Keine Geheimnisse in Logs/Fehlern (Spec FR-040)                            | ✅     | `Debug` geschwärzt, Fehlerfelder ohne Werte, Prüfung per Markierungswert in den Integrationstests                                                                                                                                                                       |

**Ergebnis vor Phase 0**: kein unbegründeter Verstoß; ein ⚠️ dokumentiert.

**Ergebnis nach Phase 1**: unverändert. Das Design fügt zehn direkte Abhängigkeiten hinzu (Rust; einige schon transitiv im Lock) und eine
Migration; beides begründet in research.md (R1, R8, R9, R12). Die Abweichungen vom Datenmodell von
haex-vault 2.x sind in [data-model.md](./data-model.md) als A1–A6 gelistet. Die Spec wurde an sieben
Stellen an die Planung angeglichen (R20).

## Project Structure

### Documentation (this feature)

```text
specs/034-password-manager/
├── plan.md                        # dieser Plan
├── research.md                    # Phase 0: Entscheidungen R1–R20
├── data-model.md                  # Phase 1
├── quickstart.md                  # Phase 1
├── contracts/
│   ├── tauri-commands.md          # Commands, Fehlerarten
│   └── access.md                  # Aufrufer, Freigaben, Regeln Z1–Z13
├── checklists/requirements.md
└── tasks.md                       # Phase 2 (/speckit-tasks)
```

### Source Code (Repository-Wurzel)

```text
src-tauri/src/
├── identity/
│   ├── migrations.rs              # HOLZI_TRIGGER_VERSION 14, Eintrag 0022        [ändern, klein]
│   └── migrations_passwords.rs    # SQL der zwölf Tabellen und Indizes             [neu]
├── passwords/
│   ├── mod.rs                     # Modulliste, Konstanten (Limit, Karenzzeit)      [neu]
│   ├── ids.rs                     # Namensräume, abgeleitete Kennungen, fold        [neu]
│   ├── model.rs                   # Zeilen-/Antworttypen (ts-rs), Debug geschwärzt  [neu]
│   ├── access.rs                  # Caller, Grant, Scope, Regeln Z1–Z13 (rein)      [neu]
│   ├── service/                   # PasswordsService: der einzige Eingang, prüft den Aufrufer
│   │   ├── mod.rs, items.rs, organize.rs, trash.rs, history.rs, attachments.rs,
│   │   │   passkeys.rs, presets.rs, import.rs, usage.rs                             [neu]
│   ├── items.rs                   # Übersicht, Detail, Anlegen, Teil-Update         [neu]
│   ├── reveal.rs                  # reveal, history_reveal, copy_field-Kern         [neu]
│   ├── groups.rs                  # Baum, Verschieben, Zyklus                       [neu]
│   ├── trash.rs                   # Papierkorb, Wiederherstellen, endgültig löschen [neu]
│   ├── tags.rs                    # Tags, Zuordnung, reconcile_tags                 [neu]
│   ├── binaries.rs                # Anhänge, Hash, Limit, prune_binaries            [neu]
│   ├── snapshots.rs               # Verlaufsstände, Änderungsliste, Wiederherstellen[neu]
│   ├── passkeys.rs                # Liste, Spitzname, Löschen, Schlüsselableitung   [neu]
│   ├── usage.rs                   # EntryUsage-Trait: holzi-Funktionen melden genutzte Einträge [neu]
│   ├── presets.rs                 # Generator-Voreinstellungen                      [neu]
│   ├── totp.rs                    # RFC 6238, Base32, otpauth-Eingabe               [neu]
│   ├── clipboard.rs               # Schreiben + abbrechbares Löschen                [neu]
│   ├── import/
│   │   ├── mod.rs                 # ImportModel, Vorschau, Doppelte                 [neu]
│   │   ├── keepass.rs, bitwarden.rs, lastpass.rs, csv.rs, icons.rs, apply.rs, report.rs [neu]
│   ├── commands/
│   │   ├── mod.rs, read.rs, items.rs, organize.rs, trash.rs, attachments.rs,
│   │   │   history.rs, passkeys.rs, presets.rs, import.rs, agent.rs   (rufen nur den Dienst) [neu]
│   └── *_tests.rs                 # je Modul (access, totp, ids, tags, trash, …)    [neu]
├── error.rs                       # HolziError + Passwords*-Varianten              [ändern]
├── lib.rs                         # mod passwords, Plugin, generate_handler        [ändern]
├── instances/open.rs              # prune_binaries + reconcile_tags beim Öffnen    [ändern, klein]
└── chat/eval/tools.json           # Schnappschuss neu erzeugt (export:eval-tools)  [ändern]

src-tauri/
├── Cargo.toml                     # keepass, csv, sha1, p256, unicode-normalization, clipboard-Plugin [ändern]
├── capabilities/default.json      # dialog:allow-save                              [ändern]
└── tests/
    ├── passwords_roundtrip.rs, passwords_sync.rs, passwords_access.rs, passwords_import.rs  [neu]
    └── fixtures/passwords/        # erfundene KeePass-, Bitwarden-, LastPass-Dateien       [neu]

src/
├── lib/passwords/
│   ├── registry.ts                # Orte, Routenmuster, Titelschlüssel              [neu]
│   ├── generator.ts               # Passwortgenerator (reines TS)                   [neu]
│   ├── search.ts                  # fold, Filter über Kopfdaten                     [neu]
│   ├── tree.ts                    # Ordnerbaum, Sortierung, Papierkorb-Erkennung    [neu]
│   ├── format.ts                  # Dateigröße, Bildtyp, Dateiname bereinigen       [neu]
│   └── selection.ts, icons.ts, usage.ts   # Mehrfachauswahl, Symbolliste, Warnentscheidung [neu]
├── lib/actions/
│   ├── passwordsActions.ts        # passwords.items.search                          [neu]
│   ├── scopes.ts, catalog.ts      # Bereich passwords.read, Anhängen an ALL_ACTIONS [ändern]
├── lib/wm/apps.ts                 # system.passwords                                [ändern]
├── components/wm/appRoutes.ts     # Routen der App                                  [ändern]
├── components/apps/PasswordsApp.vue                                                 [neu]
├── components/passwords/          # Seitenleiste, Liste, Editor (Details, Felder, TOTP,
│                                  # Anhänge, Verlauf, Passkeys), Generator, Import-Assistent,
│                                  # Papierkorb, Dialoge, Werkzeugleiste             [neu]
├── composables/usePasswords.ts    # invoke-Hüllen (*Async)                          [neu]
├── stores/passwords.ts            # Übersicht, Auswahl, onVaultTablesChanged        [neu]
├── stores/passwordsActionHandlers.ts, plugins/actions.client.ts                     [neu / ändern]
├── composables/useErrorString.ts  # neue Fehlerarten                                [ändern]
├── i18n/locales/{de,en}.json      # passwords.*, wm.apps.passwords, actions.*, errors.* [ändern]
└── types/bindings/*.ts            # ts-rs-Export (pnpm generate:ts-types)           [generiert]

scripts/
├── check-passwords-generator.ts, -search.ts, -routes.ts, -actions.ts,
│   -tree.ts, -format.ts, -selection.ts, -usage.ts                                   [neu]
└── e2e/scenarios/passwords-basic.test.ts, passwords-sync-two-devices.test.ts,
    passwords-narrow-window.test.ts, passwords-session-restore.test.ts, lib/passwords.ts [neu]

docs/adr/0007-secrets-in-vault-db-protected-by-grants.md                             [neu]
package.json, .github/workflows/ci.yml          # check:passwords                    [ändern]
```

**Structure Decision**: Ein Rust-Modul `passwords/` (je Verantwortung eine Datei, Commands je
Bereich), damit keine Datei die 500-Zeilen-Grenze erreicht und die Zugriffsprüfung als reines
Modul ohne Datenbank testbar ist. Das Frontend trennt reines TS (testbar mit Node) von Vue und
legt die Seitenleisten-Entscheidung in Aufgabe T002 (Graph-Abfrage), nicht jetzt.

## Complexity Tracking

| Verstoß / Aufwand                                                            | Warum nötig                                                                                                                                     | Einfachere Alternative verworfen, weil                                                                                                                                                                          |
| ---------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Abweichung vom Datenmodell von haex-vault 2.x (A1–A6, vor allem kein UNIQUE) | UNIQUE-Konflikte halten den Sync an (R2); der Papierkorb soll den Ort merken (R3); BLOB statt Base64 und `orphaned_at` schützen den Sync (R4)   | 1:1-Übernahme brächte einen Sync, der wegen eines Tagnamens stehen bleibt, den Verlust des Ortes beim Wiederherstellen oder das Löschen einer verspäteten Referenz                                              |
| Zehn Rust-Abhängigkeiten (mehrere schon transitiv im Lock)                   | KDBX, RFC-4180-CSV, SHA-1 für TOTP, P-256, EdDSA und RSA-Schlüsselteile für den öffentlichen Passkey-Schlüssel, Zwischenablage mit Löschen, NFC | TOTP und Base32 selbst zu schreiben ist vertretbar (R8) und geschieht; KDBX/Argon2, CSV, Elliptische Kurven, ASN.1-Schlüsselformate und Unicode-Normalisierung selbst zu schreiben wäre riskanter als ein Crate |
| Teil-Update und `reveal` statt Klartext-Detail                               | Geheimnisse bleiben im Backend (R7, FR-005, FR-040)                                                                                             | Alle Felder im Klartext zu laden ist einfacher, legt aber alle Geheimnisse in den Webview                                                                                                                       |
| Aufrufer-/Freigabe-Modul und Dienst ohne Verwalter für Freigaben             | FR-024 bis FR-030 verlangen die Prüfung jetzt; Spec 029 braucht den Zugriff, 017–019/021 den Rest                                               | Prüfung später nachrüsten hieße, die Commands und den Dienst ein zweites Mal anzufassen; die Prüfung ist rein und klein                                                                                         |
| `identity/migrations.rs` bleibt über 500 Zeilen                              | Bestehende Überschreitung (577), nicht von dieser Spec                                                                                          | Aufspaltung ist eine eigene Änderung; hier genügt eine neue Datei für das neue SQL                                                                                                                              |

## Bewusste Grenzen (aus research.md)

- Doppelte Tags nach einem Umbenennungs-Wettlauf werden nur beim Öffnen zusammengeführt (R2);
  die Dauerlösung wäre eine eigene `ApplyPolicy` im Sync (Spec 024).
- Es gibt kein Gesamtbudget für Anhänge; ein erster Vollabruf hält alle Binärzellen im Speicher
  (R4). Aufrüstweg: gestaffeltes Scannen in haex-crdt.
- Endgültig gelöschte Anhänge belegen bis zu sieben Tage Platz (Karenzzeit, R4).
- Dateipfade aus Dialogen setzen ein Desktop-Dateisystem voraus; Mobil braucht später `plugin-fs`
  oder einen Datenstrom (R4).
- Die Zwischenablage wird bei einem Absturz nicht geleert (R9).
- Der Konflikt-Token ist auf Millisekunden genau (R15).
- Keine PDF-Vorschau und kein Export (Spec, Nicht im Umfang; R18). Der Import lässt dagegen nichts
  weg; was nicht ankommt (Anhänge über 25 MiB, ein nicht ableitbarer öffentlicher Passkey-Schlüssel),
  steht im Bericht (R12).
- Das Verzeichnis der angelegten Zeilen für das Rückgängigmachen liegt nur im Speicher; ein
  Absturz mitten im Import hinterlässt Teile (R12).
- Der eingebaute Agent kennt nur Titel, Tags und Ordnernamen; mehr kommt mit 017–019 und 021
  über deren Eingänge (R6, R14).
- Ob `keepass` den Bedarf deckt, zeigt T003; sonst gilt die zweite Wahl (R12).
