# Quickstart: The header bar belongs to the app

All commands from the worktree root. Frontend commands run in the Nix shell (`nix develop --command sh -c '…'`) as
in the other specs; on Pop!_OS without Nix, as the CI does (apt + rustup).

## Fast checks (no app)

```bash
pnpm check:wm-state          # includes check-wm-header.ts (layout function)
pnpm check:files             # includes check-files-header.ts
pnpm check:extensions        # includes check-extensions-header.ts
pnpm check:wm-navigation     # tab actions and history still work without the strip
pnpm check:settings
pnpm typecheck:scripts       # before every push (research R16)
pnpm lint && pnpm format:check
```

After a frontend change to a composable that runs in the Node sandboxes (`useWmHeader`, anything under
`src/lib/wm/`): run **all** `check:*`, not only the one you touched (feedback), and no `instanceof Element`.

## Rust unit tests (header validation)

```bash
cd src-tauri
env -u CC -u CXX -u AR CARGO_PROFILE_DEV_DEBUG=line-tables-only CARGO_INCREMENTAL=0 \
  systemd-run --user --scope -q -p MemoryMax=10G -p MemorySwapMax=0 \
  cargo test -j 1 --config 'profile.dev.package.holzi.debug=0' --lib -- extensions::bridge::header
cd .. && sed -i -E "s/[[:space:]]+$//" src/types/bindings/*.ts     # ts-rs leaves trailing spaces
git status --short src/types/bindings                              # only the new WmHeader* files
```

## See it in the running app

1. `pnpm dev` (frontend on port 3030) and the debug binary as in spec 023's quickstart; open Settings.
2. Settings: one bar with the sidebar button, the search and the window buttons; no row under it. Drag the bar;
   double-click maximizes. Type in the search: hits appear in the sidebar; Enter opens the first; Escape clears.
3. Narrow the window: the search shrinks, then moves into the `⋯` popover; the sidebar button and the window
   buttons stay. Widen it: they come back in reverse.
4. Files: one panel, the whole bar is in the header; turn on the split: the header keeps the sidebar button and the
   switch, each panel has its bar.
5. A window with two tabs (create one with the action `wm.tab.new`): no tab strip;
   restart the app: both tabs are back.
6. Extension: install the `probe` fixture, open it, and in its frame run
   `window.probe.request('extension_header_set', {start:[{type:'button',id:'a',label:'A',icon:'plus'}]})`.
   The button shows in that window's header; click it and read the event in the frame's console.
   A 33-control description is refused with `3001` and the old header stays.

## T001 measurement (first task)

With the app running, measure the title row's height (`getBoundingClientRect` of the element holding `WmTabBar`) at
the default window size; write the number into `research.md` R8. The new header uses the same.

## E2E (needs the CI image or the host bridge)

```bash
pnpm test:e2e --grep settings-    # then --grep files-split, --grep extension-header, --grep window-header
# (--grep takes a text that the scenario name contains; the runner has no scenario arguments)
```

E2E does not run on pull requests: before merging, `gh workflow run ci.yml --ref <branch>` and wait for green.

## Reference

- Spec: [spec.md](./spec.md) · Plan: [plan.md](./plan.md) · Research: [research.md](./research.md)
- COSMIC Settings headerbar: the screenshot in the spec's input (sidebar button, search pill with clear, window buttons).
