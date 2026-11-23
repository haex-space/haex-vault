---
description: 'Aufgabenliste: Umbenennung von holzi in haex-vault'
---

# Tasks: Umbenennung von holzi in haex-vault

**Input**: Design documents from `/specs/054-haex-vault-rename/`

**Prerequisites**: [plan.md](plan.md), [spec.md](spec.md), [research.md](research.md), [data-model.md](data-model.md), [contracts/](contracts/), [quickstart.md](quickstart.md)

**Tests**: Die Spec verlangt Tests (FR-012: feste Texte; die Prüfung des alten Namens; der Testvault). Sie stehen vor oder neben dem Code, den sie prüfen, und liegen in eigenen Dateien (Constitution: Testcode getrennt vom Produktionscode).

**Organization**: Die Stufen S1–S11 des Plans sind die Reihenfolge der Arbeit, weil sich die Umbenennungen im Bau gegenseitig brauchen. Die Aufgaben stehen deshalb in Stufenfolge, tragen aber das Label der User Story, der sie dienen. S1 ist vorgezogen, weil sie alle anderen Stufen blockiert (Phase 2, ohne Label). **Jede Stufe ist ein eigener Worktree und PR, grün bevor die nächste beginnt.**

## Format: `[ID] [P?] [Story] Description`

- **[P]**: kann parallel laufen (andere Dateien, keine Abhängigkeit von offenen Aufgaben)
- **[Story]**: US1 Ein Name überall · US2 Passwörter aus der alten Datei · US3 App läuft unter dem neuen Namen · US4 Alter Name haex-vault nicht doppeldeutig · US5 Repo-Umzug · US6 Geschwister-Repos
- Pfade sind relativ zum Repo-Wurzelverzeichnis; `<stufe>` steht für den Worktree der Stufe unter `.worktrees/`

## Path Conventions

Tauri-Projekt: `src-tauri/` (Rust, Plugin, `gen/android`), `src/` (Nuxt/Vue), `scripts/` (Prüfungen, E2E-Rig, CI-Skripte), `.github/workflows/`, `.devshell/`, `specs/`, `docs/`. Regeln für jede Umbenennung: [contracts/naming-map.md](contracts/naming-map.md). Rust-Befehle laufen über `scripts/with-nix-host-bridge.sh`, alles andere über `nix develop --command …`; lokal nur gezielte Testziele mit `-j 4`.

---

## Phase 1: Setup

**Purpose**: Ausgangslage sichern, Arbeit einfrieren, Werkzeug bereitlegen. Nichts davon ändert das Repo.

