# Implementation Plan: Onboarding-Härtung und Modellwahl-Persistenz

**Branch**: `002-onboarding-model-prefs` | **Date**: 2026-09-10 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `specs/002-onboarding-model-prefs/spec.md`

## Summary

Ein Nutzer bekommt beim ersten Öffnen einer Vault auf einem Gerät (Genesis oder Adoption via `cp`) einen geführten Onboarding-Wizard mit Gerätenamen-Pflichtfeld und optionaler Hardware-passender Modellwahl. Nach Abschluss landet er auf einer minimalen Workspace-Landing mit persistentem FAB unten rechts, über den der bestehende Chat aufgerufen wird. Ein minimaler Settings-Screen enthält "Modell als Standard setzen" (Scope: dieses Gerät ODER vault-weit) sowie den Alias-Editor. Modellwahl-Persistenz kommt über eine neue sync-tracked `preferences`-Tabelle mit Composite-PK `(vault_device_uuid, key)` und Hard-FK auf `known_devices` (siehe [ADR-0001](../../docs/adr/0001-device-scoped-data-convention.md)). Session-Resolver-Kette (last_active → default(device) → default(vault) → first-available → onboarding) gibt beim Chat-Aufruf über den FAB ein Modell zurück. Bestehende `device_downloaded_models_no_sync`-Tabelle wird zugunsten reiner Filesystem-Wahrheit unter `AppLocalData/models/` gelöscht.

## Technical Context

