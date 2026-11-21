# Implementation Plan: Sitzung wiederherstellen (wählbar)

**Branch**: `022-session-restore` | **Date**: 2026-09-26 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/022-session-restore/spec.md`

## Summary

Welche Arbeitsbereiche, Fenster und Tabs offen sind, wird nur noch gespeichert,
wenn der Nutzer „Sitzung wiederherstellen“ eingeschaltet hat, für dieses Gerät
oder für die ganze Vault. Standard ist aus. Gespeicherte Sitzungen verlassen ihr
Gerät nie, und die ungefragt gespeicherten Sitzungen aus Spec 015 verschwinden
beim Update aus der Vault.

Technischer Ansatz (Begründungen in [research.md](./research.md)):

- **Gerätelokale Speicherung** (R1, R2): eine Zeile je Gerät in
  `wm_sessions_no_sync` mit der ganzen Sitzung als JSON. Die Endung `_no_sync`
  hält sie aus der Synchronisierung heraus; Löschen ist dort echtes Löschen.
  Das relationale Modell aus 015 (rund 2 000 Zeilen Rust) entfällt.
- **Einstellung als Präferenz** (R3): `wm.session_restore`, Gerät vor Vault,
  sonst aus; synchronisiert wie andere Einstellungen.
- **Vier Befehle** (R4) statt sechs; Setzen und Aufräumen atomar,
  Speichern prüft die Einstellung selbst.
- **Altdaten** (R5, R6): Migration 0020 löscht die drei Tabellen aus 0019 per
  `DROP TABLE` (keine Löschvermerke); beim Start wird der alte Präferenzschlüssel
  gelöscht, `secure_delete` eingeschaltet und einmal `VACUUM` ausgeführt.
- **Frontend** (R7, R8): Speicher-Warteschlange als `useWmSession`,
  Store speichert nur bei geltender Einstellung, neue Einstellungskomponente
  und zwei Aktionen im Katalog.

## Technical Context

**Language/Version**: Rust (Edition des Projekts, Tauri 2.11), TypeScript 6
(strict), Vue 3.5, Nuxt 4.5.2 (SPA), Node 22

**Primary Dependencies**: nur Vorhandenes — haex-crdt (Pin `ed230d2c3f58c1b10710b6025ea0ce6c20b8d009`),
rusqlite mit SQLCipher, serde_json, ts-rs, Pinia, haex-ui-Layer

**Storage**: SQLCipher-Vault über haex-crdt; neu `wm_sessions_no_sync` und
`haex_maintenance_no_sync` (beide nicht synchronisiert), Präferenz in
`preferences`; entfernt `workspaces`, `shell_windows`, `shell_window_tabs`

**Testing**: `cargo test` (neue Befehls- und Migrationstests), `check:wm-state`
(Warteschlange), Regression `check:wm-navigation`, `check:chat-state`,
`check:templates`, `typecheck`, `lint`, `format:check`, `lint:rust`; manuell
nach [quickstart.md](./quickstart.md)

**Target Platform**: Tauri-Desktop (Linux, macOS, Windows); Android ohne
Besonderheiten

**Project Type**: desktop-app (Nuxt-SPA + Rust-Backend in einem Tauri-Projekt)

**Performance Goals**: SC-006 — Öffnen nicht langsamer als bisher; einmalige
Bereinigung ≤ 1 s zusätzlich. Speichern entprellt (400 ms), ein Schreibvorgang
je Speicherung

**Constraints**: Dateien ≤ 500 Zeilen (`stores/windowManager.ts` steht bei 485;
der Umbau entfernt mehr, als er hinzufügt); Migrationen sind unveränderlich
(`MigrationContentDrift`), also nur neue Migration 0020; kein `DELETE` auf
CRDT-Tabellen in Migrationen (R5); Sitzung ≤ 4 MiB (mit Historien je Tab)

**Scale/Scope**: eine Zeile je Gerät, typischerweise wenige KB

## Constitution Check

_GATE: Muss vor Phase 0 bestehen. Nach Phase 1 erneut geprüft — Ergebnis unten._

Geprüft gegen `.specify/memory/constitution.md` (v1.4.0) und die
spaex-Constitution `.spaex/constitution.md`.

| Prinzip / Vorgabe                                                          | Status | Begründung                                                                                                                                                                                            |
| -------------------------------------------------------------------------- | ------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| I Keine Geheimnisse in Git                                                 | ✅     | Keine Geheimnisse berührt                                                                                                                                                                             |
| II Keine lokalen absoluten Pfade in versionierter Konfiguration            | ✅     | Nur repo-relative Pfade                                                                                                                                                                               |
| III Projektidentität geräteunabhängig                                      | ✅     | Berührt nicht                                                                                                                                                                                         |
| IV Cross-Repo-Referenzen an unveränderliche SHAs gepinnt                   | ✅     | haex-crdt `ed230d2c3f58c1b10710b6025ea0ce6c20b8d009` in Plan und Research                                                                                                                             |
| V Externe Quellen nur per Opt-in                                           | ✅     | Keine neue Quelle                                                                                                                                                                                     |
| VI Selbstverändernde Anweisungen review-pflichtig                          | ✅     | Keine Änderung an Constitution/Skills; `CONTEXT.md` läuft durch den PR                                                                                                                                |
| VII Relay-Ausfall blockiert lokale Arbeit nicht                            | ✅     | Rein lokal                                                                                                                                                                                            |
| VIII Keine Verheimlichung in Agent-Ausgaben                                | ✅     | –                                                                                                                                                                                                     |
| Workflow: speckit-Stufen, PR auf `main`, Conventional Commits, kein Squash | ✅     | specify → plan → tasks → implement; Topic-Branch im Worktree                                                                                                                                          |
| ADR bei prinzipienrelevanter Entscheidung                                  | ✅     | Keine; ADR-0001 (gerätebezogene Daten) bleibt gültig und wird angewendet                                                                                                                              |
| Test-Code in separaten Dateien                                             | ✅     | `wm_session_tests.rs`, `migrations_tests.rs`, `scripts/check-wm-persistence.ts`                                                                                                                       |
| Worktree je Änderung                                                       | ✅     | `.worktrees/022-session-restore`                                                                                                                                                                      |
| 500-LoC-Grenze                                                             | ✅     | Neue Dateien klein; `windowManager.ts` wird kürzer                                                                                                                                                    |
| Graphify vor neuen benannten Artefakten                                    | ✅     | Abfragen in R3 (Graph `055a411`, im Worktree unverändert): `preferences.rs` wird erweitert statt parallel gebaut; für die Sitzungsspeicherung kein Kandidat                                           |
| `ponytail:`-Kommentar bei bewusster Vereinfachung                          | ✅     | Geplant an der fehlenden Fremdschlüssel-Kaskade (R1) und an der Momentaufnahme statt einzelner Fenster (R2)                                                                                           |
| Nicht-triviale Logik hinterlässt einen ausführbaren Check                  | ✅     | Rust-Tests für Befehle und Migration; `check:wm-state` für die Warteschlange                                                                                                                          |
| Keine Selbstreferenzen von Agenten in Artefakten/Commits                   | ✅     | Wird bei Commits eingehalten                                                                                                                                                                          |
| **Phasen-Disziplin**                                                       | ✅     | Setzt Spec 015 voraus, die auf `main` im Einsatz ist. Der Branch stapelt nur wegen gemeinsamer Dateien auf 020; er nutzt keine Funktion aus 020, die noch nicht im Einsatz wäre. Merge erst nach #142 |

**Ergebnis vor Phase 0**: kein Verstoß.

**Ergebnis nach Phase 1**: unverändert. Das Design fügt keine Abhängigkeit
hinzu, entfernt mehr Code als es anlegt und bleibt bei vier Tauri-Befehlen.

## Project Structure

### Documentation (this feature)

```text
specs/022-session-restore/
├── plan.md                    # Dieses Dokument
├── research.md                # Phase 0 (R1–R9)
├── data-model.md              # Phase 1: Einstellung, WmSession, Tabellen, Altdaten
├── quickstart.md              # Phase 1: automatische und manuelle Validierung
├── contracts/
│   └── wm-session.md          # Befehle, Start/Wartung, Aktionen, Store, Einstellungsansicht
├── checklists/requirements.md # Spec-Qualitätscheckliste
└── tasks.md                   # Phase 2 — NICHT von /speckit-plan erzeugt
```

### Source Code (repository root)

Stand nach der Umsetzung (2026-09-26).

```text
src-tauri/src/
├── error.rs                       # NEU: HaexError::SessionTooLarge { bytes }
├── identity/
│   ├── migrations.rs              # NEU: 0020_wm_session_no_sync (DROP ×3, CREATE ×2, Wartungseintrag)
│   └── migrations_tests.rs        # NEU: 0020 frisch + aktualisiert; 0019-Kaskadentests entfallen
├── instances/
│   ├── open.rs                    # NEU: maintenance::run_after_open nach dem Öffnen
│   └── create.rs                  # NEU: maintenance::run_after_open für neue Vaults
├── storage/
│   ├── maintenance.rs             # NEU: secure_delete, Alt-Präferenz löschen, einmaliges VACUUM
│   ├── maintenance_tests.rs       # NEU
│   ├── preferences.rs             # ERWEITERT: parse_bool, ScopedBool, get_scoped_bool
│   ├── preferences_tests.rs       # ERWEITERT: Auflösung Gerät vor Vault
│   ├── wm_session.rs              # NEU: Tabelle wm_sessions_no_sync (load/save/delete je Gerät)
│   ├── wm_session_tests.rs        # NEU
│   ├── wm_session_commands.rs     # NEU: die vier Befehle, Wire-Typen (ts-rs), current_device_uuid
│   ├── wm_session_commands_tests.rs # NEU: inkl. Vault-Scope
│   ├── wm_commands.rs, wm_windows.rs, wm_workspaces.rs + *_tests.rs   # ENTFALLEN
│   └── mod.rs                     # Module anpassen
└── lib.rs                         # Befehlsregistrierung: 6 alte raus, 4 neue rein

