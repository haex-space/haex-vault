# Implementation Plan: Allgemein mit Grundeinstellung und Erscheinungsbild

**Branch**: `feat/settings-general-restructure` | **Date**: 2026-10-07 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/042-settings-general-restructure/spec.md`

## Summary

Die Kategorie „Darstellung“ geht in „Allgemein → Erscheinungsbild“ auf; „Allgemein“ wird eine Übersicht
mit „Grundeinstellung“ (Sprache, Vaultpasswort, Gerätename, Sitzung wiederherstellen) und
„Erscheinungsbild“ (Farbschema, Workspace-Hintergrund, bisherige Darstellungs-Regler). Neu sind eine
synchronisierte Vault-Sprache mit Systemsprachen-Start und Wahl auf dem Startbildschirm, die Änderung des
gerätelokalen Vault-Passworts per SQLCipher-Rekey und ein synchronisiertes Hintergrundbild.

Technischer Ansatz (Begründungen in [research.md](./research.md)):

- **Struktur** (R1): nur Registry-Einträge, vorhandene `OverviewView`/`SettingsRow`; `subView` bekommt
  `settingKeys`.
- **Sprache** (R2, R3): T001 bestätigt zuerst, ob `@nuxtjs/i18n` die Systemsprache ohne Persistenz
  erkennt; andernfalls übernimmt eine reine Funktion plus Plugin diese Aufgabe. Die Vault-Präferenz
  `general.language` wird über neues `useLanguage` nach dem Muster `useColorScheme` gelesen und geschrieben.
- **Passwort** (R4, R5): Command `change_vault_passphrase`, Ablauf aus haex-vault 2.x (Legacy) (Checkpoint → DELETE →
  rekey → WAL), Prüfung des aktuellen Passworts über eine zweite, nur lesende Connection; nicht im
  Action-Katalog.
- **Hintergrund** (R6–R8): Vault-Präferenz `appearance.background` als WebP-Data-URL, nativer File-Input,
  Verkleinerung im Frontend über einen mit den Passwort-Thumbnails geteilten Helper; Agenten können ihn
  nur entfernen oder die Ansicht öffnen.

## Technical Context

**Language/Version**: Rust (Tauri 2, Edition laut `src-tauri/Cargo.toml`); TypeScript (strict), Vue 3.5,
Nuxt 4 (SPA)

**Primary Dependencies**: vorhanden — `@nuxtjs/i18n` 10.6.0, haex-crdt (`raw-connection`), rusqlite
(bundled SQLCipher), haex-ui-Layer; keine neue Abhängigkeit, keine neue Tauri-Berechtigung

**Storage**: zwei neue Schlüssel in der synchronisierten `preferences`-Tabelle (Vault-Scope); keine Migration
([data-model.md](./data-model.md))

**Testing**: `cargo test` (Integrationstest `tests/vault_passphrase_change.rs`, `passphrase_tests.rs`),
`pnpm check:settings` (`node --test`), `pnpm typecheck`, `pnpm lint`, `pnpm format:check`, E2E
`scripts/e2e/scenarios/` (nur Arch)

**Target Platform**: Linux, macOS, Windows, Android, iOS

**Project Type**: Desktop- und Mobil-App (Tauri)

**Performance Goals**: Sprachwechsel ohne spürbare Verzögerung; Rekey einer typischen Vault in wenigen
Sekunden (skaliert linear mit der Dateigröße, blockiert in der Zeit alle DB-Zugriffe)

**Constraints**: offline-fähig; Passwort nie geloggt (`Passphrase`-`Debug` ist redacted, keine `println!`
wie in haex-vault 2.x); Hintergrund ≤ 4 MiB als Präferenzwert

**Scale/Scope**: 4 neue/umgebaute Settings-Locations, 1 Command, 2 Aktionen, 2 Präferenzschlüssel,
~6 E2E-Szenarien anzupassen

## Constitution Check

_GATE: Must pass before Phase 0 research. Re-check after Phase 1 design._

`.specify/memory/constitution.md` (I–VIII) und `.spaex/constitution.md`:

| Prinzip                                                      | Bewertung                                                                                                                                                                      |
| ------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| I No secrets in git                                          | ✅ Passwörter nur zur Laufzeit; Tests nutzen Wegwerf-Passwörter in Tempdirs                                                                                                    |
| II No local absolute paths                                   | ✅ keine                                                                                                                                                                       |
| III–V Identity / pinned refs / opt-in                        | ✅ nicht berührt; haex-vault 2.x nur als Vorbild zitiert, kein Code-Import                                                                                                     |
| VI Self-modifying instructions                               | ✅ keine Agent-Konfiguration geändert                                                                                                                                          |
| VII Relay unavailability                                     | ✅ alles lokal; Sync nur für Präferenzen wie bisher                                                                                                                            |
| VIII No concealment                                          | ✅                                                                                                                                                                             |
| spaex: speckit-workflow-adherence                            | ✅ specify → plan → tasks → implement                                                                                                                                          |
| spaex: pr-required-for-main, conventional commits, no squash | ✅ Topic-Branch, PR                                                                                                                                                            |
| spaex: graphify-first-authoring                              | ✅ Kandidaten geprüft: `useColorScheme` (Muster für `useLanguage`), `scaledUrl` (Extraktion R7, Operator-Freigabe mit diesem Plan), MIN_PASSPHRASE-Duplikat (→ `validate_new`) |
| spaex: ponytail                                              | ✅ zwei `ponytail:`-Kommentare (Rekey-Key im SQL-Text, Präferenz-Refresh liest ganzen Wert)                                                                                    |
| spaex: phasing-discipline                                    | ✅ Teil der laufenden haex-vault-2.x-Angleichung                                                                                                                               |
| ADR nötig?                                                   | Nein — keine Core-Principle-Änderung                                                                                                                                           |

**Post-Design Re-Check**: ✅ bestanden mit einer dokumentierten Abweichung vom Entwurf: kein
`setBackground(pfad)` für Agenten (R8); FR-019 wurde entsprechend angepasst.

## Project Structure

### Documentation (this feature)

```text
specs/042-settings-general-restructure/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/settings-contract.md
├── checklists/requirements.md
└── tasks.md             # /speckit-tasks
```

### Source Code (repository root)

```text
nuxt.config.ts                                   # i18n: defaultLocale en, detectBrowserLanguage
src/
├── lib/settings/registry.ts                     # Kategorien/Locations, subView settingKeys
├── lib/settings/language.ts                     # LANGUAGE_KEY, parseLanguage (+ systemLocale falls nötig)
├── lib/settings/background.ts                   # BACKGROUND_KEY, isBackgroundValue
├── lib/images/downscale.ts                      # downscaleToWebp (aus usePasswordsThumbnails extrahiert)
├── composables/useLanguage.ts                   # neu
├── composables/useWorkspaceBackground.ts        # neu
├── composables/usePasswordsThumbnails.ts        # nutzt downscaleToWebp
├── composables/useInstance.ts                   # changePassphraseAsync
├── components/wm/appRoutes.ts                   # SETTINGS_VIEWS für neue IDs
├── components/wm/Desktop.vue                    # Hintergrundbild
├── components/settings/
│   ├── GeneralView.vue → BasicView.vue          # Grundeinstellung
│   ├── LanguageSetting.vue                      # neu
│   ├── PasswordChangeView.vue                   # neu
│   ├── BackgroundSetting.vue                    # neu
│   └── AppearanceView.vue                       # + ColorScheme oben, Background
├── pages/index.vue                              # Sprachwahl, useLanguage nach Unlock/Create
├── pages/workspace/[instance].vue               # load/refresh Sprache und Hintergrund
├── lib/actions/settingsActions.ts               # setLanguage, removeBackground, settings.get
├── stores/settingsActionHandlers.ts             # Handler
└── i18n/locales/{de,en}.json
src-tauri/
├── src/instances/passphrase.rs                  # validate_new
├── src/instances/passphrase_change.rs           # neu: change_vault_passphrase
├── src/instances/{create,link_vault}.rs         # validate_new statt Duplikat
├── src/instances/mod.rs, src/lib.rs             # Registrierung
├── src/storage/preferences_commands.rs          # validate_value für appearance.background
└── tests/vault_passphrase_change.rs             # neu
scripts/
├── check-settings.ts
└── e2e/scenarios/{settings-categories,settings-color-scheme,appearance-basic,settings-narrow-window,settings-search,settings-deep-links}.test.ts
```

**Structure Decision**: bestehende Tauri-/Nuxt-Struktur; neue Dateien nur, wo kein Kandidat existiert.

## Complexity Tracking

Keine Verstöße.
