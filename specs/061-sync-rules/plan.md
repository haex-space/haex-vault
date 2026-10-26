# Implementation Plan: Sync-Regeln

**Branch**: `061-sync-rules` | **Date**: 2026-10-26 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/061-sync-rules/spec.md`

## Summary

Eine Sync-Regel verbindet zwei Orte des Dateibrowsers (lokal, eigenes Gerät, S3, WebDAV, SFTP, verschlüsselter
Ordner) einseitig oder beidseitig und läuft auf genau dem Gerät, auf dem sie angelegt wurde. Ein neues Modul
`file_sync` listet beide Seiten, vergleicht sie mit dem gemerkten **Stand** der Regel, bildet daraus einen
**Plan** (reine Logik) und führt ihn aus. Kopieren geht über die vorhandene Übertragungsschicht
(`files::transfer`); Listen, Prüfen, Verschieben und Löschen über eine dünne eigene Schicht `Place`. Regeln,
Stand, Läufe und anstehende Löschungen liegen in Tabellen `*_no_sync` der Vault-Datenbank, je Gerät getrennt.

Technischer Ansatz (Begründungen und verworfene Alternativen in [research.md](./research.md)):

- **Orte** (R1): `Place` mit zwei Ausprägungen, Gerät (Dateisystem) und Speicher (`StorageFiles`, auch für
  Geräte der Vault und verschlüsselte Ordner). Es gibt keine gemeinsame Schnittstelle über alle Arten, und
  der `ObjectStore` taugt nicht für lokale Ordner (`put` nimmt den ganzen Inhalt).
- **Kopieren** (R2): `files::transfer` (`local::run`, `remote::run_remote`) pro Datei, Konfliktfrage
  beantwortet der Plan, nicht der Nutzer.
- **Plan und Stand** (R3, R4): `plan::build(state, left, right, rule)` ist rein; der Stand hält je Datei Größe
  und Zeit beider Seiten nach dem letzten Erfolg und wird erst nach dem Schritt fortgeschrieben.
- **Prüfsumme** (R5): zur Kontrolle nach dem Kopieren und überall dort, wo eine Verschiebung nicht bereits über
  eine verlässliche Dateiidentität bestätigt ist; bei mehreren gleich großen und gleich datierten Kandidaten zur
  Disambiguierung. Streaming-SHA-256, bei verschlüsselten Ordnern aus der Sidecar-Datei.
- **Ausnahmen** (R6): Crate `ignore` (nur `GitignoreBuilder`), Vereinigung beider Ausnahmedateien.
- **Speicher** (R7): neue Migration mit Tabellen `*_no_sync` und `vault_device_uuid` (ADR 0018 begründet die
  Abweichung von ADR 0001); Protokolltiefe als Einstellung des Geräts.
- **Ablauf** (R8, R9): Dienst `file_sync` startet beim Öffnen der Vault in `open.rs` und `import.rs`; Wächter
  `notify` für lokale Orte, Intervall für entfernte; ein Lauf je Regel, Läufe verschiedener Regeln nebenläufig
  mit Obergrenze.
- **Papierkorb** (R10): Systempapierkorb nur bei lokalen Orten auf dem Desktop, sonst `.holzi-trash`.
- **Agent** (R11): neue `AgentFileKind::SyncRules`, Aktionen `syncRules.*` mit eigener Abfrage je Änderung.
- **Oberfläche** (R12): Eintrag in der Seitenleiste, `FilesSyncRules`-Seite, `FilesFolderPicker` (neu),
  Ausnahmen-Editor mit `UiTextarea`, Einstellungen „Sync-Protokoll“.

## Technical Context

**Language/Version**: Rust (Tauri 2, Edition wie `src-tauri/Cargo.toml`); TypeScript (strict), Vue 3, Nuxt 4 (SPA)

**Primary Dependencies**: neu `ignore` (0.4.x, nur `gitignore::GitignoreBuilder`; Version bei der Umsetzung mit
`cargo add` prüfen); vorhanden `notify` 8 + `notify-debouncer-full` 0.5, `trash` 5.2 (Desktop), `sha2`, `walkdir`,
`tokio`, `tokio-util`, `ts-rs` 12, `vue-i18n`, haex-ui

**Storage**: Vault-Datenbank (haex-crdt), neue Migration `0044_sync_rules` mit fünf Tabellen `*_no_sync`
(keine Trigger-Version-Erhöhung, kein Sync); Einstellung `syncRules.logDepth` als `PrefScope::Device`. Die
Laufdateien wachsen mit Protokolltiefe mal Dateien je Regel, ohne künstliche Eintragsgrenze pro Lauf.

**Testing**: gezieltes `cargo test -j 4 --lib file_sync::` und `identity::migrations_sync_rules`;
`node --test` (`pnpm check:files`, `check:settings`, `check:agent-actions`); E2E `files-sync-rules`;
Rust-Tests in `*_tests.rs` neben dem Modul

**Target Platform**: Linux, macOS, Windows, Android (Dateiwächter nicht auf iOS; iOS ist kein Ziel von holzi
für Dateien)

**Project Type**: Tauri-Desktop-/Mobil-App (Rust-Backend, Nuxt-Frontend)

**Performance Goals**: Abgleich eines unveränderten lokalen Ordners mit 10 000 Dateien < 5 s (SC-007); Vorschau
ohne Kopieren; Oberfläche bleibt bedienbar während eines Laufs

**Constraints**: nie eine halbe Datei als fertig (FR-014); leerer oder fehlender Ort löscht nie (FR-022);
Stand nur nach vollständigem Schritt (FR-015); Zugangsdaten verlassen das ausführende Gerät nicht (FR-029);
Läufe brechen mit der Sperre der Vault ab (FR-026)

**Scale/Scope**: bis 100 000 Dateien je Regel; ein Dutzend Regeln je Gerät; Protokoll 0 bis 100 Läufe je Regel

## Constitution Check

_GATE: vor Phase 0 und nach Phase 1 geprüft._

- I Keine Geheimnisse in Git: Testdaten sind erzeugt; keine Schlüssel oder Zugangsdaten in Spec, Plan oder
  Vorlagen.
- II Keine lokalen Pfade: Spec, Plan und Verträge nennen nur Pfade im Repo; Beispielpfade in Quickstart und
  Tests sind relativ oder Platzhalter (`<Ordner>`); Testordner entstehen zur Laufzeit.
- III–V Identität und Quellen: nicht berührt; das neue Crate `ignore` ist eine gewöhnliche Abhängigkeit mit
  Version in `Cargo.toml`.
- VI Selbstverändernde Anweisungen: keine; die Agent-Berechtigung „Sync-Regeln verwalten“ ist Nutzerdaten, nie
  vom Agent gesetzt (`set` nur über die Einstellungen).
- VII Relay-Ausfall blockiert lokale Arbeit nicht: Regeln zwischen lokalen Orten brauchen kein Netz; ein
  nicht erreichbarer Ort setzt nur diese Regel auf „wartet“.
- VIII Keine Verheimlichung: Der Agent legt Regeln nie ohne Abfrage an, ändert oder löscht sie nie ohne Abfrage.
- Konventionelle Commits, PR nach `main`, kein Squash: Arbeit in `.worktrees/061-sync-rules`, Commits auf
  Englisch, ohne Claude-Zusätze (Vorgabe des Betreibers).
- ADR für Entwurfsbeschlüsse: Tabellen `*_no_sync` mit `vault_device_uuid` für geräteeigene Daten, die nicht
  synchronisiert werden dürfen, weicht von ADR 0001 ab (dort: nachverfolgte Tabellen). **ADR 0018
  „Sync-Regeln und ihr Zustand bleiben auf dem ausführenden Gerät“** im selben PR (Aufgabe in `tasks.md`).
- Eine Umsetzung je Regel: Vergleich nur in `plan::build`; Muster nur in `ignore::Matcher`; Löschmodus nur in
  `execute::delete_step`; Überlappungsprüfung nur in `rules::validate`.
- Jedes Gerät und jedes System: Rust-Zweige mit `cfg` (Trash, Wächter) auch für `aarch64-linux-android` linten.

Nach Phase 1: bestanden, mit dem ADR. Keine Komplexitätsabweichung außer der eigenen `Place`-Schicht
(siehe unten).

## Project Structure

### Documentation (this feature)

```text
specs/061-sync-rules/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── tauri-commands.md     # Befehle, Ereignisse, Typen
│   └── ignore-file.md        # Format und Auswertung von .haex-ignore
├── checklists/requirements.md
└── tasks.md                  # durch /speckit-tasks
```

### Source Code (repository root)

```text
src-tauri/src/
├── file_sync/                          # neu
│   ├── mod.rs                          # Dienst, Zustand (FileSyncState), start_for_active_instance
│   ├── model.rs                        # Rule, Direction, DeleteMode, RuleStatus, PlaceRef
│   ├── rules.rs                        # validate (Überlappung, Orte, Intervall), Anlegen/Ändern/Löschen
│   ├── store.rs                        # Tabellen: Regeln, Stand, anstehende Löschungen
│   ├── place.rs                        # Place: list_tree, stat, hash, rename, delete, trash, read_text, write_text
│   ├── place_device.rs                 # Ausprägung Gerät (Dateisystem, Papierkorb des Systems)
│   ├── place_storage.rs                # Ausprägung Speicher (StorageFiles, Peer, verschlüsselt)
│   ├── scan.rs                         # Listen beider Seiten zu Facts (Pfad, Größe, Zeit, Flags)
│   ├── ignore.rs                       # Matcher aus .haex-ignore (beide Seiten), feste Ausnahmen
│   ├── plan.rs                         # build: Stand + Seiten -> Plan (rein)
│   ├── execute.rs                      # Schritte ausführen, Stand fortschreiben, Konfliktkopien
│   ├── run.rs                          # ein Lauf: scan, plan, execute, Protokoll, Statuswechsel
│   ├── schedule.rs                     # Steuerung: Intervall, Wächter, „jetzt“, Obergrenze, Sperre
│   ├── watch.rs                        # notify-Wächter für lokale Orte
│   ├── log.rs                          # Lauf-Protokoll schreiben, kürzen, lesen
│   ├── status.rs                       # RuleStatus ableiten, Ereignis sync-rules-changed
│   ├── commands.rs                     # Tauri-Befehle
│   ├── agent.rs                        # SyncRulesAgent (NativeExecutor)
│   ├── agent_confirm.rs                # SyncRuleConfirm (PromptPark), Ereignis, Befehl
│   └── *_tests.rs
├── files/
│   ├── permissions.rs                  # AgentFileKind::SyncRules
│   ├── access.rs                       # AgentGrants.sync_rules
│   └── transfer/mod.rs, remote.rs      # file_sync braucht einen schmalen Eintritt (siehe R2)
├── identity/
│   ├── migrations.rs                   # 0044 anmelden
│   ├── migrations_sync_rules.rs        # neu
│   └── migrations_sync_rules_tests.rs
├── storage/preferences.rs              # Schlüssel syncRules.logDepth: Wertprüfung
├── instances/open.rs, import.rs        # file_sync::start_for_active_instance an beiden Stellen
└── lib.rs                              # Befehle, SyncRulesAgent, FileSyncState

