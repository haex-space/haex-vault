# Quickstart: checking spec 093 by hand

Per app, after its pull request (`pnpm tauri dev` or the E2E build):

1. Open the app in a window of 1000 px: the header holds every control of the list in the row; no row of the app under it
   (the content starts where Settings' content starts).
2. Narrow the window to 460 px: the search shrinks, then moves into the panel (⋯), then the filter, the menu and "+" in
   that order; the sidebar button and the window buttons stay. Every control is usable in the panel.
3. Set a filter (Passwords: a tag; Wallet: a kind; Marketplace: a category). The filter icon has a dot; narrow the window
   until the filter is in the panel: the ⋯ button has a dot; open the panel: the filter's row has a dot and its entry a
   check mark. Reset the filter: the dots go.
4. Open an entry/contact/pass: the controls named in the spec leave the header; close it: they return.
5. Open a dialog from the menu (Passwords: new folder, tags, settings), collapse the menu into the panel while it is open
   (resize): the dialog stays.
6. Search, press Escape (clears), step back in the history (restores the text).

Checks to run: `pnpm check:wm-header`, `pnpm check:<app>-header`, `pnpm typecheck`, `pnpm typecheck:scripts`,
`pnpm lint`, `pnpm format:check`, `pnpm check:templates`; Rust: `cargo test --lib bridge::header` in the 10 GB scope
(`CARGO_PROFILE_DEV_DEBUG=line-tables-only CARGO_INCREMENTAL=0 … -j 1`). E2E: `gh workflow run ci.yml --ref <branch>`.
