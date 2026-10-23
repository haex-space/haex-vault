# Implementation Plan: Erweiterungs-Host für haextensions

**Branch**: `017-extension-host` | **Date**: 2026-10-02 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/017-extension-host/spec.md`

## Summary

holzi bekommt einen Host für haextensions: signierte Web-Bundles laufen in abgeschotteten iframes als Apps im
Window Manager, sprechen über das Protokoll des vault-sdk v3.7.0 mit einer einzigen Prüfstelle in Rust und
dürfen SQL auf ihre eigenen Tabellen und mit Berechtigung auf die anderer Erweiterungen absetzen, nie auf
Kerntabellen. Dazu kommen alle Host-Funktionen von haex-vault 2.x (Legacy) außer der Space-Zuordnung, auf Desktop, Android
und iOS. Vorlage ist haex-vault 2.x: Verhalten und die Prüfungen in Rust werden übernommen, die Lücken, die die
Analyse dort gefunden hat, schließt holzi beim Portieren (Spec, Clarifications 2026-10-02).

Technischer Ansatz (Begründungen und verworfene Alternativen in [research.md](./research.md)):

- **Sechs Lieferungen** (R1): L0 Vorarbeit in haex-crdt, vault-sdk und wry (Android, Windows), L1 der nutzbare Kern auf einem Gerät
  (US1–US3), L2 mehrere Geräte (US4), L3 Meldungen, Speicher, fremde Tabellen, Lebenszyklus, Entwicklermodus,
  L4 Netz, Benachrichtigungen, Dateien, L5 Passwörter, entfernter Speicher, Mail, Shell. Jede Lieferung bringt
  ihre Fälle der Umgehungssammlung (SC-002) mit.
- **Bundle-Format v2** (R2, R3): Zip mit Manifest als kanonischem JSON (RFC 8785) und einer eigenen
  `signature.json` mit Pfad, Größe und SHA-256 jeder Datei; Ed25519 über eine Nachricht mit Domänenpräfix.
  Strenge Regeln für Pfade, doppelte Einträge und Zip-Bomben. Das Werkzeug `haex` im vault-sdk erzeugt das
  Format; holzi lehnt das alte ab.
- **Bundles als Vault-Daten** (R4, R5): je Datei ein inhaltsadressierter BLOB, ausgeliefert direkt aus der
  verschlüsselten Datenbank mit Hash-Prüfung bei jedem Start und jeder Anfrage; abgeleitete Kennungen statt
  UNIQUE-Constraints.
- **SQL in zwei Schichten** (R6–R8): die **aus haex-vault 2.x portierte Rust-Prüfung** (Parser, Planer,
  Tabellen-Extraktion, Validator, Berechtigungsabgleich samt Tests), beim Portieren um die Lücken korrigiert,
  liefert die nötigen Berechtigungen vorab, damit holzi fragen kann; dazu der **SQLite-Authorizer** als zweite,
  unabhängige Durchsetzung mit Erlaubtliste (eigene und freigegebene
  Tabellen, erlaubte Funktionen, nur Trigger von haex-crdt). Dafür bekommt haex-crdt `write_guarded`/
  `read_guarded`. Migrationen laufen in einem eigenen Lauf in holzi mit Schema-Modus und Triggern im selben
  Commit; Journal geräteeigen als `_no_sync`.
- **Mehrere Geräte** (R10, R11): Gruppen für noch fehlende Tabellen werden ganz geparkt und nach den
  Migrationen angewendet, ohne anderen Sync aufzuhalten; wirksame Fassung deterministisch (höchste Semver);
  Entfernen über eine Grabstein-Marke, die jedes Gerät lokal ausführt.
- **Einbettung** (R12–R14, R17): ein Protokoll `holzi-ext` mit Pfad-Routing, Abschottung über den
  undurchsichtigen Ursprung von `sandbox="allow-scripts allow-pointer-lock"` (mit `allow="fullscreen"`), CSP als Antwort-Header mit Hashes der
  Inline-Skripte, `frame-src` in holzis CSP gegen Selbstnavigation; Start-Token je Rahmen gegen
  Navigation zu einer anderen Erweiterung; SDK prüft `event.source`. Das Frontend reicht Anfragen nur an einen
  Command `extension_bridge_call`, Rust prüft. Ein eingefügter Rahmen-Shim bildet Hash-Navigation, Titel,
  `beforeunload`, `window.close()` und Tastenkürzel auf die Tab-Schnittstelle ab.
- **Berechtigungen** (R15): reines Rust-Modul, Geltungsbereich vault-weit oder Gerät nach ADR-0001 (Shell immer
  Gerät, alles andere vault-weit, ohne Wahl; Clarification 2026-10-06), Anfrage SDK-kompatibel über Fehler 1004 und
  Wiederholung, Warteschlange und Kategorie „Erweiterungen“ in den Einstellungen.
- **Übrige Host-Funktionen** (R18–R21): Netz mit selbst geprüften Weiterleitungen, Dateien mit aufgelöstem
  Ziel, Sperrliste und Dialog-Auswahl als Berechtigung, Benachrichtigungen mit Klick, Passwörter als Adapter
  auf 034, entfernter Speicher auf 029, Mail mit Host und Port, Shell je Programm.
- **Neue Abhängigkeiten**: Rust `haex-bundle` (Crate im vault-sdk, per Git-Revision gepinnt; die einzige
  Umsetzung des Bundle-Formats, R3), `ed25519-dalek` (gemeinsam mit 034, schon im Lock), in L4 `notify` + `notify-debouncer-full`, `tauri-plugin-notification`, ggf. `notify-rust`,
  in L5 `async-imap`, `lettre`, `mail-parser`; Frontend keine. haex-crdt- und vault-sdk-Pins werden
  angehoben. ADR-0008 hält Signaturformat und Authorizer fest (R24).

## Technical Context

**Language/Version**: Rust (Tauri 2.12.1, wry 0.57; Cargo-Features `llm-cpu` Standard, Build auch mit
`--no-default-features`); TypeScript 6 (strict), Vue 3.5, Nuxt 4.5.2 (SPA), Node 22.19 für die Prüfskripte

**Primary Dependencies**: vorhanden — `haex-crdt` (Pin `aeb26eb` → neue Revision aus L0), `rusqlite` 0.40
über haex-crdt, sqlparser 0.62 über `haex_crdt::sqlparser`, `sha2` 0.11, `uuid` (v5), `serde_json`
(`preserve_order`), `reqwest` 0.13 (rustls), `portable-pty` 0.9, `tauri-plugin-dialog`,
`tauri-plugin-opener`, `thiserror`, `ts-rs`; Frontend: Pinia, `@nuxtjs/i18n`, haex-ui-Layer.
**Neu (Rust)**: `haex-bundle` (vault-sdk, R3), `ed25519-dalek` 3 (mit 034 geteilt); L4: `notify`,
`notify-debouncer-full`, `tauri-plugin-notification`, `notify-rust` (nur Linux, nach Machbarkeitsprüfung);
L5: `async-imap` (tokio, `tokio-rustls`), `lettre` (rustls/ring), `mail-parser`. TLS überall rustls mit ring.
**Andere Repositories (L0)**: `haexmas/haex-crdt` (SqlGuard, Trigger nach DDL, Schema-Modus, metadatentreuer
Umbau, Spaltennamen, lokaler Modus), `haex-space/vault-sdk` (Format v2 in `haex`, `haex verify`, Testvektoren,
`event.source`-Prüfung), `tauri-apps/wry` (Android und Windows: Init-Skripte nur in den Hauptrahmen, R12, R25; bis zur
Veröffentlichung Fork über `[patch.crates-io]`); alle mit voller Revision gepinnt.

**Storage**: Migration `0023_extensions` (nach `0022_passwords` aus 034) in
`identity/migrations_extensions.rs`: neun synchronisierte Tabellen (`extensions`, `extension_bundles`,
`extension_bundle_files`, `extension_blobs`, `extension_migrations`, `extension_permissions`,
`extension_limits`, `extension_device_status`, `extension_kv`) und sechs `_no_sync`-Tabellen (Journal,
ausgeführtes Aufräumen, Protokolle, geparkte Sync-Gruppen, zwei für den Entwicklermodus). `HOLZI_TRIGGER_VERSION` 14 → 15. Tabellen der
Erweiterungen entstehen zur Laufzeit über deren Migrationen. Details in [data-model.md](./data-model.md).

**Testing**: Rust — Einheitstests in `*_tests.rs` je Modul; Integration in `src-tauri/tests/`
(`extension_bundle_format`, `extension_sql_bypass`, `extension_sql_exec`, `extension_migrations`,
`extension_bridge_contract`, `sync_extension_parking`, `extension_lifecycle_sync`, `extension_web`,
`extension_fs`); Frontend — neues `pnpm check:extensions`, Regression `check:wm-navigation`, `check:wm-state`,
`check:settings`, `check:templates`, `typecheck`, `lint`, `format:check`; End-to-End
`extension-install-open`, `extension-permission-prompt`, `extension-isolation`, `extension-two-devices`,
`extension-dialog`;
manuell nach [quickstart.md](./quickstart.md)

**Target Platform**: Linux, macOS, Windows, Android, iOS (FR-066). Die mobilen Builds von holzi (Projekt,
Signierung, CI, Oberfläche für kleine Bildschirme) sind eine eigene Spec; 017 baut den Host mobil-tauglich und
wird auf Android und iOS abgenommen, sobald diese Ziele bestehen (R25).

**Project Type**: desktop-app (Nuxt-SPA-Frontend + Rust-Backend in einem Tauri-Projekt)

**Performance Goals**: grobe Zielgrenzen, keine Messaufgaben: Öffnen einer installierten Erweiterung fühlt sich
sofort an (Hash-Prüfung eines Bundles von 20 MiB grob 50 ms, vermutet); Meldungen erreichen offene Rahmen
innerhalb einer Sekunde (SC-005); eine Abfrage endet spätestens nach 5 s und sperrt die Verbindung nicht
länger (SC-006)

**Constraints**: Dateien ≤ 500 Zeilen (`identity/migrations.rs` steht bei 585 → SQL in eigener Datei;
`lib.rs` 351 bekommt nur `mod`/Registrierungen); Testcode in eigenen Dateien; keine `unwrap`/`expect` auf
Eingaben von Erweiterungen; Identität einer Erweiterung nie aus ihren Angaben; synchronisierte Zeilen von Kern-
und Erweiterungstabellen nie in einer Schreibgruppe; jede Schreibgruppe < 100 MiB (BLOBs einzeln); kein Zugriff über `with_connection`
(Clippy-Sperre); kein lokalisierter Text im Backend; Fehlertexte ohne Namen außerhalb der Berechtigungen

**Scale/Scope**: 15 Tabellen, rund 25 Tauri-Commands für die Oberfläche plus der eine Brücken-Command mit
rund 75 Methoden der Erlaubtliste, rund 35 Rust-Module in `src-tauri/src/extensions/`, eine Änderung im
Sync-Empfang, rund 15 Vue-Komponenten, 4 reine TS-Module, 16 Rust-Integrationstests, 4 End-to-End-Szenen, je
ein PR in haex-crdt und vault-sdk

## Constitution Check

_GATE: Muss vor Phase 0 bestehen. Nach Phase 1 erneut geprüft — Ergebnis unten._

Geprüft gegen `.specify/memory/constitution.md` (v1.4.0) und die spaex-Constitution `.spaex/constitution.md`.

| Prinzip / Vorgabe                                                          | Status | Begründung                                                                                                                                                                                                                                      |
| -------------------------------------------------------------------------- | ------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| I Keine Geheimnisse in Git                                                 | ✅     | Testschlüssel für Fixture-Bundles werden nur für Tests erzeugt und sind als solche gekennzeichnet; kein Herausgeberschlüssel einer echten Erweiterung im Repository; private Schlüssel nie im Bundle (bundle-format.md)                         |
| II Keine lokalen absoluten Pfade in versionierter Konfiguration            | ✅     | CSP und Capabilities nennen nur Schemata und `localhost`; Pfade der Entwicklerin bleiben in `_no_sync`-Zeilen der Vault                                                                                                                         |
| III Projektidentität geräteunabhängig                                      | ✅     | Berührt nicht; Geräteeigenes folgt ADR-0001                                                                                                                                                                                                     |
| IV Cross-Repo-Referenzen an unveränderliche SHAs gepinnt                   | ✅     | haex-vault 2.x `8dce379…`, vault-sdk `502593e…`, haextension `db48f9a…` in Spec und Research; die neuen Revisionen von haex-crdt, vault-sdk und dem wry-Fork aus L0 werden mit vollem SHA gepinnt, Testvektoren tragen Repository, SHA und Pfad |
| V Externe Quellen nur per Opt-in                                           | ✅     | Keine neue Harness-Quelle; Erweiterungen sind Nutzerdaten, keine Harness-Inhalte                                                                                                                                                                |
| VI Selbstverändernde Anweisungen review-pflichtig                          | ✅     | Keine Änderung an Constitution, Skills oder Werkzeugrechten; Erweiterungen und Agenten können keine Berechtigungen ändern (FR-021)                                                                                                              |
| VII Relay-Ausfall blockiert lokale Arbeit nicht                            | ✅     | Erweiterungen laufen lokal; L2 parkt Gruppen, statt den Sync oder die lokale Arbeit anzuhalten                                                                                                                                                  |
| VIII Keine Verheimlichung in Agent-Ausgaben                                | ✅     | Kein Weg aus Erweiterungen zum Agenten (FR-009, Vertragstest); Anfragen zeigen offen, ob eine Berechtigung erklärt war                                                                                                                          |
| Workflow: speckit-Stufen, PR auf `main`, Conventional Commits, kein Squash | ✅     | specify → clarify → plan → tasks → implement; Worktree `.worktrees/017-extension-host`; je Lieferung ein PR                                                                                                                                     |
| ADR bei prinzipienrelevanter Entscheidung                                  | ✅     | Kein Prinzip betroffen; ADR-0008 trotzdem für Signaturformat und Authorizer (R24), weil beides über holzi hinaus (vault-sdk) wirkt                                                                                                              |
| Test-Code in separaten Dateien                                             | ✅     | `*_tests.rs` per `#[path]`, `src-tauri/tests/`, `scripts/check-extensions-*.ts`                                                                                                                                                                 |
| Worktree je Änderung                                                       | ✅     | holzi: `.worktrees/017-extension-host`; haex-crdt und vault-sdk: eigene Worktrees in ihren Repositories                                                                                                                                         |
| 500-LoC-Grenze                                                             | ⚠️     | `identity/migrations.rs` steht schon bei 585 (nicht von dieser Spec); das neue SQL liegt in `migrations_extensions.rs`, `migrations.rs` bekommt eine Zeile. Neue Module sind nach Aufgaben geteilt (siehe Struktur)                             |
| Graphify vor neuen benannten Artefakten                                    | ✅     | Aufgabe in tasks: Abfragen für Tab-Schnittstelle, `VaultDb`, `vault_events`, Sync-Empfang, Einstellungs-Registry, Dialog-Muster vor dem ersten neuen Namen                                                                                      |
| `ponytail:`-Kommentar bei bewusster Vereinfachung                          | ✅     | Geplant an: Neuladen des Rahmens beim Verschieben (R17), Ringpuffer der Protokolle (R20), feste Karenzzeit der BLOBs (R4), Klicks auf macOS/Windows (R20), Entwicklermodus ohne Netzsperre (R16)                                                |
| Nicht-triviale Logik hinterlässt einen ausführbaren Check                  | ✅     | Umgehungssammlung, Format-Testvektoren, Vertragstest, Sync-Tests, `check:extensions`, End-to-End                                                                                                                                                |
| Keine Selbstreferenzen von Agenten in Artefakten/Commits                   | ✅     | Commits und PRs ohne Agent-Zusätze (holzi-Regel), auch in haex-crdt und vault-sdk                                                                                                                                                               |
| Phasen-Disziplin                                                           | ✅     | Baut auf 015, 020, 022, 023 (gemerged, im Einsatz) und 024 (US1–US7 im Einsatz; L2 setzt das voraus); 034 ist gemerged (PR #222), L5 wartet nur für den entfernten Speicher auf 029; 018, 019, 021 und 028 folgen danach                        |

**Ergebnis vor Phase 0**: kein unbegründeter Verstoß; ein ⚠️ dokumentiert.

**Ergebnis nach Phase 1**: unverändert. Das Design fügt zwei direkte Abhängigkeiten in L1 hinzu (beide schon
im Lock), weitere in L4/L5 (begründet in R19–R21), eine Migration, eine Änderung im Sync-Empfang (R10) und je
einen PR in haex-crdt und vault-sdk (R6, R8, R2, R13). Die Spec wurde an vier Stellen an die Planung
angeglichen (R22).

## Project Structure

### Documentation (this feature)

```text
specs/017-extension-host/
├── plan.md                  # dieser Plan
├── research.md              # Phase 0: Entscheidungen R1–R25
├── data-model.md            # Phase 1
├── quickstart.md            # Phase 1
├── contracts/
│   ├── bundle-format.md     # Format v2, Prüfung, Werkzeug haex
│   ├── bridge.md            # Rahmen, Kanal, Shim, Methoden, Meldungen, Fehlercodes
│   ├── sql-policy.md        # Laufzeit, Authorizer, Migrationen, Umgehungssammlung
│   ├── permissions.md       # Arten, Auswertung, Installation, Anfrage, Einstellungen
│   └── tauri-commands.md    # Commands und Ereignisse für holzis Oberfläche
├── checklists/requirements.md
└── tasks.md                 # Phase 2 (/speckit-tasks)
```

### Source Code (Repository-Wurzel)

```text
src-tauri/src/
├── identity/
│   ├── migrations.rs                 # HOLZI_TRIGGER_VERSION 15, Eintrag 0023         [ändern, klein]
│   └── migrations_extensions.rs      # SQL der 15 Tabellen                                [neu]
├── extensions/
│   ├── mod.rs                        # Modulliste, Konstanten (Grenzen)                   [neu, L1]
│   ├── ids.rs                        # Namensräume, abgeleitete Kennungen, TablePrefix    [neu, L1]
│   ├── error.rs                      # ExtensionErrorCode, Abbildung auf SDK-Fehler      [neu, L1]
│   ├── bundle/                       # mod.rs (Aufruf von haex-bundle, R3), manifest.rs, store.rs (BLOBs, R4) [neu, L1]
│   ├── registry/                     # install.rs, effective.rs (R11), lifecycle.rs, status.rs, purge.rs [neu, L1/L2/L3]
│   ├── protocol/                     # handler.rs (holzi-ext), csp.rs (Hashes), shim.rs (Rahmen-Shim), token.rs [neu, L1]
│   ├── bridge/                       # frames.rs (Rahmensitzungen), dispatch.rs (Erlaubtliste), events.rs [neu, L1]
│   ├── permissions/                  # model.rs, evaluate.rs, store.rs, prompts.rs, manifest_map.rs [neu, L1]
│   ├── sql/                          # policy.rs, ast_check.rs, authorizer.rs, exec.rs, values.rs, migrate.rs, migrate_rules.rs, changes.rs [neu, L1]
│   ├── context.rs, kv.rs, logs.rs    # Kontext, Schlüssel-Wert-Speicher, Protokolle       [neu, L1/L3]
│   ├── dev.rs                        # Entwicklermodus (R16)                              [neu, L3]
│   ├── web.rs, notifications.rs      # R18, R20                                           [neu, L4]
│   ├── fs/                           # resolve.rs, denylist.rs, ops.rs, dialogs.rs, watch.rs (R19) [neu, L4]
│   ├── passwords.rs, remote_storage.rs, mail/, shell.rs  # R21                            [neu, L5]
│   ├── commands/                     # install.rs, manage.rs, permissions.rs, frames.rs, dev.rs, logs.rs [neu]
│   └── *_tests.rs                    # je Modul                                           [neu]
├── sync/
│   ├── inbound.rs                    # Vorprüfung je Gruppe, Parken statt UnknownTable    [ändern, L2]
│   └── inbound_park.rs               # geparkte Gruppen, Anwenden nach Migrationen        [neu, L2]
├── vault_gate/db.rs                  # write_guarded / read_guarded                       [ändern, L1]
├── vault_events.rs                   # broadcast an den Host                              [ändern, L3]
├── error.rs                          # HolziError::Extension*                              [ändern]
└── lib.rs                            # mod extensions, Protokoll, Plugins, generate_handler [ändern, klein]

src-tauri/tauri.conf.json             # frame-src/img-src für holzi-ext in csp und devCsp  [ändern, L1]
src-tauri/tests/                      # die Integrationstests aus Technical Context        [neu]
src-tauri/tests/fixtures/extension_bundles/  # Testvektoren aus dem vault-sdk mit Herkunftskopf [neu, L0/L1]

src/
├── lib/extensions/                   # bridge.ts (Relais, Bytes), queue.ts (Anfragen), apps.ts (App-Liste), shim-protocol.ts [neu, L1]
├── lib/wm/apps.ts                    # allApps(): WM_APPS + Erweiterungen                 [ändern, L1]
├── stores/extensions.ts              # Liste, Status, Ereignisse                          [neu, L1]
├── stores/windowManager.ts           # App-Liste statt WM_APPS                            [ändern, L1]
├── composables/useExtensionFrame.ts  # Kanal, Puffer, Shim, Tab-Schnittstelle             [neu, L1]
├── components/wm/TabPanel.vue        # ExtensionFrame für extension.*                     [ändern, L1]
├── components/wm/{Launcher,NewTabMenu}.vue, appRoutes.ts                                   [ändern, L1]
├── stores/{wmLayoutHandlers,wmActionHandlers}.ts                                          [ändern, L1]
├── pages/workspace/[instance].vue    # Erweiterungen vor dem Wiederherstellen laden       [ändern, L1]
├── components/extensions/            # ExtensionFrame.vue, PermissionRequestDialog.vue, InstallDialog.vue, FrameError.vue, DevConsole.vue [neu]
├── components/settings/extensions/   # Liste, Details, Berechtigungen, Grenzen, Protokolle, Behaltene Daten, Entwicklermodus [neu, L1/L3]
├── lib/settings/registry.ts          # Kategorie „Erweiterungen“                          [ändern, L1]
└── i18n/                             # Texte de/en                                        [ändern]

scripts/check-extensions-*.ts         # pnpm check:extensions                              [neu]
scripts/e2e/scenarios/extension-*.test.ts  # vier Szenen                                   [neu]
docs/adr/0008-extension-bundle-signature-and-sql-authorizer.md                             [neu]
plans/README.md                       # Zeile für 017                                      [ändern]
```

**Structure Decision**: Alles Neue liegt in `src-tauri/src/extensions/` (Rust) und `src/lib/extensions/`,
`src/components/extensions/`, `src/components/settings/extensions/` (Frontend), damit die Grenze der
Prüfstelle in einem Modul liegt. Bestehende Module ändern sich nur an den Stellen, die die Recherche nennt
(Sync-Empfang, `VaultDb`, `vault_events`, Window Manager, Einstellungs-Registry, CSP).

## Complexity Tracking

| Abweichung                                               | Warum nötig                                                                                                                                                               | Einfachere Alternative verworfen, weil                                                                                              |
| -------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------- |
| Änderung in haex-crdt (anderes Repository)               | Der Authorizer muss um genau die Anweisung der Erweiterung liegen, und DDL zur Laufzeit braucht Trigger, Schema-Modus und metadatentreuen Umbau im selben Commit (R6, R8) | Nur AST-Prüfung ist die Lücke von haex-vault 2.x; `install_crdt` nachträglich ist nicht atomar und kaskadiert Löschungen beim Umbau |
| Änderung im vault-sdk (anderes Repository)               | Signaturformat v2 und `event.source`-Prüfung (R2, R13)                                                                                                                    | Altes Format ist manipulierbar; ohne die Prüfung kann ein Geschwisterrahmen einen falschen Kanal unterschieben                      |
| Fork von wry (Android, Windows) bis zur Veröffentlichung | Ohne Korrektur bekäme jeder Rahmen auf Android und Windows den Invoke-Key und damit holzis Commands (R25)                                                                 | Erweiterungen auf Android sperren (vom Betreiber abgelehnt); eigenes Webview je Erweiterung (auf Mobilgeräten nicht möglich)        |
| Eingriff in den Sync-Empfang                             | FR-037: Gerät ohne Tabellen einer Erweiterung darf den Sync nicht anhalten (R10)                                                                                          | Ohne Parken bricht der Empfang ab oder verliert Daten still                                                                         |
| Rahmen-Shim in ausgelieferten HTML-Dokumenten            | FR-012/FR-013 ohne Änderung am Code der Erweiterungen (R17)                                                                                                               | SDK v3.7.0 hat keine Meldungen für Navigation, Titel, Tastatur                                                                      |

## Bewusste Grenzen (aus research.md)

- Ein Rahmen lädt neu, wenn sein Tab in ein anderes Fenster wandert oder der Arbeitsbereich wechselt; der Ort
  bleibt (R17).
- Kein Schutz über CSP gegen WebRTC und DNS-Prefetch; keine Prozesstrennung zwischen Rahmen (R12).
- Klicks auf Benachrichtigungen auf macOS und Windows nur, soweit das System es zulässt; scheitert die
  Machbarkeitsprüfung, kommt FR-052 zur Spec zurück (R20).
- Entwicklermodus: Antworten des Entwicklungsservers ohne holzis CSP; Einschalten lädt das Hauptfenster neu
  (R16).
- Auf Android laufen Erweiterungen erst mit der wry-Korrektur (R25); auf Mobilgeräten keine Shell, kein
  Beobachten von Ordnern und nur Dateien aus der System-Auswahl.
- Ganze Zahlen über 2^53 kommen ungenau an, wie in haex-vault 2.x (R7).
- Hat ein Rahmen den Fokus, kann die Erweiterung ein gebundenes Kürzel von holzi auslösen, als hätte der Nutzer
  die Taste gedrückt; ohne Fokus nicht (contracts/bridge.md §Rahmen-Shim, R17).
- Eine Erweiterung, die nur mit `allow-same-origin` funktioniert (Service Worker, IndexedDB), läuft nicht
  (R12).