src/
├── components/files/
│   ├── SyncRules.vue                   # Liste, Status, Aktionen
│   ├── SyncRuleDialog.vue              # Anlegen und Ändern
│   ├── SyncRuleIgnoreEditor.vue        # Ausnahmen (UiTextarea)
│   ├── SyncRulePlan.vue                # Vorschau
│   ├── SyncRuleLog.vue                 # Lauf-Protokoll
│   ├── SyncRulePending.vue             # Löschungen und Konflikte bestätigen
│   ├── FolderPicker.vue                # Quelle + Ordner wählen (neu)
│   └── Sidebar.vue                     # Eintrag „Sync-Regeln“
├── components/settings/general/SyncLogView.vue   # Protokolltiefe
├── components/settings/AgentFilesSetting.vue      # Schalter „Sync-Regeln verwalten“
├── components/wm/appRoutes.ts          # Route sync-rules, Einstellungen general/sync-log
├── composables/useSyncRules.ts         # invoke-Aufrufe
├── stores/syncRules.ts                 # Zustand, Ereignis sync-rules-changed
├── lib/files/registry.ts               # Ort „sync-rules“
├── lib/files/syncRules.ts              # reine Helfer (Status, Beschriftung)
├── lib/actions/syncRuleActions.ts      # syncRules.* für den Agent
├── lib/files/agentPermissions.ts       # syncRules
├── lib/settings/registry.ts            # general.syncLog
├── i18n/locales/{de,en}.json
└── types/bindings/                     # ts-rs (SyncRule…)

