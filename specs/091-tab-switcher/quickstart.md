# Quickstart: Switching tabs by hand

All commands from the worktree root; frontend commands in the Nix shell as in the other specs (or as the CI does).
This spec is frontend only: no Rust build.

## Fast checks (no app)

```bash
pnpm check:wm-navigation     # includes check-wm-tab-switcher.ts and the system-back case
pnpm check:wm-state          # includes check-wm-app-pages.ts
pnpm check:templates
pnpm typecheck && pnpm typecheck:scripts
pnpm lint && pnpm format:check
pnpm knip                    # after the AppTiles extraction
```

After a change under `src/lib/wm/` or to a composable, run **all** `pnpm check:*` (Node sandbox rules).

## See it in the running app

Part A (no spec 090 needed):

1. `pnpm dev` and the debug binary as in the other quickstarts; open the launcher.
2. With the six system apps everything is on one page: no indicator, no swipe.
3. Install the `probe` fixture several times under different names, or use the test-only app-list override
   (research R9), to get 40 apps. The launcher now has pages and an indicator; swipe, drag with the mouse, use the arrow
   keys at a page's edge and PageUp/PageDown. The assistant and lock tiles stay visible on every page.
4. Press a tile after a swipe: it does not open. Long-press or right click a tile: the pin menu opens; start a swipe on
   a long press on the phone: no menu (research R6).

Part B (after spec 090 is merged):

5. A window with one tab shows a "+" beside the window buttons. Press it: the app tiles cover the content; pick one;
   it opens as a tab and the button shows 2.
6. Press the number: the overview with two tiles, the open one with a ring and a check; pick the other; Escape;
   the "+" tile; close a tile with its button; down to one tab the overview closes and the "+" is back.
7. Make a background tab ask for attention (`probe.request('extension_tab_attention', {active: true})` in an inactive
   tab's frame): the button shows a dot; the tile shows it too.
8. Compact window (narrow it, or Android): two columns, large close buttons, no horizontal scroll; the Android back
   gesture closes the overlay.

## E2E

```bash
pnpm test:e2e --grep tab-switcher
pnpm test:e2e --grep launcher-pages
```

E2E does not run on pull requests: before merging, `gh workflow run ci.yml --ref <branch>` and wait for green.

## Reference

Spec [spec.md](./spec.md) · plan [plan.md](./plan.md) · research [research.md](./research.md) · contracts
[tab-button](./contracts/tab-button.md), [switcher-overlay](./contracts/switcher-overlay.md),
[app-tiles](./contracts/app-tiles.md).
