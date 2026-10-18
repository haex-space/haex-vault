# Settings: "Allgemein" with "Grundeinstellung" and "Erscheinungsbild"

**Status**: Design agreed with the operator on 2026-10-07; specified and planned in
[`specs/042-settings-general-restructure/`](../../specs/042-settings-general-restructure/).
Follows the haex-vault 2.x (Legacy) settings layout (`components/haex/system/settings/general/{basic,appearance}.vue`)
as part of the ongoing alignment with haex-vault 2.x's look.

## Decisions

| #   | Question                                     | Decision                                                                                                                                             |
| --- | -------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | What happens to the category "Darstellung"?  | It leaves the sidebar and becomes "Allgemein → Erscheinungsbild"; every existing appearance control moves along.                                     |
| 2   | Where do device name and session restore go? | "Grundeinstellung".                                                                                                                                  |
| 3   | Where is the language stored?                | As a vault preference (synced). The device itself remembers nothing.                                                                                 |
| 4   | Language before a vault is open?             | The system language if it is `de`/`en`, otherwise English. The start screen offers a language picker whose choice lasts only until the app restarts. |
| 5   | Start-screen choice vs. vault language?      | The vault's language wins on unlock. A vault without a stored language (new or existing) stores the currently active one.                            |
| 6   | Vault name?                                  | Dropped: no screen would show it. Can be added later as one more vault preference key.                                                               |
| 7   | Changing the vault password?                 | Current password, new password, repetition. Applies to this device only.                                                                             |
| 8   | Workspace background?                        | One image for all workspaces, stored as a vault preference (synced), with a remove action.                                                           |
| 9   | Backward compatibility?                      | None: holzi has no users yet. Old paths and keys are replaced, not aliased.                                                                          |
| 10  | Agent access to the password?                | An agent can never change it, but it can open the password view (`wm.app.open` with `at`).                                                           |

## Structure

The sidebar loses `appearance`. "Allgemein" (`/`) becomes an overview (the existing `OverviewView`)
with two rows:

| Location                 | Path                      | Contents                                                                                   |
| ------------------------ | ------------------------- | ------------------------------------------------------------------------------------------ |
| `general.basic`          | `/general/basic`          | Language, vault password (row → sub-view), device name, session restore                    |
| `general.basic.password` | `/general/basic/password` | Current / new / repeat, "this device only" hint                                            |
| `general.appearance`     | `/general/appearance`     | Color scheme, workspace background, accent and tint rows, window hint, import/export/reset |

The settings search `settingKeys` move with their controls. Existing E2E settings and appearance
scenarios get the new paths.

## Language

- **Before unlock**: `navigator.language` starting with `de` → German, anything else → English.
  `defaultLocale` becomes `en`. Nothing is persisted, so the next start detects again. Check
  whether `@nuxtjs/i18n` `detectBrowserLanguage` without a cookie already does this before writing
  a plugin.
- **Start screen**: a compact language picker on the onboarding screen; it calls `setLocale` only.
- **Vault**: new key `general.language` in `VAULT_PREFERENCE_KEYS`. After unlock or creation:
  stored → `setLocale`; missing → write the active locale. A change synced from another device
  applies the same way the color scheme does today.
- **Action**: `settings.general.setLanguage` (`effect: 'write'`).

## Vault password

Ported from haex-vault 2.x (`src-tauri/src/database/maintenance.rs`, `change_vault_password`):

1. Verify the current passphrase by opening a second, read-only connection with it (holzi does not
   keep the passphrase in memory; pattern: `passwords/import/legacy_haex_vault/open.rs`).
2. Under `Database::with_connection` (haex-crdt holds one connection behind a mutex, so sync and
   every other access wait): `wal_checkpoint(TRUNCATE)` → `journal_mode=DELETE` (SQLCipher `rekey`
   does not work in WAL mode) → `PRAGMA rekey` → `journal_mode=WAL`.

The new passphrase must be at least `MIN_PASSPHRASE_LEN`. Each linked device has its own
passphrase, and the passphrase plays no part in sync, so haex-vault 2.x's "re-encrypt the vault key on
sync backends" step has no counterpart here.

The Tauri command `change_vault_passphrase` is **not** registered in the action registry, so it
never becomes an agent tool.

## Workspace background

- Vault preference `appearance.background`: a WebP data URL, or absent.
- "Choose image…" via a native file input and "Remove" (only when set).
- The frontend scales the image to at most 2560 px on the long edge and encodes it as WebP at
  quality 0.8 (expected 200–600 KB).
- `wm/Desktop.vue` renders it with `background-size: cover` behind every workspace; without one it
  stays `bg-muted/10`.
- The user chooses the file in the view; the only background action is
  `settings.appearance.removeBackground`.
- The existing sync page budget carries the expected ~500-KB value as one preference; T037 adds a
  two-device regression check for that limit, so no separate CRDT image table is needed.