scripts/check-files-sync-rules.ts                      # neu
scripts/check-settings.ts                              # general.syncLog
scripts/e2e/scenarios/files-sync-rules.test.ts         # neu
docs/adr/0018-sync-rules-state-stays-on-the-device.md  # neu
docs/…, specs/…                                        # Verweise auf 025 ersetzen, 025 löschen
```

**Structure Decision**: ein neues Rust-Modul `file_sync` neben `files` (der Dateibrowser kennt Regeln nicht, das
Modul kennt nur seine Schnittstellen); im Frontend ein Bereich im Dateibrowser plus ein Menüpunkt der
Einstellungen.

## Complexity Tracking

| Abweichung                                         | Warum nötig                                                                                                                                                  | Einfachere Alternative verworfen, weil                                                                                                                           |
| -------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Eigene `Place`-Schicht (zwei Ausprägungen)         | Es gibt keine gemeinsame Schnittstelle über Gerät, Speicher, Peer und verschlüsselte Ordner; der Motor braucht Listen, Stat, Prüfsumme, Verschieben, Löschen | Alles als `ObjectStore` zu behandeln scheitert an `put(Vec<u8>)` für große lokale Dateien und liefert für Geräteordner nur den Umweg über `Protocol::Local` (R1) |
| Tabellen `*_no_sync` statt nachverfolgter Tabellen | Spec FR-003 und FR-045: Regeln und Protokoll dürfen nicht synchronisiert werden; je Datei ein Stand wäre Synclast                                            | ADR 0001 verlangt nachverfolgte Tabellen für geräteeigene Daten; das würde den Stand je Datei auf jedes Gerät schicken (R7, ADR 0018)                            |
