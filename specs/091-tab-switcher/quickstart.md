# Quickstart: Switching tabs by hand

All commands from the worktree root; frontend commands in the Nix shell as in the other specs (or as the CI does).
This spec is frontend only: no Rust build.

## Fast checks (no app)

```bash
pnpm check:wm-navigation     # includes check-wm-tab-switcher.ts and the system-back case
pnpm check:wm-state          # includes check-wm-tile-grid.ts and check-wm-tab-switcher-store.ts
pnpm check:templates
pnpm typecheck && pnpm typecheck:scripts
pnpm lint && pnpm format:check
pnpm knip                    # after the AppTiles extraction
```

After a change under `src/lib/wm/` or to a composable, run **all** `pnpm check:*` (Node sandbox rules).

## See it in the running app

Part A (no spec 090 needed):

1. `pnpm dev` and the debug binary as in the other quickstarts; open the launcher.
2. With the six system apps everything fits: no scroll bar.
3. Install the `probe` fixture several times under different names, or use the test-only app-list override
   (research R9), to get 40 apps. The launcher's grid scrolls (wheel, touchpad, finger, scroll bar); the assistant and
   lock tiles stay visible; the arrow keys move between tiles and scroll the focused one into view.
4. Start a scroll on a tile and let go: the scroll does not open it; a separate press afterwards does. Long-press or right click a tile: the pin menu opens; a scroll that
   starts on a tile on the phone does not open it (research R6).

Part B (after spec 090 is merged):

5. A window with one tab shows a "+" beside the window buttons. Press it: the app tiles cover the content; pick one;
   it opens as a tab and the button shows 2.
6. Press the number: the overview with two tiles, the open one with a ring and a check; pick the other; Escape;
   the "+" tile; close a tile with its button; down to one tab the overview closes and the "+" is back.
7. Make a background tab ask for attention (`probe.request('extension_tab_attention', {active: true})` in an inactive
   tab's frame): the button shows a dot; the tile shows it too. Showing the tab does not clear it; send `{active:
false}` from the probe and both marks go.
8. Compact window (narrow it, or Android): two columns, large close buttons, no horizontal scroll; the Android back
   gesture closes the overlay.

## E2E

```bash
pnpm test:e2e --grep tab-switcher
pnpm test:e2e --grep launcher-grid
```

E2E does not run on pull requests: before merging, `gh workflow run ci.yml --ref <branch>` and wait for green.

## Reference

Spec [spec.md](./spec.md) · plan [plan.md](./plan.md) · research [research.md](./research.md) · contracts
[tab-button](./contracts/tab-button.md), [switcher-overlay](./contracts/switcher-overlay.md),
[app-tiles](./contracts/app-tiles.md).