src/
├── composables/
│   └── useWmLayout.ts → useWmSession.ts   # Warteschlange mit wm_session_save, Rückfall ohne Historien
├── lib/wm/
│   ├── session.ts                 # NEU: WmSession, snapshotSession, parseWmSession, splitSession, withoutHistories
│   ├── sessionSync.ts             # NEU: Speichern/Wiederherstellen, rein und unter Node testbar
│   └── types.ts                   # PersistedLayout bleibt Eingabe von hydrate (nur Kommentar)
├── stores/
│   ├── windowManager.ts           # delegiert an sessionSync: restoreSessionAsync, setSessionRestore, getSessionRestore
│   ├── wmLayoutHandlers.ts        # wm.workspace.create synchron
│   └── settingsActionHandlers.ts  # settings.sessionRestore.set/clear, settings.get
├── lib/actions/settingsActions.ts # zwei neue Aktionen
├── components/
│   ├── settings/SessionRestoreSetting.vue # NEU
│   └── apps/SettingsApp.vue       # Komponente einbinden
├── pages/workspace/[instance].vue # restoreSessionAsync mit Fehlerbehandlung vor ?open=
├── types/bindings/                # 5 alte DTOs raus; SessionRestoreState, SessionRestoreScope, SessionRestoreSetArgs, WmSessionLoad, WmSessionSaveArgs, WmSessionSaved rein
└── i18n/locales/{de,en}.json      # settings.sessionRestore.*, actions.settings.sessionRestore.*