**Language/Version**: Rust 1.77+ (backend, `src-tauri`), TypeScript 5 (frontend, Nuxt 4.5 SPA)
**Primary Dependencies**: haex-crdt 0.4.0 (pinned rev `1c069ef`), Tauri 2.6, mistralrs 0.8.1 (llm-cpu/llm-cuda), reqwest 0.12, eventsource-stream 0.2, sysinfo 0.32 (für OS-Hostname-Detection wird bereits verwendet), vue-router (nuxt-native), @haex/ui-Layer (pinned rev `634d621`)
**Storage**: SQLite via SQLCipher durch haex-crdt; per-device Preferences als sync-tracked Composite-PK-Tabelle mit Hard-FK auf `known_devices(vault_device_uuid)`
**Testing**: `cargo test --lib` (unit + wiremock), `cargo test --test <name>` (integration mit bootstrap), `pnpm typecheck` (nuxt); Playwright ist nicht eingerichtet, UI-Tests bleiben manuell
**Target Platform**: Desktop (Linux primär, macOS/Windows/WSL2 mitgedacht per plan); Mobile bleibt Post-MVP
**Project Type**: Desktop application (Tauri: Rust backend als static lib + Nuxt 4 SPA-frontend als static bundle)
**Performance Goals**: Wizard-Interaktion < 30s (SC-001), Auto-Load bei warm cases < 3s (SC-006), CUDA-Kalt-Load ~30s mit erklärendem Text als akzeptabel (Etappe-0-Findung #4)
**Constraints**: Single-vault-per-process (bestehend, FR-022 in spec 001); haex-crdt-kompatible Schema-Änderungen (kein DROP von CRDT-Metadaten-Spalten, korrekte `haex_hlc_no_sync`-Injektion); offline-first (Onboarding funktioniert ohne Netz — nur Download braucht Netz)
**Scale/Scope**: Single-Nutzer, ~1-5 Geräte pro Vault, ~5-20 lokale Modelle, ~10 Anbietermodelle pro Provider, ~5 Anbieter maximal

## Constitution Check

_GATE: Must pass before Phase 0 research. Re-check after Phase 1 design._

Bewertung gegen haex-vault Constitution (`.specify/memory/constitution.md`, hard-pinned von haex-hive):

| Prinzip                                                 | Status | Begründung                                                                                                                                                                                                                                                                                                      |
| ------------------------------------------------------- | ------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| I. No Secrets in Git                                    | ✓ PASS | Feature führt `preferences`-Tabelle ein; keine Secrets. Keine Test-Fixture mit Credentials in Git.                                                                                                                                                                                                              |
| II. No Local Absolute Paths in Versioned Config         | ✓ PASS | Keine neuen Config-Files. Bestehende `AppLocalData`-Resolver-Pfade bleiben zur Laufzeit, nicht im Git-Zustand.                                                                                                                                                                                                  |
| III. Project Identity Is Device-Independent             | ✓ PASS | Feature bestätigt Prinzip explizit: `vault_device_uuid` (device-abhängig) ist getrennt von `vault_identity` (device-unabhängig). `preferences.scope`-Werte referenzieren nur ephemere Device-IDs, die per Adoption auf neuen Geräten frisch geminted werden. Keine Filesystem-Pfade in synchronisierten Zeilen. |
| IV. Cross-Repo References Pin Immutable Revisions       | ✓ PASS | Keine neuen Cross-Repo-Refs; haex-crdt bleibt auf `1c069ef` gepinnt.                                                                                                                                                                                                                                            |
| V. External Sources Are Opt-in Per Project              | ✓ PASS | N/A für dieses Feature (keine externen Harness-Content).                                                                                                                                                                                                                                                        |
| VI. Self-Modifying Instructions Are Always Review-Gated | ✓ PASS | Änderung an ADR-0001, CONTEXT.md und Plan geht durch PR-Review.                                                                                                                                                                                                                                                 |
| VII. Relay Unavailability Never Blocks Local Work       | ✓ PASS | Onboarding + Preferences funktionieren komplett offline (nur Modell-Download braucht Netz — bestehende Funktionalität, unverändert).                                                                                                                                                                            |
| VIII. No Concealment Instructions in Agent Output       | ✓ PASS | Kein Concealment-Content.                                                                                                                                                                                                                                                                                       |

**Result**: Alle Gates PASS. Keine Complexity Tracking-Einträge nötig.

## Project Structure

### Documentation (this feature)

```text
specs/002-onboarding-model-prefs/
├── plan.md                    # This file
├── spec.md                    # Feature specification (existing, from /speckit.specify + /speckit.clarify)
├── research.md                # Phase 0 output (from this command)
├── data-model.md              # Phase 1 output
├── quickstart.md              # Phase 1 output
├── contracts/                 # Phase 1 output
│   └── tauri-commands.md      # New/changed Tauri command shapes
├── checklists/
│   └── requirements.md        # Spec-quality checklist (from /speckit.specify)
└── tasks.md                   # Phase 2 output (/speckit.tasks — NOT this command)
```

### Source Code (repository root)

Dieses Feature erweitert die bestehende Tauri-Desktop-App-Struktur (kein Web-Backend, kein Mobile-Bereich in scope). Betroffene Verzeichnisse:

```text
src-tauri/src/
├── identity/
│   ├── migrations.rs             # + Migrations 0011 preferences, 0012 DROP device_downloaded_models_no_sync
│   ├── bootstrap.rs              # + Sentinel-Row INSERT OR IGNORE, + Alias-Nullability weiterhin
│   ├── mod.rs                    # + Export VAULT_SCOPE_UUID
│   └── (installation.rs, migrations.rs bleiben strukturell)
├── storage/
│   ├── preferences.rs            # + NEU: typed get/set/delete/query mit scope
│   ├── preferences_tests.rs      # + NEU
│   ├── known_devices.rs          # + list ohne Sentinel, + update_alias erweitert
│   ├── models.rs                 # unverändert
│   └── (device_downloaded_models.rs wird gelöscht — Filesystem ist Wahrheit)
├── models/
│   ├── paths.rs                  # + canonical_model_file für Loader und Discovery
│   └── commands.rs               # + list_installed_models scannt Filesystem statt DB
├── chat/
│   ├── commands.rs               # + resolve_default_model + Chain, + write last_active bei send_message
│   └── session.rs                # unverändert
├── catalog/
│   └── mod.rs                    # + recommend_tiers(&HardwareInfo) -> [entry; 3]
├── hardware/
│   ├── mod.rs                    # + hostname_placeholder() für Alias-Default
│   └── fit.rs                    # unverändert
├── device/                       # NEU (kleines Modul für Device-Info-Command)
│   └── commands.rs               # + current_device_info() Command
└── lib.rs                        # + neue Commands im invoke_handler

src/
├── pages/
│   ├── index.vue                 # Route-Guard-Erweiterung: nach Vault-Open → Landing statt Chat
│   ├── onboarding/
│   │   └── [instance].vue        # NEU: Alias + Modell-Wahl Wizard
│   ├── workspace/
│   │   └── [instance].vue        # NEU: Minimaler Landing-Stub mit Instanznamen + FAB
│   ├── settings/
│   │   └── [instance].vue        # NEU: Alias-Rename + "Als Standard setzen"-Toggle
│   └── chat/
│       └── [instance].vue        # angepasst: erreichbar per FAB von workspace/, Loading-UX-Meldungen präziser
├── composables/
│   ├── usePreferences.ts         # NEU: get/set/clear + resolve_default_model
│   ├── useDevice.ts              # NEU: current_device_info + update_device_alias
│   ├── useChat.ts                # angepasst: Loading-Label mit Kontext
│   ├── useCatalog.ts             # angepasst: recommend_tiers verfügbar
│   └── useHardware.ts            # angepasst: hostname_placeholder verfügbar
├── components/
│   ├── workspace/
│   │   └── ChatFab.vue           # NEU: persistenter FAB unten rechts
│   ├── onboarding/
│   │   ├── OnboardingWizard.vue  # NEU: Wizard-Root
│   │   ├── AliasStep.vue         # NEU
│   │   └── ModelChoiceStep.vue   # NEU: 3 Tier-Chips
│   └── settings/
│       ├── DefaultModelSetting.vue  # NEU
│       └── AliasSetting.vue         # NEU

src-tauri/tests/
├── preferences_roundtrip.rs      # NEU: Storage-Wrapper-Roundtrip + Sentinel + FK-Cascade
├── bootstrap.rs                  # bestehend, erweitert um Sentinel-Idempotenz-Test
└── local_inference.rs            # bestehend, unverändert (nur Types-Import)
```

**Structure Decision**: Diese Feature-Erweiterung folgt der bestehenden Tauri-Desktop-App-Struktur ohne neue Top-Level-Verzeichnisse. Backend-Erweiterungen bleiben in `src-tauri/src/` unter den bereits eingeführten Modulen (`identity/`, `storage/`, `chat/`, `catalog/`, `hardware/`) plus ein neues kleines `device/`-Modul für den `current_device_info`-Command. Frontend-Erweiterungen führen drei neue Routen-Dateien und ihre Support-Komponenten ein; bestehende `chat/[instance].vue` bleibt funktional unverändert, wird aber vom FAB der Workspace-Landing statt direkt als Startroute aufgerufen.

## Complexity Tracking

Keine Constitution-Verletzungen. Keine Einträge.
