# Onboarding Tour Design

**Date:** 2026-03-13
**Status:** Approved

## Overview

Replace the existing welcome dialog with a Driver.js guided tour that navigates the user through the Settings UI. The tour teaches navigation structure while guiding through the required setup steps.

## Trigger

Same condition as the old welcome dialog: vault has no device name set. Tour starts once on first vault open, never shown again after completion or explicit abort.

## Library

**Driver.js** — lightweight, no framework dependency, works with Vue/Nuxt/Tauri.

```
pnpm add driver.js
```

## Tour Steps (Driver.js)

8 steps total — each logical step has a sidebar highlight followed by a content highlight.

| # | Element (`data-tour`) | Popover | Side effect on Next |
|---|---|---|---|
| 1 | `settings-nav-general` | "Hier findest du allgemeine Einstellungen" | navigate → `general` |
| 2 | `settings-device-name` | "Gib diesem Gerät einen Namen" | navigate → `extensions` tab |
| 3 | `settings-nav-extensions` | "Hier verwaltest du deine Erweiterungen" | navigate → `extensions` |
| 4 | `settings-extensions-install` | "Installiere Erweiterungen aus dem Marketplace" | navigate → `identities` tab |
| 5 | `settings-nav-identities` | "Hier verwaltest du deine Identitäten" | navigate → `identities` |
| 6 | `settings-identities-create` | "Erstelle deine erste Identität für die verschlüsselte Synchronisation" | navigate → `sync` tab |
| 7 | `settings-nav-sync` | "Hier richtest du die Synchronisation ein" | navigate → `sync` |
| 8 | `settings-sync-add-backend` | "Verbinde einen Sync-Server, um deine Vault zu synchronisieren" | complete tour |

## Architecture

### `useTourStore` (Pinia)

```ts
interface TourState {
  isCompleted: boolean  // persisted to localStorage — tour never shown again
}
```

- `start()` — sets Driver.js running, opens system settings window
- `complete()` — marks `isCompleted = true`, persists
- `abort()` — same as complete (user explicitly quit)

### `useTour` Composable

Wraps Driver.js. Responsible for:
- Defining all 8 steps with selectors and popover content (i18n strings)
- `onNextClick` callbacks that update `activeCategory` in settings before Driver.js moves to next step
- Calling `tourStore.complete()` on `onDestroyed`

### Settings Integration

`settings/index.vue` exposes `activeCategory` as a writable ref. The tour composable imports the settings store/ref to switch tabs programmatically.

The system settings window must be open and visible before the tour starts. The tour trigger in `app.vue` (or vault layout) opens the system window then calls `useTour().start()` after a short `nextTick` to ensure elements are mounted.

### `data-tour` Attributes

Added to the following elements:

| Attribute | Component | Element |
|---|---|---|
| `settings-nav-general` | `settings/index.vue` | General nav button |
| `settings-nav-extensions` | `settings/index.vue` | Extensions nav button |
| `settings-nav-identities` | `settings/index.vue` | Identities nav button |
| `settings-nav-sync` | `settings/index.vue` | Sync nav button |
| `settings-device-name` | `settings/general.vue` | Device name UiInput |
| `settings-extensions-install` | `settings/extensions.vue` | Install/browse area |
| `settings-identities-create` | `settings/identities.vue` | Create identity button |
| `settings-sync-add-backend` | `settings/sync.vue` | Add backend button |

## User Controls

Driver.js popover footer includes:
- **Weiter** — advance to next step (always enabled)
- **Überspringen** — skip current step, advance
- **Tour beenden** — abort tour entirely → `tourStore.abort()`

No blocking — user can always proceed without completing the action for that step.

## Files to Create/Modify

### New
- `src/stores/tour.ts` — Pinia store
- `src/composables/useTour.ts` — Driver.js wrapper

### Modify
- `src/components/haex/system/settings/index.vue` — add `data-tour` on nav items, expose `activeCategory` setter
- `src/components/haex/system/settings/general.vue` — add `data-tour="settings-device-name"`
- `src/components/haex/system/settings/extensions.vue` — add `data-tour="settings-extensions-install"`
- `src/components/haex/system/settings/identities.vue` — add `data-tour="settings-identities-create"`
- `src/components/haex/system/settings/sync.vue` — add `data-tour="settings-sync-add-backend"`
- `src/layouts/default.vue` or `app.vue` — tour trigger on vault load
- **Delete** `src/components/haex/welcome/dialog.vue` — replaced by tour

## Driver.js Config

```ts
driver({
  animate: true,
  overlayColor: 'rgba(0,0,0,0.5)',
  allowClose: false,        // must use the Tour beenden button
  stagePadding: 8,
  popoverClass: 'haex-tour-popover',
  nextBtnText: 'Weiter',
  prevBtnText: 'Zurück',
  doneBtnText: 'Fertig',
})
```

Custom CSS for `haex-tour-popover` to match Nuxt UI design tokens.
