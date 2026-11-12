# Research: The header bar belongs to the app

Each entry: **Decision**, why, what was rejected. R-numbers are cited by `plan.md`, `data-model.md` and the contracts.

## R1 — The channel: an SDK method through the bridge, not the frame shim

**Decision**: An extension hands its header over with a new bridge method `extension_header_set` (the SDK call
`client.header.set(...)`), the same way it marks its tab with `extension_tab_attention`. Rust validates it in the one
checkpoint (ADR-0004), answers the SDK call (`3001` invalid input, with the reason), and emits the validated
description as a Tauri event `extension-header-set {frame, header}`; the frame composable gives it to its own tab.

**Why**: the precedent is exact (`bridge/methods.rs` `tab_attention`, `extension-tab-attention` in
`useExtensionFrame.ts`). A bridge call answers the author synchronously with a reason when the description is
refused (FR-021's "the developer console says why" comes free, plus the SDK promise rejects), the allowlist keeps one
list of what an extension may ask, and the validated type can be exported to TypeScript for the system apps.

**Rejected**: the frame shim (`holzi:frame:init` port). The shim runs in the extension's realm and is an observer of web
standards (hash, `document.title`, `beforeunload`); a header has no web equivalent, which is what `ponytail:` in
`protocol/shim.rs` already says for attention. It would also put extension-facing API outside the SDK.

## R2 — Validation in Rust, types exported with ts-rs

**Decision**: `src-tauri/src/extensions/bridge/header.rs` holds the serde types and `validate`, with the tests in
`header_tests.rs`. The types are exported with `#[ts(export, export_to = "../../src/types/bindings/")]` under the
domain prefix `WmHeader…` (feedback: ts-rs name collisions). The frontend trusts what Rust emitted and renders it;
system apps build the same exported type.

**Rules in `validate`** (details in `contracts/header-description.md`): the value is an object with `start`, `center`,
`end` (each optional, default empty); every control is parsed on its own. A control whose `type` is **not in the
catalog** is left out (FR-021); a control of a known type that is malformed, lacks its accessible name, repeats an id
or breaks a limit makes the **whole** description fail and nothing is emitted.

**Why one place**: the limits are security-relevant (FR-022/023) and belong where the other extension input is
checked; Rust tests run without a webview.

**Rejected**: validating in TypeScript at the frame (a second parser next to `readShimMessage`). It would leave the
bridge accepting anything and put the check where it is harder to test per case. Also rejected: a Rust enum with
`#[serde(tag = "type")]` for the whole list: an unknown tag would fail the whole description, which the spec says to
only leave out.

## R3 — The limits (FR-022)

| Bound                                                                                 | Value                                                                                                                              |
| ------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------- |
| Controls per description (all areas together)                                         | 32                                                                                                                                 |
| Entries per `menu`                                                                    | 64                                                                                                                                 |
| Options per `choice`                                                                  | 8                                                                                                                                  |
| Segments per `breadcrumb`                                                             | 64                                                                                                                                 |
| Identifier                                                                            | 1–64 characters of `A–Z a–z 0–9 . _ : -`                                                                                           |
| Text (`label`, `text`, `tooltip`, `placeholder`, entry/option/segment label, `title`) | 1–120 characters (`placeholder`, `tooltip` may be empty)                                                                           |
| One icon file (decoded)                                                               | 256 KiB (the limit of notification icons)                                                                                          |
| Total size or number of icons                                                         | **no limit** (the operator's rule: shipped files, nothing from outside; bounded in practice by the control and entry counts above) |
| An inline `data:` icon (encoded text; development versions only)                      | decodes to at most 256 KiB (encoded text at most 2 × 256 KiB), the same as notifications                                           |
| `search.value`, `breadcrumb.editValue`                                                | up to 1000 characters                                                                                                              |
| Collapse order                                                                        | integer 0–1000                                                                                                                     |
| Whole description without inline `data:` icons (serialised JSON)                      | 32 KiB                                                                                                                             |
| Calls                                                                                 | no limit; the frontend applies at most one description per animation frame (R7)                                                    |

**Why**: the file browser needs about ten controls and one menu of eight entries; Settings three. 32/64/120 leave
room several times over and stay far below anything that could crowd a 40 px bar. Tested at the boundary and one over.

## R4 — Events back to the extension

**Decision**: holzi posts `{type: "haextension:header:event", data: {id, kind, value?}, timestamp}` on the frame's SDK
port with a direct `postMessage`, not through `FrameEventQueue`. The SDK exposes it as `client.header.onEvent(handler)`
(`client.on(HAEXTENSION_EVENTS.HEADER_EVENT, …)`).

**Why**: `FrameEventQueue` carries Rust-origin events and has reliability bookkeeping for peer channels. A header event is
born in the frontend, and a control can only be used after a description arrived over the same port, so the port is ready
by construction. The throttle for `input` (FR-012, ~100 ms, leading edge on Enter/blur) lives in the header component
(R7), so the extension never debounces.

**Event kinds**: `click` (button, menu entry: `value` = entry id for menus), `toggle` (`value` boolean), `input`
(`value` text), `submit` (`value` text; Enter in a search, a typed breadcrumb path), `select` (`value` = option id of a
`choice`, segment id of a `breadcrumb`).

## R5 — Where the description lives: the tab's runtime

**Decision**: `TabRuntime` (`src/lib/wm/types.ts`) gets `header: WmHeaderDescription | null` and
`headerHandler: ((event: WmHeaderEvent) => void) | null`. Never persisted (like `attention`). The header of a window
reads the active tab's runtime, so switching tabs switches headers for free and a background tab's description is
kept (spec: Edge Cases).

**Clearing**: the frame composable sets `null` when its frame reloads, closes or fails (FR-020); a system app clears it
by unmounting (the composable of R6 does it in `onScopeDispose`).

**Why**: `TabRuntime` already carries `attention`, `titleOverride`, `guard`, which are the same kind of per-tab
state, and `syncTabRuntime` already drops it with the tab.

## R6 — The system-app side: `useWmHeader`

**Decision**: `useWmHeader(description: () => WmHeaderDescription | null, onEvent: (event) => void)` in
`src/composables/useWmHeader.ts`, built on two new members of `WmTabApi` (`setHeader`, `registerHeaderHandler`),
provided by `TabPanel.vue` exactly as `setTitle` and `registerActionHandler` are, with inert defaults for the Node
harnesses. The getter runs inside a `watchEffect`, so a reactive app (the sidebar toggle's `pressed`) updates the header
by changing its state.

**Why the handler is separate from the description**: a description is plain JSON (it is what an extension sends), so
callbacks cannot be in it. One `onEvent` with a switch on `id` is the same shape the extension has.

**Actions (FR-025)**: the handler calls the same catalog action the removed control called (`useAction`), or the
same local function where the control was action-exempt (Settings' sidebar toggle is local state today).

**Rejected**: a Vue Teleport into the header for system apps (decision 4 of the spec: one description, two producers).

## R7 — The layout: a pure function over measured widths

**Decision**: `src/lib/wm/headerLayout.ts` exports `layoutHeader(controls, widths, available)`, a pure function that
returns, per control, one of `row | icon | shrunk(width) | overflow`. `Header.vue` measures and applies. The algorithm is in
`contracts/collapse.md`. Measuring: a hidden, inert, `aria-hidden` ghost row renders every control in its full form
(and in its icon form where the form is `icon`); a `ResizeObserver` on the header and on the ghost feeds the widths;
the visible row is the function's result. The window's width, not the screen's, drives it (FR-016) because the observer
sits on the header element.

**Applying**: the description is applied at most once per animation frame (`requestAnimationFrame` coalescing), so an
extension that echoes every key cannot rebuild the bar faster than the screen.

**The search rules** (FR-012 merge, FR-013 typed text) are a second pure module, `headerSearch.ts`, with an injected
clock, for the same reason.

**Why a function**: the order/form rules (FR-011/014/017) have enough cases (ties, `keep`, `icon` without icon, all
`keep`, the menu button's own width) that they need a unit test per case without a DOM; `scripts/check-*.ts` is the
repo's pattern.

**Rejected**: CSS-only collapse (`@container`, `hidden` classes): cannot express the app's order across areas, nor
the icon form, nor the menu's content.

**Fallback widths**: `shrink` has a smallest width per type unless the control says more: search 120 px, breadcrumb
96 px, title 64 px. (The control may not set it; the types fix it. A property for it can be added later, additively.)

## R8 — The geometry of the header

**Decision**: the header is one row of **36 px plus the 1 px bottom border**, which is what the title row measures today
(`pt-1` 4 px + 32 px buttons = 36 px, `border-b`). Controls are 32 px high. T001 measures the real value in the running
app and records it in `quickstart.md`; if it differs by more than 1 px the number here is corrected, not the
spec (FR-005, SC-001). Derived from the code on 2026-11-12 (`pt-1` + the 32 px window buttons + `border-b`): 37 px;
`Header.vue` is `h-9` + `border-b` = 37 px. The measurement in the running app is still to do.

The header keeps the surface and the drag behaviour of the title row (`Window.vue`): `bg-muted/40`, `@pointerdown` →
`startMove` when `interactive`, `@dblclick` → maximize. Every control stops `pointerdown` (as `WindowControls` does
today) so it never starts a drag.

**Role**: `role="toolbar"` with the window title as `aria-label`; no arrow-key roving (a search field in the bar would
fight it), the Tab key walks start → middle → end (FR-006).

## R9 — Icons: an extension ships its own, a system app names one

**Decision**: the operator's rule (spec FR-009) is that an extension shows **only icons it ships itself**, each small.

- **Extension**: a control's `icon` is a file path inside the extension's own signed bundle (`"icons/plus.svg"`), or,
  for a development version (no stored bundle), a `data:` URL. Rust reads the file with `read_verified_file` (the
  signature of that file is checked), accepts PNG, JPEG, WebP, GIF and SVG, and enforces **256 KiB per file** and
  no limit on how many icons or how much in total (the files are in the signed bundle, so nothing dangerous is
  loaded and all files are already there; larger menus are allowed). A name from holzi's set (`lucide:…`), a URL
  (`://`), a path outside the bundle or a file of another type is refused (the existing rule of `notifications.rs`,
  which is the precedent: `MAX_ICON_BYTES = 256 * 1024`, `data:` URL or bundle file).
- **System app**: unchanged, an Iconify name (`lucide:panel-left`), bundled at build time by the scan in
  `nuxt.config.ts`. System apps are holzi's own code.
- **The emitted description** carries the pictures: Rust adds `icons: { "<path>": "data:<mime>;base64,…" }` to the
  description it emits (the extension cannot send this field; it is ignored if it does), and a control keeps
  `icon: "<path>"`. Because there is no total limit and an extension may set its header often, Rust keeps a cache of the
  icons it has resolved **per frame session** (dropped with the session) and an event carries only the icons the tab
  has not been sent yet; the frontend merges them into an icon table that lives exactly as long as the same frame
  session (`useExtensionFrame`'s `closeAsync` drops it), so both sides always agree on what was sent, also when only the
  document reloads and the description is cleared. Both sides keep only the icons the current description uses: with
  every new description Rust forgets, and the frontend drops, the icons it no longer names, so the cache is bounded by
  one description (32 controls and their entries) however often the header changes; an icon used again later is read
  and sent again. A file is therefore read and signature-checked once per frame, and the same file on many
  controls is sent once. The frontend tells the kinds apart by prefix:
  `lucide:` is an Iconify name, anything else is looked up in `icons`.
- **Drawing**: an extension picture is drawn as an `<img>` from its `data:` URL, never inlined into the DOM, so an SVG
  cannot run script or load anything (SVG in an image context is inert). A control may set `monochrome: true`, then
  the picture is drawn with CSS `mask-image` in the text colour (its alpha channel is the shape), so a one-colour icon
  follows light and dark schemes; the default is the picture as it is.
- **Reuse (graphify-first)**: `notifications.rs::icon` already does the path-or-data-URL reading and size check. It is
  extracted into a shared helper (`extensions/icon_file.rs`, parameters: allowed types, byte limit) that both
  notifications and the header call. Notifications keep their raster-only types and their tests; behaviour is
  unchanged.

**Why**: it follows the operator's rule and the existing precedent, needs no list to maintain in two languages, and
keeps the app offline (no iconify fetch for an unknown name, no external URL ever loaded).

**Rejected**: a closed list of holzi icons for extensions (first plan; extensions would look like holzi, the list had
to be kept in two repositories); any `lucide:` name from an extension (would need to be in the build scan, otherwise it
is fetched from the network); inline SVG markup (script and style injection).

**Cost accepted**: reading files makes `extension_header_set` a call with a database read (as the notification call
is), and a description with icons is a few hundred KiB at most in an event; `HeaderDescription` equality checks in the
frontend compare the `icons` table by reference (it only changes when the files do).

## R10 — The overflow layer

**Decision**: the menu button opens the layer's popover (`ShadcnPopover`, used in `AgentSessionsPopover.vue`), not a
dropdown menu: the popover has to hold a search field and a breadcrumb, which a menu's typeahead and roving focus
fight. Inside it a vertical list renders each moved control in its "menu form" (button → full-width row, toggle →
switch row, menu → submenu list, choice → radio rows, search → the field, breadcrumb → a row per segment, title →
label, separator → rule). Choosing a button, toggle, choice or menu entry closes it (FR-015); a search keeps it open.

**Why**: `ShadcnDropdownMenu` (the path bar's current menu) stays right for `menu` controls in the row; the overflow
layer is a different thing.

## R11 — The tab strip leaves the header (decision 2)

**Decision**: `Window.vue` stops rendering `WmTabBar` and `WmTabListMenu`; `WmTabBar.vue`, `WmTabListMenu.vue` and
`WmNewTabMenu.vue` are deleted if nothing else uses them (knip decides in the clean-up task; `WindowOverview.vue` is the only other
user of tab data and does not render them). Spec 091 restores what it needs (the new-tab menu, say) from git. The tab model,
`wm.tab.*`, history, side buttons, guards, attention and session restore are untouched (FR-028).

**What goes with it** (FR-029): `active-tab emphasis` (spec 040) and the attention dot on an inactive tab have no
place to show until the follow-up spec; the unit checks that cover them (`check-wm-*`) are adjusted to the model, not
to the strip. The window's own attention badge (`WindowOverview`, dock) is unchanged because it derives from
`TabRuntime.attention`.

**Rejected**: keeping the components "for the follow-up" (dead code that knip reports and that rots; git has it).

## R12 — Settings

**Decision**: `SettingsApp.vue` calls `useWmHeader` with `start: [toggle sidebar]` and `center: [search]` and drops
`SettingsToolbar` (the file is deleted). The search is always visible as a field (the COSMIC reference), so the
search-open button and its state (`searchOpen`, `settings-search-open`) go away; Escape clears first and then returns
focus to where it was (spec 023 FR-023 keeps its meaning). Collapse: search `shrink` (order 10), toggle `keep`.

**Container queries**: the content below keeps its `@container` rules (the sidebar follows the frame's width, spec
023 FR-004). The header does not depend on them.

**E2E**: `settings-categories` and `settings-narrow-window` use `settings-sidebar-toggle` and `settings-search-open`;
the first keeps its test id on the header control, the second is rewritten for the always-open field.

## R13 — The file browser

**Decision**: a pure builder `src/lib/files/header.ts` turns the one-panel state (history, path, view, sort, hidden,
create menu, split switch) into a description; `FilesApp.vue` passes it to `useWmHeader`. With the split on, the
description is only `start: [sidebar]` + `end: [split switch]` and `PanelBar.vue` stays in each panel exactly as
spec 062 has it. `files/Toolbar.vue` is deleted.

**Known ceiling (marked `ponytail:` in the code)**: the one-panel controls exist twice, as a description (header) and as
`PanelBar.vue` (two panels). The upgrade path is to render `PanelBar` from the same description; it is left out here
because two-panel behaviour (spec 062: per-panel history, selection) is the risk, not the header.

**Breadcrumb collapse**: the breadcrumb control's `shrink` form replaces the `max-md` swap in `PathBar.vue`: below its
full width it shows the open folder's name with the menu of the folders above, then Edit and Reload, as today on a
phone, decided by the control's width instead of the screen's.

**E2E**: `files-split*`, `files-agent`, `files-sync-rule-menu` use the toolbar's test ids; the ids stay on the header
controls (`files-sidebar-toggle`, `files-reload`, …) where the control survives.

## R14 — haex-vault 2.x and the SDK

**Decision**: the SDK gets a `HeaderAPI` (`client.header.set`, `client.header.onEvent`) and the types, in
`haex-space/vault-sdk` as its own PR. In haex-vault 2.x `extension_header_set` is an unknown method (it replies with a plain
`Error("Unknown method: <method>")`, `src/composables/extensionMessageHandler.ts`; holzi replies with code 8000). The
SDK's `header.set` therefore **resolves with `false`** instead of rejecting when the host answers unknown method (code
8000, or the 2.x message prefix `Unknown method`), and with `true` when applied; every other error still rejects. An
extension keeps running and can read the result to decide not to try again. holzi does
**not** wait for the SDK release: the E2E `probe` fixture calls `probe.request('extension_header_set', …)` directly
(as `extension-dialog` does for `extension_dialog_confirm`).

**Why**: FR-018 requires additive and ignorable; a rejected promise nobody caught is an unhandled rejection, so the
SDK absorbs the one known failure mode (unknown method) and reports it as a value.

## R15 — Checks and tests

| Layer              | Where                                                                  | What                                                                                 |
| ------------------ | ---------------------------------------------------------------------- | ------------------------------------------------------------------------------------ |
| Rust               | `bridge/header_tests.rs`                                               | every FR-021/022/023 case, boundaries, unknown type, duplicate id, missing label     |
| Layout function    | `scripts/check-wm-header.ts` (in `check:wm-state`)                     | order, forms, `keep`, ties, icon without icon, all-`keep`, restore order, menu width |
| Files builder      | `scripts/check-files-header.ts` (in `check:files`)                     | one panel vs split, disabled states, history, sort/hidden/create                     |
| Frame → tab        | `scripts/check-extensions-header.ts` (in `check:extensions`)           | clear on reload/close, per-tab, event post shape                                     |
| E2E                | `settings-*`, `files-*` moved; new `extension-header`; `window-header` | SC-001/002/005/007 and the extension round trip with the `probe` fixture             |
| By hand (operator) | SC-008                                                                 | Settings against the COSMIC reference                                                |

E2E does not run on pull requests: before merge, `gh workflow run ci.yml --ref <branch>` (feedback).

## R16 — Build and process constraints that shape the steps

- Rust builds only in the 10 GB scope with `-j 1`, `CARGO_PROFILE_DEV_DEBUG=line-tables-only CARGO_INCREMENTAL=0`,
  `env -u CC -u CXX -u AR` (quickstart repeats the command).
- A new `#[ts(export)]` type changes `src/types/bindings`; `cargo test` leaves trailing spaces, so the CI's
  `sed -i -E "s/[[:space:]]+$//" src/types/bindings/*.ts` runs before committing.
- `pnpm typecheck:scripts` before every push: the layout and builder modules are loaded by Node check scripts and must
  not pull a binding with nested imports. **Risk**: the description type is recursive in a few places (menu entries); if
  `tsconfig.scripts.json` cannot resolve it, `headerLayout.ts` takes a structural subset type it declares itself (only
  `id`, `collapse`, `type`, `icon`), and the full type stays in the components. T002 settles it first.
  **Settled in T002 (2026-11-12)**: it cannot. A generated binding that imports another one (`./WmHeaderControl`
  without an extension) fails under `moduleResolution: nodenext`, even one level deep. The fix is not the structural
  subset but `#[ts(inline)]` on every field of the `WmHeader*` types that names another type: each generated file is
  then self-contained and the full type is loaded by the Node check scripts (`lib/wm/types.ts`). The cost is a larger
  `WmHeaderDescription.ts` (the control union is written out for each of the three areas).
- Rust with `cfg` branches is clipped for `aarch64-linux-android` too; `header.rs` has none, so this only confirms.
- Commit messages: Conventional Commits, no agent trailer (feedback and the operator's global rule); PR branches live
  in a repository-local worktree made from `origin/main` (`git worktree add`).