scripts/check-wm-persistence.ts    # auf useWmSession umgestellt
scripts/check-wm-session.ts        # NEU: session.ts und sessionSync.ts (in check:wm-state)
specs/015-workspace-shell/spec.md  # Vermerke nach FR-016
specs/020-tab-navigation/spec.md   # Vermerk an FR-011
CONTEXT.md                         # Begriff „Sitzung (wm session)“, Abgrenzung zur Vault-Session
plans/README.md                    # Zeile 022
```

**Structure Decision**: Bestehende Tauri-Struktur. Rust-Speicherlogik unter
`src-tauri/src/storage/`, Frontend unter `src/` nach den Mustern von Spec 015
und 020.

## Anforderungen → Umsetzung

| Anforderungen                         | Umsetzung                                                                                                    |
| ------------------------------------- | ------------------------------------------------------------------------------------------------------------ |
| FR-001–003 (Einstellung, Scopes, aus) | `preferences.rs::get_scoped_bool`, `wm_session_restore_get`                                                  |
| FR-004 (Ansicht)                      | `SessionRestoreSetting.vue`                                                                                  |
| FR-005 (sofort wirksam)               | `wm_session_restore_set` über `setSessionRestore` (gleiche Warteschlange, speichert beim Einschalten sofort) |
| FR-006 (Aktion)                       | `settings.sessionRestore.set/clear`                                                                          |
| FR-007 (Ausschalten löscht)           | `wm_session_restore_set` löscht in derselben Transaktion                                                     |
| FR-008 (beim Öffnen aufräumen)        | `wm_session_load` löscht, wenn nichts gilt                                                                   |
| FR-009, FR-011 (Altdaten)             | Migration 0020 (`DROP TABLE`), `maintenance.rs` (Alt-Präferenz, `VACUUM`)                                    |
| FR-010 (nie an andere Geräte)         | `wm_sessions_no_sync`, `secure_delete`                                                                       |
| FR-012 (Fehler)                       | Protokollieren und weiterlaufen; Wartungseintrag bleibt für den nächsten Start                               |
| FR-013–015 (Start, Deep-Link)         | `sessionSync.restoreAsync` via `restoreSessionAsync`, Fehlerbehandlung in `pages/workspace/[instance].vue`   |
| FR-016 (Doku 015, 020)                | Vermerke in `specs/015-workspace-shell/spec.md` und `specs/020-tab-navigation/spec.md`                       |

## Complexity Tracking

Keine Einträge.