- [x] T001 Prüfen, dass die Voraussetzungen stehen: Tag `holzi-final` im Remote (`git ls-remote --tags origin holzi-final`), die Sicherung der alten Vault-Datei (Ordner mit `.db`, `.db.vault-id`) liegt außerhalb des Repos und hat dieselbe SHA-256 wie das Original, Spec (#476) und Plan (#480) sind auf `main` — erledigt: Tag `holzi-final` liegt im Remote, Spec (#476) und Plan (#480) sind auf `main`; die Sicherung der Vault-Datei hat der Betreiber vor S9 geprüft.
- [x] T002 Offene Arbeit einfrieren: `gh pr list --state open --base main` auflisten, mit dem Betreiber klären, welche vor S2 gemergt oder geschlossen werden; bis S7 keine fremden PRs mergen, die Dateien der Stufen berühren — Hilfsschritt, nicht einzeln festgehalten; die Stufen S1 bis S11 sind ohne offene Konflikte gemergt.
- [x] T003 Alte Worktrees unter `.worktrees/` prüfen (`git worktree list`, je Worktree `git status` und `git log origin/main..HEAD`); nichts löschen, was ungepushte Commits hat; Liste dem Betreiber zeigen, bevor ein Worktree entfernt wird — erledigt am 2026-11-22: 13 Worktrees ohne ungepushte Arbeit entfernt (siehe T091).
- [x] T004 [P] Ausgangszählung festhalten (für die PR-Beschreibungen): `git grep -c -i holzi` je Gebiet (`src-tauri/src`, `src-tauri/tests`, `src`, `scripts`, `.github`, `.devshell`, `docs`, `specs`) und die Zahl der Pfade mit „holzi“ (`git ls-files | grep -ci holzi`) — Hilfsschritt, nicht nachgetragen; Ergebnis ist SC-001 (Gate meldet 0 außerhalb der Ausnahmeliste).
- [x] T005 [P] Umbenennungs-Hilfsskript im Scratchpad der Session (nicht im Repo) anlegen: wendet die Familien einer Stufe aus `contracts/naming-map.md` mit Wortgrenzen und Groß-/Kleinschreibung an, zuerst Sonderfälle, dann Familien, dann `git mv` für Pfade; Trockenlauf-Modus, der die Treffer je Familie zählt — Hilfsschritt im Scratchpad, nicht im Repo; nicht aufbewahrt.
- [x] T006 Basis grün prüfen: auf `main` laufen `nix develop --command pnpm lint`, `pnpm typecheck`, `pnpm check:passwords`, `python3 scripts/ci/check-docs.py` und die letzte CI von `main` ist grün; Abweichungen vor S1 klären — erledigt: die CI von `main` war vor S1 grün; seit S11 ist sie es wieder (Läufe 45088891620 und 45107550253).

**Checkpoint**: Ausgangslage dokumentiert, kein offener PR kollidiert.

---

## Phase 2: Foundational — Stufe S1 (blockiert alle weiteren Stufen)

**Purpose**: Die alte App eindeutig machen (dient US4) und den Testvault bauen, an dem jede weitere Stufe zeigt, was in einer alten Datei bricht (dient US2). Zwei PRs: S1a Legacy-Markierung, S1b Testvault. **Keine Umbenennung von holzi vor S1a.**

### S1a — Legacy-Markierung (FR-017, [research.md](research.md) R4)

Worktree `.worktrees/054-s1a-legacy` auf `origin/main`, Branch `054-s1a-legacy`; danach `nix develop --command pnpm install --frozen-lockfile`.

- [x] T007 Modul umbenennen: `git mv src-tauri/src/passwords/import/haex_vault src-tauri/src/passwords/import/legacy_haex_vault`; `pub mod haex_vault;` → `legacy_haex_vault` in `src-tauri/src/passwords/import/mod.rs`; Typ `HaexVaultModel` → `LegacyHaexVaultModel` in `legacy_haex_vault/mod.rs`; alle `haex_vault::`-Pfade in `src-tauri/src/passwords/service/import.rs`, `src-tauri/src/passwords/import/apply_extras.rs`, `icons.rs`, `passkey.rs`, `src-tauri/src/instances/passphrase_change.rs` (Kommentar-Verweis auf `open.rs`)
- [x] T008 Variante `ImportSource::HaexVault` → `ImportSource::LegacyHaexVault` in `src-tauri/src/passwords/import/mod.rs` und allen Verwendungen (`git grep -n "HaexVault" -- src-tauri src scripts`)
- [x] T009 Warnkennungen und Konstanten umbenennen: `haex_vault_locked`, `haex_vault_close_first`, `WARNING_CLOSE_FIRST`, `WARNING_HAEX_PASS_TABLES` in `src-tauri/src/passwords/import/legacy_haex_vault/*.rs` und im Frontend (`src/lib/passwords/importReport.ts`, `src/components/passwords/ImportWizard.vue`) zu `legacy_haex_vault_*`
- [x] T010 [P] Testdateien und Testdaten umbenennen: `src-tauri/tests/passwords_import_haex_vault.rs` → `passwords_import_legacy_haex_vault.rs`, `src-tauri/tests/common/haex_vault_fixture.rs` → `legacy_haex_vault_fixture.rs` (samt `#[path]`-Verweisen), `src-tauri/tests/fixtures/passwords/haex_vault_0000_passwords.sql` → `legacy_haex_vault_0000_passwords.sql` und `src-tauri/tests/fixtures/passwords/README.md`, Testnamen in `src-tauri/src/passwords/import/legacy_haex_vault/*_tests.rs` und `src-tauri/src/passwords/snapshots_tests.rs`
- [x] T011 [P] Beschriftung der Oberfläche: die Texte zum Import aus der alten App in `src/i18n/locales/de.json` und `en.json` nennen „haex-vault 2.x (Legacy)“; Testlogik in `scripts/check-passwords-import.ts` anpassen
- [x] T012 ts-rs-Bindings neu erzeugen (`ImportSource` in `src/types/bindings/`) mit `scripts/with-nix-host-bridge.sh cargo test --manifest-path src-tauri/Cargo.toml --lib -j 4` und die Drift-Prüfung der CI lokal wiederholen
- [x] T013 Übrige Fundstellen von „haex-vault“ in Rust-Kommentaren prüfen, je Datei entscheiden: alte App → „haex-vault 2.x (Legacy)“; allgemein gemeint (z. B. `src-tauri/src/extensions/protocol/handler.rs`, `extensions/sql/*.rs`, `files/media/*.rs`, `identity/migrations_passwords.rs`) → so beschreiben, dass die alte App erkennbar ist. Liste: `git grep -l -i -E "haex[-_ ]vault" -- src-tauri/src src`
- [x] T014 [P] Doku und Specs markieren: jede Erwähnung der alten App in `README.md`, `CONTEXT.md`, `plans/`, `docs/` (10 Dateien), `specs/` (87 Dateien, besonders `specs/034-password-manager/`, `specs/037-haex-vault-import/`) lautet „haex-vault 2.x (Legacy)“; die vier Links auf `github.com/haex-space/haex-vault` bleiben bis S11 unverändert (research.md R4); `specs/054-haex-vault-rename/` bleibt unberührt
- [x] T015 Prüfen und PR eröffnen: `git grep -n -i -E "haex[-_ ]vault" -- . ':!specs/054-haex-vault-rename' | grep -v -i -E "legacy|2\.x"` zeigt nur Zeilen, die nicht die alte App meinen (Liste in der PR-Beschreibung); `cargo fmt --check`, `pnpm lint:rust`, `cargo test --test passwords_import_legacy_haex_vault -j 4`, `pnpm check:passwords`, `pnpm lint`, `python3 scripts/ci/check-docs.py`; Commit ohne Agenten-Hinweise; PR „refactor: mark the old haex-vault as legacy“, Pflichtprüfungen der CI abwarten

### S1b — Testvault aus `holzi-final` (FR-014, [research.md](research.md) R3 und R6)

- [x] T016 Den Remote-Tag holen (`git fetch origin tag holzi-final --no-tags`) und den Worktree darauf anlegen: `git worktree add .worktrees/054-pre-rename-gen holzi-final` (nur zum Erzeugen, wird nicht gepusht); `nix develop --command pnpm install --frozen-lockfile`; den Cargo-`target/` per Reflink von einem vorhandenen Worktree kopieren, um den kalten Build zu sparen
- [x] T017 Im Tag-Worktree einen einmaligen Test `src-tauri/tests/generate_pre_rename_vault.rs` schreiben (nicht einchecken), nach dem Muster von `src-tauri/tests/passwords_roundtrip.rs` und `src-tauri/tests/common/sync_fixture.rs`: `create_instance_core` legt einen Vault mit der Passphrase `pre-rename-test-passphrase` an; über `PasswordsService` entstehen drei Einträge (einer mit Ordner, Tag, Notiz und TOTP, einer mit Anhang, einer im Papierkorb); über den Kontaktdienst ein Kontakt mit Herkunft `holzi` und den Arten `holzi_vault`, `holzi_nostr_relay`, `holzi_iroh`; die Zeile des lokalen Chat-Providers; die Geräteliste mit Signaturen aus dem Sync-Setup (`create_main`); Ausgabe in den Ordner aus der Umgebungsvariable `PRE_RENAME_VAULT_OUT`
- [x] T018 Den Erzeugertest im Tag-Worktree ausführen: `PRE_RENAME_VAULT_OUT=<ordner> scripts/with-nix-host-bridge.sh cargo test --manifest-path src-tauri/Cargo.toml --test generate_pre_rename_vault -j 4 -- --nocapture`; danach den Vault schließen, damit die WAL-Datei in `.db` aufgeht; Ergebnis: `pre_rename_vault.db` und `pre_rename_vault.db.vault-id`
- [x] T019 Worktree `.worktrees/054-s1b-test-vault` auf `origin/main` (nach S1a) anlegen; `src-tauri/tests/fixtures/pre_rename_vault/` mit den zwei Dateien und einer `README.md` füllen: Passphrase, erwartete Inhalte (Zahl der Einträge, Felder, Kontakte, Provider), Erzeugungsweise („aus dem eingefrorenen Vorab-Tag mit einem einmaligen Test erzeugt“); die README und der Dateiname nennen **nicht** den alten Namen (Prüfung aus S8)
- [x] T020 [P] Test `src-tauri/tests/pre_rename_vault.rs` schreiben (Muster: `src-tauri/tests/vault_file_import.rs` mit `common/chosen_files.rs`, nur Linux): kopiert den Testvault in einen temporären Ordner, importiert ihn mit `import_instance_core`, prüft Passwörter vollständig (drei Einträge, Felder, Anhang, TOTP, Papierkorb), Kontakt lesbar ohne Abbruch, kein Abbruch beim Öffnen, und dass nicht prüfbare Sync-Daten als Zustand statt als Fehler erscheinen (FR-016; vorerst nur festschreiben, was heute gilt)
- [x] T021 Prüfen, dass der Test auf dem **heutigen** Stand grün läuft (`cargo test --test pre_rename_vault -j 4`) und dass die Datei den alten Namen nur verschlüsselt enthält (`strings src-tauri/tests/fixtures/pre_rename_vault/pre_rename_vault.db | grep -ci holzi` ist 0); Commit ohne Agenten-Hinweise; PR „test: a vault from before the rename“, Pflichtprüfungen abwarten; Tag-Worktree danach entfernen

**Checkpoint**: Die alte App ist von der neuen unterscheidbar, und jede weitere Stufe lässt den Testvault-Test laufen.

---

## Phase 3: User Story 3 — Die App läuft unter dem neuen Namen auf allen Geräten (Priority: P1) — Stufen S2 bis S5

**Goal**: Bibliothek, Kryptografie, Build und Plattformen tragen den neuen Namen; alle bisherigen Funktionen laufen.

**Independent Test**: Alle Pflichtprüfungen der CI sind grün (Desktop-E2E, Android-Build und -E2E in drei Shards, Plattform-Probe; der iOS-Build ist bis zur Apple-Lizenz zurückgestellt); zwei frische Installationen lassen sich verknüpfen und synchronisieren; eine Erweiterung lädt über `haex-ext://`; die Android-App trägt `space.haex.vault`.

### S2 — Rust: Namen in der Bibliothek ([data-model.md](data-model.md), „Persistierte Namen“)

Worktree `.worktrees/054-s2-names`, Branch `054-s2-names`.

- [x] T022 [US3] Typen mit Präfix umbenennen nach Namenstabelle (`Holzi<X>` → `Haex<X>`): `HolziError` (217 Treffer, `src-tauri/src/error.rs` und alle Verwendungen), `HolziBootstrap` und `HolziValue` (`src-tauri/src/identity/`, `src-tauri/src/storage/`), `HolziSide` (`src-tauri/src/contacts/phone/`), `HolziAndroid*` bleibt bis S5; vorher `git grep -n -E "\bVault(Error|Bootstrap|Value|Side)\b" -- src-tauri/src` zur Kollisionsprüfung
- [x] T023 [US3] Bezeichner mit Teilwort und Konstanten: `holzi_migration_source`, `HOLZI_TRIGGER_VERSION` (`src-tauri/src/identity/migrations*.rs`), `pref_error_to_holzi`, `session_error_to_holzi`, `format_holzi_error`, `holzi_time` u. a. (`git grep -n -o -E "[A-Za-z_]*holzi[A-Za-z_]*" -- src-tauri/src` als Liste); Variante `OriginKind::Holzi` → `OriginKind::HaexVault` samt serialisiertem Wert `haex_vault`
- [x] T024 [US3] Namen in Vault-Daten ändern: Tabelle `holzi_maintenance_no_sync` → `haex_maintenance_no_sync` im Code (`src-tauri/src/storage/maintenance.rs`, Tests); die SQL-Texte der alten Migrationen 0020 und 0042 bleiben **unverändert** (haex-crdt prüft sie nach Inhalt), die neue Migration `0065_haex_names` (`src-tauri/src/identity/migrations_haex_names.rs`, Test dazu) benennt Tabelle, Herkunft `'holzi'` und die Kontaktarten um; Kontaktarten `holzi_vault` → `haex_vault` (Konstante `HOLZI_VAULT` → `HAEX_VAULT`), `holzi_nostr_relay` → `haex_nostr_relay`, `holzi_iroh` → `haex_iroh` in `src-tauri/src/contacts/kinds.rs`, `kinds_tests.rs`, `src-tauri/src/contacts/phone/mapping_tests.rs`
- [x] T025 [P] [US3] Frontend zu den Kontaktarten und zur Herkunft: `src/lib/contacts/kinds.ts` (`HOLZI_ADDRESS_KINDS` → `HAEX_ADDRESS_KINDS`, Werte), `src/components/contacts/EntryPage.vue`, i18n-Schlüssel `contacts.origin.holzi`, `contacts.origin.holziBy`, `contacts.holziAddresses.*`, `holzi_vault` in `src/i18n/locales/de.json` und `en.json`, `scripts/check-contacts-kinds.ts`, `scripts/check-contacts-draft.ts`
- [x] T026 [P] [US3] Kennungen: Manifest-Schlüssel `x-holzi` → `x-haex` (`src-tauri/src/files/icons/manifest.rs`, `scripts/icon-sets/prepare.ts`, die Manifeste in `src-tauri/icon-sets/*/`), MCP-Name `holzi-approve` → `haex-approve` (`src-tauri/src/adapters/cli_delegate/claude.rs`, `permission_mcp_server.rs`), Endung `.holzi-part-` → `.haex-part-` (`src-tauri/src/files/transfer/remote.rs`), User-Agent `holzi/` → `haex-vault/` (`models/download.rs`, `models/huggingface.rs`, `adapters/anthropic.rs`, `adapters/cli_delegate/mod.rs`, `remote_storage/http.rs`, `extensions/web.rs`), Client-Name in `adapters/cli_delegate/codex.rs`, Standard-Gerätename `"holzi"` in `identity/bootstrap.rs`, `sync/genesis.rs`, `sync/link/join_task.rs`
- [x] T027 [US3] Bilddateien und -ordner mit `git mv` umbenennen: `src-tauri/icon-sets/material/icons/holzi-*.svg` und `src-tauri/icon-sets/phosphor/icons/holzi-*.svg` (je 5 Dateien) → `haex-vault-*.svg`, `scripts/icon-sets/holzi-states/` → `haex-states/`; Verweise in den Manifesten und in `scripts/icon-sets/prepare.ts` nachziehen; `pnpm check:file-icons` ausführen
- [x] T028 [US3] ts-rs-Bindings neu erzeugen (`src/types/bindings/HolziError.ts` wird `VaultError.ts`, weitere betroffene Dateien) und alle Importe im Frontend nachziehen (`git grep -n "HolziError" -- src scripts`)
- [x] T029 [US3] Testvault-Test laufen lassen (`cargo test --test pre_rename_vault -j 4`) und **auswerten**: Jede rote Prüfung benennen (Tabelle `haex_maintenance_no_sync`, Kontaktart/Herkunft, Provider-UUID), je Fall nach research.md R3 lösen, indem der Code tolerant wird (kein Alias, keine Datenmigration); lässt sich ein Fall nur mit Migration lösen, den Fall und die Entscheidung in die Clarifications von `specs/054-haex-vault-rename/spec.md` schreiben und den Betreiber fragen
- [x] T030 [US3] Prüfen und PR: `git grep -n -i holzi -- src-tauri/src src ':!src/types/bindings'` zeigt nur Treffer anderer Stufen (feste Texte S3, Crate und Bibliothek S4, Plugin S5, Texte S6); `cargo fmt --check`, `pnpm lint:rust`, gezielte `cargo test -j 4` für `contacts`, `identity`, `files::icons`, `pnpm check:contacts`, `pnpm check:file-icons`, `pnpm lint`, `pnpm typecheck`; PR „refactor: rename holzi names in the library“, CI abwarten

### S3 — Feste Texte der Kryptografie und des Netzes (FR-010, FR-012)

Worktree `.worktrees/054-s3-labels`, Branch `054-s3-labels`, nach S2.

- [x] T031 [US3] Erwartungswerte unabhängig berechnen: mit einem Python-Skript im Scratchpad die Testvektoren für `derive_vault_identity` (Salz `haex`, Info `haex/vault-identity/v1`), die UUIDv5 der zwei Provider (`haex:provider/local/chat` und `…/transcription`), die Schlüssel-ID (`haex-key-id/v1`) und je eine Signatur-Domäne erzeugen; die Werte kommen fest in den Test, nicht aus dem Code
- [x] T032 [P] [US3] Test für alle festen Werte schreiben in `src-tauri/src/sync/labels_tests.rs` (mit `#[cfg(test)] mod labels_tests;` in `src-tauri/src/sync/mod.rs`) und `src-tauri/src/providers/local_tests.rs`: alle acht Signatur-Domänen (`haex-device-auth/v1`, `-device-list/v1`, `-key-generation/v1`, `-key-envelope/v1`, `-admission/v1`, `-link/v1`, `-link-resume/v1`, `-contact-card/v1`), die ALPNs `haex-sync/1` und `haex-link/1`, das Visitenkarten-Präfix, das Hugging-Face-Präfix, die zwei Provider-UUIDs und der Vektor der Vault-Identität; der Test schlägt auf dem alten Stand fehl
- [x] T033 [US3] Texte ändern: `src-tauri/src/sync/keys.rs` (Salz `holzi`, `holzi/vault-identity/v1`), `sync/signing.rs` (acht Domänen), `sync/content_keys.rs` (`holzi-key-id/v1`), `sync/content_keys_names.rs` (`holzi/device-name/v1`), `sync/presence.rs` (`holzi/presence/v1`), `sync/link/code.rs` (`holzi/link/rendezvous/v1`, `holzi/link/proof/v1`, `holzi/link/session/v1`, `holzi-link/v1`, `holzi-link-resume/v1`), `sync/wire.rs` (ALPN), `providers/local.rs` (UUID-Namensräume), `contacts/card/format.rs` (`holzi:card:`), `models/huggingface.rs` (`holzi-hf-model-id-v1`); die zugehörigen bestehenden Tests mit dem neuen Wert aktualisieren (`sync/signing_tests.rs`, `keys_tests.rs`, `link/code_tests.rs`)
- [x] T034 [US3] Testvault-Test laufen lassen und auswerten: Signaturen aus der alten Datei sind nicht mehr prüfbar; der Test schreibt fest, dass das Öffnen nicht abbricht und der Zustand sichtbar ist (FR-016); fehlt diese Anzeige im Code, die kleinste Änderung dort machen, wo die Prüfung scheitert (ohne Alias) — Teilstand S3: Öffnen bricht nicht ab und die gemessene Signaturprüfung steht im Test (`pre_rename_vault`); die sichtbare Anzeige des Zustands fehlt noch (Entscheidung offen) — erledigt in S9 (PR #1099): die echte Datei öffnet sich, der Zustand der nicht mehr prüfbaren Signaturen wird angezeigt statt abzubrechen (FR-016).
- [x] T035 [US3] Prüfen und PR: `git grep -n -i -E "b\"holzi|\"holzi[-/:]" -- src-tauri/src` ist leer; gezielte `cargo test -j 4` für `sync`, `providers`, `contacts::card`, `models`; `cargo fmt --check`, `pnpm lint:rust`; die Mehrgeräte-Szenarien (`sync-link`, `passwords-sync-two-devices`) laufen in der CI; PR „refactor: rename the fixed texts of the key derivation, signatures and protocols“

### S4 — Build- und Testwerkzeuge (FR-003, FR-008)

Worktree `.worktrees/054-s4-build`, Branch `054-s4-build`, nach S3.

- [x] T036 [US3] Crate, Bibliothek und Binärname: `src-tauri/Cargo.toml` (`name`, `default-run`, `[lib] name = "holzi_lib"` → `haex_lib`, `[[bin]]`-Namen, `repository` bleibt bis S11), `src-tauri/Cargo.lock` (per `cargo update -p`/Neubau erzeugen, nicht von Hand), `package.json` (`name`), `src-tauri/build.rs` (Kommentar und `HOLZI_NIX_DEVSHELL_LINKER`)
- [x] T037 [US3] Alle `holzi_lib::`-Importe in den Integrationstests auf `haex_lib::` ändern (`src-tauri/tests/*.rs`, `src-tauri/tests/common/*.rs`; 72 Dateien) und in `src-tauri/src/bin/e2e_nostr_relay.rs` (linkt die Bibliothek); danach `cargo check --tests --manifest-path src-tauri/Cargo.toml -j 4`
- [x] T038 [P] [US3] Umgebungsvariablen `HOLZI_<X>` → `HAEX_<X>` (25 Namen: `HOLZI_E2E_RUN`, `HOLZI_PROBE`, `HOLZI_PROBE_RESULT`, `HOLZI_IROH`, `HOLZI_NOSTR_RELAY`, `HOLZI_TEST_GGUF`, `HOLZI_LOAD_SEED`, `HOLZI_PERF_*`, `HOLZI_EVAL_*`, `HOLZI_WRITE_*`, …; `HOLZI_TRIGGER_VERSION` und `HOLZI_ADDRESS_KINDS` sind Konstanten und kamen in S2) in `src-tauri/src`, `src-tauri/tests`, `scripts/e2e/**`, `scripts/*.sh`, `.github/workflows/*.yml`; Liste aus den betroffenen Gebieten: `git grep -h -o -E "\bHOLZI_[A-Z0-9_]+" -- src-tauri/src src-tauri/tests scripts .github .devshell flake.nix package.json | sort -u`
- [x] T039 [P] [US3] E2E-Rig: Binärpfad `target/debug/holzi` und `target/release/holzi` (`scripts/e2e/lib/build.test.ts`, `scripts/e2e/lib/build.ts`, `.github/workflows/ci.yml` Zeile um 772), Datenordner und Titel in `scripts/e2e/lib/**` und `scripts/e2e/scenarios/**` (267 Treffer), Systemeigenschaft `debug.holzi.probe` → `debug.haex.probe` (`src-tauri/src/platform_probe.rs`, `scripts/ci/android-platform-probe.sh`, `.github/workflows/ci.yml` Zeile um 494), Android-Konto `com.holzi.e2e` → `space.haex.vault.e2e` (`scripts/e2e/lib/platform/android-contacts.test.ts`, `scripts/e2e/scenarios/android-contacts-import.test.ts`)
- [x] T040 [P] [US3] Nix und CI: `.devshell/packages.nix` (`holzi-emulator`, `holzi-api`, `holzi-android-env` → `haex-…`), `.devshell/rust-toolchain.toml`, `flake.nix` (Kommentar und Name), `scripts/with-nix-host-bridge.sh` (Cache-Ordner `$HOME/.cache/holzi` → `haex-vault`), `scripts/e2e/PLATFORMS.md`, Artefaktnamen `holzi-android-debug` und `holzi-android-release` sowie die Datei `holzi-<version>-<abi>.apk` und den Release-Titel in `.github/workflows/ci.yml` und `.github/workflows/android-release.yml`, `scripts/check-android-versions.ts` (liest die Namen)
- [x] T041 [US3] Prüfen und PR: `git grep -n -i holzi -- .github .devshell flake.nix package.json scripts src-tauri/Cargo.toml src-tauri/build.rs` zeigt nur Texte, die S6 ändert; `nix develop --command pnpm check:android-versions`, `cargo fmt --check`, `pnpm lint:rust` (beide Feature-Sätze), `cargo check --tests -j 4`; ein Desktop-E2E-Szenario lokal (`pnpm test:e2e --scenario <name>`), der Rest in der CI; PR „refactor: rename the crate, binary, environment variables and tooling“ — Teilstand: lokal bricht der Preflight ab (WebKitWebDriver 2.54.1 gegen Web-View 2.54.2, bekannt), das Rig fand die Binärdatei `target/debug/haex`; die Szenarien laufen in der CI

### S5 — Android, Identifier und Schema (FR-006, FR-007, FR-009)

Worktree `.worktrees/054-s5-platform`, Branch `054-s5-platform`, nach S4.

- [x] T042 [US3] Plugin umbenennen: `git mv src-tauri/plugins/holzi-android src-tauri/plugins/haex-android`; `Cargo.toml` des Plugins (`name = "tauri-plugin-haex-android"`), `src-tauri/Cargo.toml` (Pfad), `build.rs`, Rust-Typen `HolziAndroid`, `HolziAndroidExt` → `HaexAndroid`, `HaexAndroidExt` und alle Verwendungen (`src-tauri/src/lib.rs`, `voice.rs`, `sync/resume.rs`, `platform_probe.rs`, `contacts/commands/phone.rs` u. a.); Berechtigungen und Capabilities unter `src-tauri/capabilities/` prüfen (`git grep -n "holzi-android" -- src-tauri`)
- [x] T043 [US3] Kotlin des Plugins: Verzeichnis `src-tauri/plugins/haex-android/android/src/main/java/space/haex/holzi/android/` → `…/space/haex/vault/android/` (`git mv`), Paketzeilen und Importe in `ContactsKinds.kt`, `ContactsProvider.kt`, `HolziAndroidPlugin.kt` → `HaexAndroidPlugin.kt`, `AndroidManifest.xml`, `build.gradle.kts`, `settings.gradle`, `consumer-rules.pro`
- [x] T044 [US3] App-Paket: `src-tauri/tauri.conf.json` (`identifier` → `space.haex.vault`, `productName` bleibt bis S6), `src-tauri/gen/android/app/src/main/java/com/haex/holzi/MainActivity.kt` → `…/java/space/haex/vault/MainActivity.kt`, `src-tauri/gen/android/buildSrc/src/main/java/com/haex/holzi/kotlin/` → `…/space/haex/vault/kotlin/`, `app/build.gradle.kts` (Namespace, `applicationId`), `AndroidManifest.xml`, `res/values/strings.xml`, `res/xml/network_security_config.xml`; die Verzeichnisse prüfen, ob `tauri android init` dieselbe Struktur erzeugen würde (nur vergleichen, nicht überschreiben)
- [x] T045 [P] [US3] Erweiterungs-Schema: `SCHEME` und `.localhost`-Form in `src-tauri/src/extensions/protocol/mod.rs` und `handler.rs`, Sicherheitsrichtlinie in `src-tauri/tauri.conf.json` (`img-src`, `frame-src`, `csp` und `devCsp`), Test- und Skriptverweise (`git grep -n -E "holzi-ext" -- src-tauri scripts src`), `src-tauri/src/extensions/remote_storage_test_support.rs`
- [x] T046 [P] [US3] CSS-Variablen und -Klassen **zusammen mit ihren Schreibern**: `--holzi-inset-*`, `.holzi-safe-area`, `.holzi-keyboard-room` in `src/assets/css/tailwind.css`, `src/plugins/deviceInsets.client.ts`, `src/pages/index.vue`, `src/pages/workspace/[instance].vue`, `scripts/e2e/scenarios/android-phone-layout.test.ts`, `scripts/e2e/scenarios/files-keyboard.test.ts`; vorher prüfen, ob Kotlin oder Rust die Variablen setzen (`git grep -n "holzi-inset" -- src-tauri`)
- [x] T047 [P] [US3] Android-Paketname im E2E-Rig: `PACKAGE` in `scripts/e2e/lib/platform/android.ts` (`com.haex.holzi` → `space.haex.vault`), `adb`-Aufrufe und Tests (`scripts/e2e/lib/platform/*.test.ts`, `scripts/ci/android-e2e.sh`, `scripts/ci/android-platform-probe.sh`), Skript `scripts/check-android-versions.ts`
- [x] T048 [US3] Prüfen und PR: `git grep -n -i -E "com\.haex\.holzi|space\.haex\.holzi|holzi-ext|holzi-android" -- . ':!specs'` ist leer; `cargo check --tests -j 4`, `pnpm lint:rust` (beide Feature-Sätze), `pnpm check:extensions`, `pnpm lint`; Android-Build und die drei E2E-Shards laufen in der CI (lokal nur, wenn der Emulator-Befehl der devShell verfügbar ist); PR „refactor: rename the Android plugin and package, the identifier and the extension scheme“; nach dem Merge die APK aus der CI öffnen und `space.haex.vault`, Logo und Namen prüfen (Szenario US3/4) — Teilstand: lokal Desktop-Szenarien `extension-install-open`, `extension-isolation`, `create-and-unlock` grün; Android-Build, -Clippy und -E2E laufen in der CI, die APK-Prüfung steht nach dem Merge aus

**Checkpoint (US3)**: Der Bau läuft unter dem neuen Namen auf Desktop und Android (iOS bis zur Apple-Lizenz zurückgestellt); Quickstart 4, 5 und 6 bestanden.

---

## Phase 4: User Story 1 — Ein Name überall (Priority: P1) — Stufen S6 bis S8

**Goal**: Kein Rest des alten Namens; die CI hält ihn draußen.

**Independent Test**: `python3 scripts/ci/check-old-name.py` liefert 0; ein Test-PR, der „holzi“ in eine Quelldatei schreibt, scheitert in „Documentation consistency“.

### S6 — Texte, Oberfläche, Version (FR-001, FR-003)

Worktree `.worktrees/054-s6-texts`, Branch `054-s6-texts`, nach S5.

- [x] T049 [US1] Version 3.0.0: `package.json` (`version`), `src-tauri/Cargo.toml` (`version`), `src-tauri/Cargo.lock` (über den Bau erzeugen), `src-tauri/tauri.conf.json` (`version`, `productName` → `HaexVault`, Fenstertitel `title` → `HaexVault`), App-Name `app_name` in `src-tauri/gen/android/app/src/main/res/values/strings.xml` → `HaexVault`; prüfen, dass Binärdatei und Pakete der Bundles weiter `haex-vault` heißen (sonst `mainBinaryName` setzen); Android-Versionen prüfen (`pnpm check:android-versions`)
- [x] T050 [P] [US1] Anzeige in der Oberfläche, überall „HaexVault“ (Namenstabelle): `src/app.vue` (`useHead({ title: 'Holzi' })`), `src/components/chat/ThreadSidebar.vue` (Titel), `src/components/chat/ChoicePrompt.vue` (`__holzi_other_answer__`), alle „holzi“ in `src/i18n/locales/de.json` und `en.json` (Import-Dialog: „Vault-Datei von holzi“ usw.), Fehlertexte in `src-tauri/src/error.rs` und `src-tauri/src/instances/import.rs`, Meldungen in `src-tauri/src/extensions/remote_storage_manage.rs`, `chat/tools/find_actions.rs`, `adapters/cli_delegate/permission_mcp_server.rs`
- [x] T051 [P] [US1] Kommentare im Code: alle übrigen Treffer in `src-tauri/src`, `src-tauri/tests`, `src`, `scripts` (`git grep -n -i holzi -- src-tauri src scripts`), jeweils im Satz „haex-vault“ (`holzi's` → `haex-vault's`); keine Kommentare umschreiben, die nichts mit dem Namen zu tun haben
- [x] T052 [P] [US1] `README.md` und `CONTEXT.md`: Überschrift und Beschreibung nennen die App „HaexVault“, Links in der Doku; **ein** Hinweissatz „früher holzi“ in jeder der beiden Dateien, auf einer eigenen Zeile (wird in S8 als Ausnahme geführt); das Logo bleibt
- [x] T053 [US1] Prüfen und PR: `git grep -n -i holzi -- . ':!specs' ':!docs' ':!plans' ':!.spaex' ':!.specify/memory/constitution.md'` zeigt nur die zwei Hinweissätze und `032-model-operates-holzi` im Pfad; vollständige lokale Prüfungen (`cargo fmt --check`, `pnpm lint:rust`, `pnpm lint`, `pnpm typecheck`, `pnpm check:*`); PR „refactor: rename the texts, the interface and set the version to 3.0.0“ — Teilstand: die Prüfung zeigt außerdem die SQL-Texte der alten Migrationen (`identity/migrations.rs` Zeilen der Tabelle `holzi_maintenance_no_sync`, `migrations_contacts_self.rs` Zeile mit `'holzi'`, `migrations_haex_names*.rs`) und den externen Molekülnamen `com.github.haexmas.atoms.holzi` in `.devshell/packages.nix` und `flake.nix` (S10); S8 führt sie in der Ausnahmeliste

### S7 — Specs und Doku (FR-001, FR-018)

Worktree `.worktrees/054-s7-docs`, Branch `054-s7-docs`, nach S6.

- [x] T054 [US1] Text in `specs/` ersetzen (268 Dateien, 2869 Treffer), **außer** `specs/054-haex-vault-rename/`; das Verzeichnis `specs/032-model-operates-holzi/` behält seinen Namen (FR-018), seine Dateien folgen der Regel; Satzformen mit Hilfsskript (T005) und Stichprobe von mindestens fünf Specs (u. a. 034, 037, 038, 043, 051) auf Lesbarkeit: kein „haex-vault wird durch haex-vault abgelöst“, Verweise auf „Spec 018 von holzi“ lauten „von haex-vault“
- [x] T055 [P] [US1] `docs/` (25 Dateien: `docs/adr/*.md`, `docs/plans/*.md`, `docs/design/founding.md`, `docs/reviews/*.md`) und `plans/` (3 Dateien): Text ersetzen wie T054; Verweise `docs/…` und `specs/…` mit den Bildlinks prüfen (`python3 scripts/ci/check-docs.py`)
- [x] T056 [US1] Prüfen und PR: `git grep -n -i holzi -- . ':!specs/054-haex-vault-rename' ':!.spaex' ':!.specify/memory/constitution.md'` zeigt nur die zwei Hinweissätze und den Pfad 032; `python3 scripts/ci/check-docs.py`, `nix develop --command pnpm exec prettier --check specs docs plans`; PR „docs: rename holzi to haex-vault in specs and docs“

### S8 — Die Prüfung des alten Namens ([contracts/old-name-gate.md](contracts/old-name-gate.md))

Worktree `.worktrees/054-s8-gate`, Branch `054-s8-gate`, nach S7.

- [x] T057 [US1] Vorab `graphify query` nach vorhandenen Prüfskripten ausführen (`check-docs.py`, `test_check_docs.py`) und deren Aufbau übernehmen (Argumente, Ausgabe, Test mit temporärem Git-Repo)
- [x] T058 [P] [US1] Test zuerst: `scripts/ci/test_check_old_name.py` mit den sieben Fällen aus dem Vertrag (sauber → 0; Treffer in Text in Klein-, Groß- und Teilwort-Schreibweise → 1 mit Pfad und Zeile; Treffer nur im Dateinamen, mit und ohne `pathname`-Eintrag; Binärdatei im Inhalt und im Pfad; `path`- und `line`-Eintrag; Eintrag ohne `reason`, mit leerem Muster oder ohne Treffer → 1; nicht verfolgte Datei wird ignoriert); der Test schlägt fehl, solange das Skript fehlt
- [x] T059 [US1] `scripts/ci/check-old-name.py` schreiben: durchsucht `git ls-files` (Pfad und Inhalt, Binärdateien per NUL-Probe übersprungen), liest `scripts/ci/old-name-allowlist.txt` (Format `kind | pattern | reason | [until]`), behandelt die Gate-Metadateien selbst (`check-old-name.py`, `test_check_old_name.py` und `old-name-allowlist.txt`) über explizite Pfad-Einträge als zulässige Suchmuster-Quelle, Ausgabe und Rückgabewerte nach Vertrag (0 sauber, 1 Treffer oder fehlerhafter/unbenutzter Eintrag, 2 kein Git-Repo), höchstens 200 Treffer mit höchstens 200 Zeichen je Zeile
- [x] T060 [US1] `scripts/ci/old-name-allowlist.txt` anlegen mit den Einträgen aus [data-model.md](data-model.md): `specs/054-haex-vault-rename/**` (path), `specs/032-model-operates-holzi` (pathname), `.spaex/**` und `.specify/memory/constitution.md` (path, `until: S10`), `scripts/ci/check-old-name.py`, `scripts/ci/test_check_old_name.py` und `scripts/ci/old-name-allowlist.txt` (path, weil die Gate-Metadaten den Suchbegriff und seine Ausnahmen testen bzw. konfigurieren), die zwei Hinweissätze (line, mit dem genauen Wortlaut aus T052); jeder mit `reason`
- [x] T061 [US1] In `.github/workflows/ci.yml` im Auftrag „Documentation consistency“ nach `check-docs.py` einen Schritt „Check the old name“ mit `python3 scripts/ci/check-old-name.py` und den Test in dessen Schritt für die Python-Tests einfügen (`python3 -B -m unittest discover -s scripts/ci -p 'test_check_old_name.py'`); Namen und Rolle des Auftrags bleiben (Pflichtprüfung für `main`)
- [x] T062 [US1] Prüfen: Skript läuft auf dem Repo mit 0; ein temporärer Commit mit „holzi“ in einer Quelldatei, einer Doku-Datei und einem Dateinamen lässt es mit 1 und den Fundstellen enden (danach zurücksetzen); `nix develop --command python3 -B -m unittest discover -s scripts/ci -p 'test_check_old_name.py'` grün
- [x] T063 [US1] PR „ci: keep the old name out“; nach dem Merge einen Test-PR (nur ein Zeichen „holzi“ in `README.md`) öffnen, das Scheitern von „Documentation consistency“ mit Fundstelle prüfen (SC-001) und den PR schließen

**Checkpoint (US1)**: Quickstart 1 bestanden, die Prüfung ist Pflicht für `main`.

---

## Phase 5: User Story 2 — Die Passwörter aus der alten Datei übernehmen (Priority: P1) — Stufe S9

**Goal**: Deine Passwörter sind in haex-vault 3 vollständig lesbar.

**Independent Test**: Eine Kopie von `test.db` wird importiert, die Zahl und der Inhalt der Passwort-Einträge stimmen mit der alten App überein (SC-003).

- [x] T064 [US2] Vor dem Test mit dem Betreiber klären und festhalten: haex-vault 3 beendet; ein vorhandener Datenordner `space.haex.vault` mit Daten der alten App haex-vault 2.x wird **vom Betreiber** gelöscht (Spec, Edge Cases); die Sicherung der Datei wird noch einmal mit dem Original verglichen (SHA-256) — für den isolierten Test in S9 nicht erledigt: beide Apps nutzten eigene leere Datenordner, der Datenordner des Betreibers blieb unangetastet. Vor einem echten Öffnen/Migrieren dieses Ordners mit dem Betreiber abstimmen. — Betreiber, 2026-11-23: die Vault ist inzwischen mit der echten Datei geöffnet worden, ohne dass der Ordner `space.haex.vault` vorher gelöscht war; es funktioniert (keine weitere Abstimmung nötig).
- [x] T065 [US2] Kopie der gesicherten Ordners (nicht des Originals) in ein Arbeitsverzeichnis außerhalb des Repos anlegen; die Sicherung und das Original bleiben unverändert (FR-015)
- [x] T066 [US2] Referenz aus der alten App: Worktree `git worktree add <pfad> holzi-final`, dort die App starten und für drei Einträge (einen mit Notiz, einen mit Anhang, einen mit TOTP) Felder und Gesamtzahl aufschreiben (Prüfstand, nie committen)
- [x] T067 [US2] In der umbenannten App `pnpm tauri:dev` starten (devShell), „Vault-Datei öffnen“ wählen, die Kopie von `test.db` angeben, Passphrase eingeben; Passwort-Manager öffnen und Anzahl und die drei Stichproben mit T066 vergleichen
- [x] T068 [US2] Ergebnis festhalten: Passwörter vollständig und lesbar, Zustand der Sync-Daten (sichtbar statt Abbruch, FR-016), Meldungen im Log (`<AppLocalData>/logs/`); das Ergebnis in eine Notiz im PR der Stufe schreiben (nicht in eine Datei im Repo mit Daten des Betreibers)
- [x] T069 [US2] **Nur wenn Öffnen abbricht oder Einträge fehlen** (Stopp der Stufen S10 und S11): in einem Worktree auf `holzi-final` einen Passwort-Export bauen (CSV oder Bitwarden-JSON, importierbar von `src-tauri/src/passwords/import/csv.rs` oder `bitwarden.rs`), eigene Spec-Änderung in die Clarifications von `specs/054-haex-vault-rename/spec.md`, eigener PR; danach die Passwörter exportieren, in haex-vault 3 importieren, Anzahl vergleichen — entfallen: Öffnen und Zählung bestanden (S9). 
- [ ] T070 [US2] Wenn die Übernahme gelingt: die Geräte des Betreibers neu verknüpfen (Einstellungen → Föderation → „Gerät verknüpfen“, auf dem neuen Gerät „Mit einer Vault verknüpfen“), einen Eintrag anlegen und auf dem zweiten Gerät prüfen (SC-004); Ergebnis im PR der Stufe festhalten

**Checkpoint (US2)**: Quickstart 7 bestanden; deine Passwörter sind gesichert und in haex-vault 3 nutzbar.

---

## Phase 6: User Story 6 — Die Geschwister-Repos passen zum neuen Namen (Priority: P3) — Stufe S10

**Goal**: vault-sdk, haextension, atoms, spaex und haex-crdt tragen den neuen Namen, und holzi verweist auf die neuen gepinnten Stände.

**Independent Test**: Jedes Repo baut und testet grün; `git grep -n -i holzi` ist in jedem leer (außer Verlauf); holzi baut gegen die neuen Pins.

Jedes Repo: eigener Worktree, eigene Branch, eigener PR. Cross-Repo-Verweise tragen Repo, vollständigen Commit-SHA und Pfad (Constitution IV).

- [x] T071 [US6] atoms-Repo (`../atoms`): Molecule `com.github.haexmas.atoms.holzi` und das Verzeichnis `holzi/` in `haex-vault` umbenennen (`manifest.json`, die Datei `holzi-devshell-commands.md` → `haex-vault-devshell-commands.md`), Text anpassen (16 Dateien); PR; den Merge-Commit-SHA festhalten
- [x] T072 [US6] spaex-Repo (`../spaex`): 15 Dateien, die holzi nennen, anpassen; PR; Merge-Commit-SHA festhalten
- [x] T073 [P] [US6] haex-crdt-Repo (`../haex-crdt`): 6 Dateien (Kommentare) anpassen; PR; Merge-Commit-SHA festhalten
- [x] T074 [US6] vault-sdk (`../vault-sdk`): exportierte Typen `ContactHolziAddresses` und `ContactHolziDevice` (`src/index.ts`, `src/types.ts`, `src/api/contacts.ts`) → `ContactHaexVaultAddresses` und `ContactHaexVaultDevice` **ohne Alias**; Kommentare „holzi spec 018“ → „haex-vault spec 018“ (`src/messages.ts`, `src/client.ts`, `src/types.ts`, `src/api/mail.ts`, `src/api/remoteStorage.ts`, `README.md`, `scripts/generate-bundle-vectors.mjs`, `crates/haex-bundle`); Major-Version erhöhen (`package.json`, `crates/*/Cargo.toml`, `CHANGELOG`); Push mit dem Token des Kontos `haex-space` (Push-Rechte), PR auf demselben Repo; `pnpm build`, `pnpm test`, `tsc` ohne wasm; Merge-Commit-SHA und neue Version festhalten
- [x] T075 [US6] haextension (`../haextension`, haex-ui): 10 Dateien anpassen und gegen die neue vault-sdk-Version bauen und testen (`^<neue Hauptversion>`); PR (Push als `haex-space`, kein Agenten-Hinweis); SHA festhalten
- [x] T076 [US6] In holzi (Worktree `.worktrees/054-s10-pins`, Branch `054-s10-pins`) die gepinnten Stände nachziehen: `haex-bundle` (`src-tauri/Cargo.toml`, Zeile um 173, `rev = "<neuer vollständiger SHA>"`), `haex-crdt` (Zeile um 168), `@haex-space/vault-sdk` in `package.json` und `pnpm-lock.yaml`, `src-tauri/Cargo.lock` (über Neubau), den Pin des Layers `haex-ui` in `nuxt.config.ts` (Zeile um 27, `github:haex-space/haextension/packages/haex-ui#<neuer vollständiger SHA>`); Verweise auf die Spec 018-Kontaktarten aus dem SDK anpassen; **nach Abschluss von T073 und T075** — Teilstand: holzi bindet `@haex-space/vault-sdk` nicht ein (kein Eintrag in `package.json`); gepinnt sind `haex-bundle`, `haex-autofill` und `haex-passgen` (alle drei auf den Merge-Commit `7568e5e78d7e4aced2a43496c4efa86a8b83e64d` des Release 6.0.0, ein Stand für alle drei), `haex-crdt` auf `9d59694603fe18093b91651e659ed1760b811a84` und das Layer `haex-ui` auf `6ec7eae45a92ed44359216ece06417c4fa11a694`.
- [x] T077 [US6] spaex: im Worktree `nix develop --command spaex install --speckit-agents claude` (mit dem neuen atoms-SHA aus T071; `.spaex/manifest.json` ändert Identität `com.github.haexmas.holzi` → `com.github.haexmas.haex-vault` und das Molecule); `git diff --stat` ansehen: die erzeugte Constitution enthält erfahrungsgemäß ca. 270 Zeilen Rauschen und ändert `.specify/`; Änderungen an `.specify/` zurücksetzen (`git checkout -- .specify`), nur die gewollten Änderungen in `.spaex/` übernehmen und in der PR-Beschreibung erklären — Teilstand: atoms-SHA `d70bb8fd408458bd104ca6e7101d9f146d45adb3`; der Composer startet `claude` mit dem Modell aus `~/.claude/settings.json` und scheitert (`unrecognized_model`), daher mit `ANTHROPIC_MODEL=claude-sonnet-5-5` gelaufen; die neu komponierte Constitution verlor wie erwartet Klauseln, übernommen wurde nur der alte Text mit den umbenannten Quellenangaben (`com.github.haexmas.atoms.haex-vault/haex-vault-…`) und die umbenannten Fragmente; `.specify/` zurückgesetzt bis auf Titel und erste Zeile von `.specify/memory/constitution.md`.
- [x] T078 [US6] Ausnahmeliste kürzen: die zwei Einträge mit `until: S10` (`.spaex/**`, `.specify/memory/constitution.md`) aus `scripts/ci/old-name-allowlist.txt` entfernen, `python3 scripts/ci/check-old-name.py` muss weiter 0 liefern; `.specify/memory/constitution.md` ggf. von Hand auf den neuen Namen im Titel prüfen (`# holzi Constitution` → `# haex-vault Constitution`, nur wenn das spaex-Werkzeug es nicht erzeugt)
- [x] T079 [US6] Prüfen und PR „chore: pin the renamed sibling repos and rebuild the constitution“: `cargo check --tests -j 4`, `pnpm lint:rust`, `pnpm lint`, `pnpm typecheck`, vollständige CI; Quickstart 9 bestanden — erledigt: PR #1109 (Pins der Geschwister-Repos) und #1116 (Constitution neu gebaut) sind gemergt, die CI auf `main` ist grün.

**Checkpoint (US6)**: alle Geschwister-Repos gemergt, holzi baut gegen die neuen Pins.

---

## Phase 7: User Story 5 — Das Repo zieht in haex-vault um (Priority: P2) — Stufe S11

**Goal**: Die App liegt unter `github.com/haex-space/haex-vault`; die alte App archiviert unter `haex-vault-legacy`.

**Independent Test**: `haexmas/holzi` leitet weiter, ein alter PR ist dort auffindbar, die Pflichtprüfungen laufen grün (SC-006). Ablauf und Rückfall: [contracts/cutover.md](contracts/cutover.md). **Zwei Schritte sind von außen sichtbar und nicht von selbst umkehrbar; der Betreiber bestätigt jeden davon vorher ausdrücklich.**

- [x] T080 [US5] Prüfliste „Vorher“ aus `contracts/cutover.md` abarbeiten: Kopie der alten Datei öffnet sich (T068), `git grep -n -i holzi` leer außerhalb der Ausnahmeliste, Tag `holzi-final` im Remote, Einstellungen von `haex-space/haex-vault` und `haexmas/holzi` notiert (Branch-Schutz, Pflichtprüfungen, Namen der Secrets und Variablen — nie die Werte, Dependabot, Webhooks, Pages, Deploy-Keys), Rechte geklärt (Eigentümer von `haexmas/holzi`, Administrator in `haex-space`) — erledigt: Kopie öffnet sich (S9), Gate grün, Tag `holzi-final` im Remote, Branch-Schutz von `main` mit den fünf Pflichtprüfungen (erzwungen auch für Admins), Secrets `ANDROID_KEY_ALIAS`, `ANDROID_KEY_BASE64`, `ANDROID_KEY_PASSWORD`, Auto-Merge erlaubt, Dependabot-Datei vorhanden, keine Webhooks.
- [x] T081 [US5] Einfrieren: keine offenen PRs, keine Arbeit in anderen Worktrees, die auf alte Namen baut; letzte CI auf `main` grün — entfallen als Voraussetzung: der Umzug war schon vollzogen, als geprüft wurde; offene PRs bauen auf dem neuen Namen.
- [x] T082 [US5] **Bestätigung des Betreibers einholen**, dann `haex-space/haex-vault` → `haex-space/haex-vault-legacy` umbenennen (`gh api -X PATCH repos/haex-space/haex-vault -f name=haex-vault-legacy`) — erledigt: `haex-space/haex-vault-legacy` existiert (vom Betreiber vorgenommen).
- [x] T083 [US5] Legacy-Repo: README-Hinweis auf das neue Repo einfügen (PR oder direkter Commit nach dem Branch-Schutz des Legacy-Repos), dann archivieren (`gh repo archive haex-space/haex-vault-legacy`) — erledigt 2026-11-22: Hinweis am Anfang der README, die drei Verweise auf die neue Adresse (Releases, `git clone`, Tabelle) zeigen auf `haex-vault-legacy`; Repo danach wieder archiviert.
- [x] T084 [US5] **Bestätigung des Betreibers einholen**, dann `haexmas/holzi` nach `haex-space` übertragen (`gh api repos/haexmas/holzi/transfer -f new_owner=haex-space`), die Übertragung annehmen, dann zu `haex-vault` umbenennen — erledigt: `haexmas/holzi` und `haex-space/holzi` leiten auf `haex-space/haex-vault` weiter, lokales `origin` zeigt dorthin (vom Betreiber vorgenommen).
- [x] T085 [US5] Einstellungen im neuen Repo setzen (Liste aus T080): Default-Branch `main`, Branch-Schutz mit den Pflichtprüfungen (Documentation consistency, Frontend build, Rust checks (default), Rust checks (no-default), End-to-end suite), Secrets und Variablen (der Android-Signaturschlüssel, sobald er besteht), Dependabot, Auto-Merge — erledigt: Default-Branch `main`, Branch-Schutz, Secrets, Auto-Merge, Dependabot (siehe T080).
- [x] T086 [US5] PR „URLs“ im neuen Repo: `repository` in `src-tauri/Cargo.toml`, Flake, README, Abzeichen, Links in der Doku, und die vier Links auf die alte App auf `haex-space/haex-vault-legacy` umstellen (FR-021); Ausnahmeliste und Gate bleiben unverändert grün — erledigt: `repository` in `src-tauri/Cargo.toml` und im Android-Plugin, README-Links und Abzeichen zeigen auf `haex-space/haex-vault`.
- [x] T087 [US5] Lokale Ordner (FR-022): `~/Projekte/haex-vault` → `~/Projekte/haex-vault-legacy`, dann `~/Projekte/holzi` → `~/Projekte/haex-vault`; `git remote set-url origin https://github.com/haex-space/haex-vault.git`; `git worktree repair`; das Claude-Speicherverzeichnis des Projekts und `graphify-out` an den neuen Pfad hängen; alte Worktrees aufräumen — erledigt: `~/Projekte/holzi` heißt jetzt `~/Projekte/haex-vault` (Legacy liegt in `~/Projekte/haex-vault-legacy`), `origin` zeigt auf `haex-space/haex-vault`, die Worktrees sind repariert, das Claude-Speicherverzeichnis liegt unter dem neuen Pfad, `graphify-out` zog mit dem Ordner um; 13 gemergte Worktrees samt lokalen Branches sind entfernt.
- [x] T088 [US5] Prüfung nach dem Umzug: `https://github.com/haexmas/holzi` leitet weiter, PR #474, #476 und #480 sind im neuen Repo auffindbar, das Legacy-Repo ist archiviert und verweist auf das neue, ein Test-PR läuft durch alle Pflichtprüfungen und ein Push direkt auf `main` wird abgelehnt (Quickstart 8) — erledigt: Weiterleitung von `haexmas/holzi`, PR #474 im neuen Repo auffindbar, Legacy archiviert mit Hinweis, PRs #1109 und #1116 liefen durch die Pflichtprüfungen, Branch-Schutz gilt auch für Admins.

**Checkpoint (US5)**: Der Umzug ist abgeschlossen.

---

## Phase 8: Abschluss

- [x] T089 [P] Erfolgskriterien SC-001 bis SC-007 aus der Spec einzeln abhaken (Nachweis im jeweiligen PR oder in der Prüfung des Quickstarts) und `specs/054-haex-vault-rename/checklists/requirements.md` aktualisieren — Stand 2026-11-23, alle sieben belegt. SC-001 `python3 scripts/ci/check-old-name.py` liefert 0 (6957 Dateien, 277 erlaubt), das Scheitern eines Test-PR zeigt T063; **SC-002 belegt**: der vollständige `main`-Lauf [45088891620](https://github.com/haex-space/haex-vault/actions/runs/45088891620) für `558d1d6c` ist grün, einschließlich Desktop- und Android-E2E sowie Android-Build; einen iOS-Build gibt es in `.github/workflows/ci.yml` nicht und ist bis zur Apple-Lizenz bewusst zurückgestellt (Betreiber, 2026-11-23, iOS bleibt auf der Roadmap; SC-002 ist entsprechend angepasst); SC-003 T067/T068 (S9, PR #1099); SC-004 T035 und die Mehrgeräte-Szenarien im vollständigen `main`-E2E (grün in Lauf 45088891620); SC-005 T011 bis T014 (US4); SC-006 T088; SC-007 T071 bis T078: atoms, spaex, haex-crdt, vault-sdk und haextension haben auf `main` nur noch Treffer im CHANGELOG (Verlauf), CI grün (atoms hat keine CI).
- [x] T090 [P] Memory-Notizen des Betreibers (außerhalb des Repos) auf den neuen Namen und die neue Adresse umstellen: `holzi-worktree-setup`, `holzi-github-accounts` (Konto für `haex-space/haex-vault`), `holzi-cargo-in-worktrees`, `holzi-ci-runner-cancellations` u. a.; Hinweis auf die abgeschlossene Umbenennung und das Tag `holzi-final` — erledigt am 2026-11-22: `holzi-worktree-setup`, `holzi-github-accounts`, `holzi-cargo-in-worktrees`, `holzi-ci-runner-cancellations`, `holzi-supersedes-haex-vault`, `holzi-product-name-haexvault` und der Index umgestellt, neu `haex-vault-rename-054-done` und `spaex-verbatim-composition`.
- [x] T091 Alte Branches und Worktrees dieser Arbeit entfernen (`git worktree remove`, `git branch -d`, Remote-Branches der gemergten PRs); `holzi-final` bleibt als Rückfallpunkt der Quellen — erledigt am 2026-11-22: 13 Worktrees und lokale Branches entfernt, die gemergten Remote-Branches `054-haex-vault-rename`, `054-s10-pins`, `054-s9-result` und `spaex-verbatim` gelöscht; `054-squirrel-logo` (PR #474) bleibt, `holzi-final` bleibt; der Worktree dieser Notiz folgt nach dem Merge von #1117.
- [x] T092 Status der Spec auf „Implemented“ setzen und die Lage in einer kurzen Notiz (`specs/054-haex-vault-rename/spec.md`, Kopfzeile) festhalten; PR „docs(054): mark the rename as done“ — erledigt: Status „Implemented“ und Lage-Notiz in der Kopfzeile der Spec.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: keine Abhängigkeiten; T001 bis T006 vor allem anderen.
- **Phase 2 (Foundational, S1)**: braucht Phase 1. S1a (T007–T015) vor S1b (T016–T021), weil der Testvault auf dem Stand nach der Legacy-Markierung getestet wird. **Blockiert alle weiteren Stufen.**
- **Phase 3 (US3, S2–S5)**: braucht S1 (beide PRs gemergt). S2 → S3 → S4 → S5 strikt nacheinander, weil Crate-, Plugin- und Schema-Umbenennung dieselben Dateien berühren und der Bau sie braucht.
- **Phase 4 (US1, S6–S8)**: braucht S5. S6 → S7 → S8 nacheinander (das Gate prüft die Wirkung von S2–S7).
- **Phase 5 (US2, S9)**: braucht S8 (nur lesend; kann bereits nach S5 begonnen werden, schließt aber erst nach S8 ab) und löst den Rückfall T069 nur im Fehlerfall aus.
- **Phase 6 (US6, S10)**: braucht S9 (bestanden). T071 bis T075 in verschiedenen Repos, T073 und T075 vor T076, T071 und T072 vor T077 sowie T074 vor T075.
- **Phase 7 (US5, S11)**: braucht S10; T082 und T084 brauchen die ausdrückliche Bestätigung des Betreibers.
- **Phase 8**: nach S11.

### User Story Dependencies

- **US4 (S1a)** und der Testvault (S1b) sind Voraussetzung für alles.
- **US3 → US1 → US2 → US6 → US5** in dieser Reihenfolge; US2 darf früher geprüft, aber erst nach US1 abgeschlossen werden; US5 ist die letzte Stufe.

### Within Each Stage

- Test vor Code, wo ein Test neu ist (T032 vor T033, T058 vor T059).
- Regeln der Namenstabelle in der Reihenfolge Sonderfälle → Familien → Pfade → Rest; erzeugte Dateien neu erzeugen.
- Jede Stufe endet mit der Prüfung (`git grep`-Suche, Build, Tests) und einem PR; erst danach beginnt die nächste Stufe.
- Konflikte durch parallel gemergte Änderungen: Regeln erneut anwenden, nicht von Hand lösen.

## Parallel Opportunities

- **Phase 1**: T004 und T005 parallel zu T002/T003.
- **S1a**: T010, T011 und T014 laufen parallel zueinander und zu T007–T009 (verschiedene Dateien).
- **S1b**: T020 parallel zu T019.
- **S2**: T025 und T026 parallel zu T022–T024; T027 nach T026.
- **S4**: T038, T039 und T040 parallel (verschiedene Gebiete).
- **S5**: T045, T046 und T047 parallel zu T042–T044.
- **S6**: T050, T051 und T052 parallel.
- **S7**: T054 und T055 parallel.
- **S10**: T071, T072 und T073 parallel (andere Repos).
- **Phase 8**: T089 und T090 parallel.

```text
# Beispiel S1a — parallel starten:
T010 Testdateien und Testdaten umbenennen (src-tauri/tests/**)
T011 Beschriftung und Testlogik (src/i18n/locales/*.json, scripts/check-passwords-import.ts)
T014 Doku und Specs markieren (README.md, docs/, plans/, specs/**)
```

## Implementation Strategy

### MVP: Stufen S1 bis S5 (US3 mit der Grundlage aus S1)

1. Phase 1 und S1 (Legacy-Markierung und Testvault).
2. S2 bis S5: die App trägt den neuen Namen, baut, läuft und synchronisiert.
3. **Anhalten und prüfen**: Quickstart 4, 5, 6; CI grün auf `main`; der Testvault-Test grün oder die Abweichungen entschieden.
4. Erst danach Text (S6–S7), Gate (S8) und die Übernahme der echten Datei (S9).

### Incremental Delivery

- Jede Stufe ist ein PR, der für sich grün ist und den Betrieb nicht stört; `main` bleibt jederzeit lauffähig. Bricht etwas ab, geht es auf den Tag `holzi-final` zurück.
- Der Umzug des Repos (S11) kommt zuletzt, damit Einstellungen und Geheimnisse nur einmal unter dem endgültigen Namen eingerichtet werden.

## Notes

- Zeilenzahlen und Trefferzahlen stammen vom Stand `main` bei `3392de8f` und verschieben sich mit jeder Stufe; Prüfungen messen stets neu mit `git grep`.
- Commit-Nachrichten und PR-Beschreibungen enthalten keinen Hinweis auf einen Agenten (Constitution).
- Rust-Builds sind teuer (kalter Build, Speicher): lokal nur gezielte Ziele mit `-j 4`; vollständige Läufe macht die CI.
- Dieser Plan ändert keine Daten des Betreibers; Original und Sicherung der alten Datei bleiben in jedem Schritt unverändert.
