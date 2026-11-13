# Research: The headers of Contacts, Passwords, Wallet and the Marketplace

Each entry: Decision, Rationale, Alternatives. Spec 090's research (R1–R16) holds for everything not named here.

## R1. The state of the apps stays where it is; the description is derived from it

**Decision**: Each app builds its description in a pure function (`src/lib/<app>/header.ts`, as `src/lib/files/header.ts`
does) from plain values (sidebar shown, search text, the open place, the filter values) and hands it to `useWmHeader`.
The events (`click`, `toggle`, `input`, `select`) call the handlers the old toolbar called. The old `Toolbar.vue` of each
app is deleted in the same pull request.

**Rationale**: spec 090 R12/R13 did the same for Settings and Files, and the pure function is checkable in Node without a
DOM (`check:*` scripts), which is where the old toolbars had no test.

**Alternatives**: keep the toolbar component and teleport it into the header (rejected: spec 090 Decision 2, the
host draws the controls); one generic "app toolbar" builder (rejected: four apps with one use each, spaex ponytail rule).

## R2. Dialogs move up to the app frame

**Decision**: The dialogs the Passwords toolbar owns (`PasswordsClipboardSetting`, `PasswordsTagManager`,
`PasswordsFolderDialog`) are mounted in `PasswordsApp.vue` with their open flags in the same component, not in a
control. The header event only flips the flag.

**Rationale**: FR-009. A control can be collapsed into the panel or not drawn; a dialog in the toolbar would unmount with
it. Contacts and Wallet have no dialogs in their toolbars (verified when their tasks start; if one has, same rule).

**Alternatives**: a store for the flags (rejected: one consumer).

## R3. The dot is a flag of the menu control (`active`)

**Decision**: `WmHeaderControl::Menu` gets `active: Option<bool>` (`#[ts(optional)]`, default off). The renderer draws a
small dot (theme `primary`, as the icon colour, absolutely positioned so it adds no width) at the top-right of the menu's
icon. A menu in the overflow panel is drawn as a group (a heading, then one row per entry, no icon of its own): the dot
goes next to the heading. The overflow button draws it when any control that sits in the panel is `active`.
Rust's validation accepts the flag on a menu (a non-boolean is refused) and ignores it on any other control (the contract's check 2).

**Rationale**: spec 093 FR-008 and its clarification. Same mechanism as `checked` (spec 090, #986): one optional boolean
through Rust types, bindings, validation, renderer, SDK. Only `menu` has a filter state to show; `choice` already shows
its selection, `toggle` its `pressed`.

**Alternatives**: the tag's name as text next to the icon (rejected by the operator, 2026-11-13, and it overflows the
row); a `badge` field on every control (rejected: no use outside the menu).

## R4. Overflow button and dot

**Decision**: `layoutHeader` already returns which controls moved into the overflow panel. `Header.vue` computes
"any overflowed control is `active`" and hands it to `HeaderOverflow.vue`, which draws the dot on the ⋯ button. No change
to `layoutHeader`.

**Rationale**: the layout stays a pure function of widths; activity is a property of the description, joined in the
component that already knows both.

## R5. The "+" menu and the sidebar toggle

**Decision**: "+" is a `menu` control (entries entry/folder; contacts: new contact/read card), drawn like every header
control (a ghost button, the icon in primary), not the filled button of today: the header has one look.
The sidebar toggle is `toggle` with `pressed` = shown, `form: keep`, exactly as Settings.

**Rationale**: consistent with the pilots; the operator asked for no highlight (2026-11-13).

## R6. Marketplace choices are menus

**Decision**: category and marketplace are `menu` controls whose entries are `checked`; the category menu's entries carry
the count in the label (`Name (12)`), as today. "Refresh" is a `button` with `disabled` while loading. The search is the
header's `search`.

**Rationale**: `choice` holds at most 8 options; categories are unbounded. Both menus are filters, so both get `active`
when not "all".

**Alternatives**: a `select` control (rejected: a new control for one app; a menu with check marks does the same).

## R7. Where the Marketplace sets its header

**Decision**: from `CatalogGrid.vue`, which owns the search text, category, source and loading flag (it passes them to
the toolbar today). It calls `useWmHeader`; `CatalogToolbar.vue` is deleted. Entry/details pages inside the same window
are not given a header description of their own (no change from today: the toolbar is part of the grid page).

**Rationale**: the state is there; lifting it to the app root is churn. `useWmHeader` clears the header on scope
dispose, so the controls leave the header when the grid unmounts (a detail page), as the toolbar left the page.

**Checked at spec review**: the grid and the detail page are sibling routes of `MarketplaceApp` (`MARKETPLACE_VIEWS` in
`src/components/wm/appRoutes.ts`, rendered by `WmRouterView`), so the grid unmounts on a detail page and its controls
leave the header with it. No empty description is needed; T028 only confirms it by hand.

## R8. Order and dependencies of the pull requests

**Decision**: Step 0 is the dot (Rust, bindings, renderer, SDK type), its own pull request with a check script, a unit
test and a minimal E2E extension (`window-header`). Then one pull request per app, in the order of the spec
(Passwords, Contacts, Wallet, Marketplace). Steps 1–4 need #986 (the `checked` entries and the panel on the window) on
`main`; nothing starts before it is merged or the step branches off it.

**Rationale**: spec Decision 4; step 0 isolates the only mechanism change so the app pull requests stay moves.

## R9. Extension side

**Decision**: the SDK type gets `active?: boolean` on the menu control. It goes into vault-sdk#112 if that is still open
when step 0 lands, else a follow-up pull request there. The protocol stays additive: an older holzi ignores a field it
does not know (serde does not deny unknown fields), and an older SDK never sends it.

**Rationale**: constitution IV (pinned cross-repo references): when merged, cite the SDK as
`haex-space/vault-sdk@<sha>:src/types/header.ts`.

## R10. What a header menu cannot show yet

**Decision**: open, for the operator, before Phase 2 (Passwords) and Phase 4 (Wallet). A `menu` entry is
`{ id, label, icon?, disabled?, checked? }` and the menu has no heading entry. Today's rows use two things more:

- **Group headings** in Wallet's filter menu ("Sort", "Kind", under which the entries stand). Without a heading, "Date",
  "Title" and the kind names stand in one list. Options: (a) a `heading` entry type for menus (Rust, bindings, renderer,
  SDK: one more mechanism change, best done in step 0 so the apps stay moves); (b) separators only, and labels that say
  what they are ("Sort by date"; new translation keys).
- **The tag's colour and count** in the Passwords tag filter (a coloured icon, the number of entries at the end). The
  count can go into the label (`Name (3)`, as R6 does for categories). The colour cannot: header icons are names, not
  coloured. Options: accept the loss (the check mark and the name stay), or add an optional colour to a menu entry.

**Rationale**: spec FR-004 asks for the same effect; both differences are visible and the mechanism change is small, so
the decision belongs to the operator, not to the builder.
