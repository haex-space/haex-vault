# Feature Specification: Switching tabs by hand

**Feature Branch**: `091-tab-switcher`

**Created**: 2026-11-11

**Status**: Draft

**Input**: User description: "Die Tabs wie bei den Mobile-Browsern: bei nur einem Tab ein „+“-Button für weitere Tabs,
ab zwei Tabs stattdessen ein Button mit der Zahl der offenen Tabs, der zu einer Kachelübersicht aller offenen Tabs
führt, in der man einen auswählt." (operator, 2026-11-11), after spec 090 took the tab strip out of the window header.
The decisions are below.

## Background

- **Where spec 090 left off**: the header bar of a window belongs to its app, the tab strip and the tab-list menu are
  gone from it. The tab model stays: a window holds several tabs, shows the active one, and every tab action
  (`wm.tab.new`, `wm.tab.activate`, `wm.tab.close`, back, forward) and the session restore work as before. What is
  missing is a way for a person to open, see, switch and close tabs without an agent or a keyboard shortcut. Spec 090
  accepted that gap (its Decision 2) and named this spec as the way to close it.
- **What a person has today**: nothing visible. The mouse side buttons walk the history of the active tab; the keyboard
  shortcuts of spec 020 (Alt+Arrow) walk the history of the active tab, and no shortcut switches tabs; the window
  overview (spec 015) lists windows, not tabs.
- **The pattern**: mobile browsers show one button with the number of open tabs, and a tap shows every tab as a tile
  to pick from. A window of holzi has the same two needs (few tabs, little room), and on a phone this is how people
  already switch tabs.
- **What the apps already look like**: the launcher shows every app as a tile (two columns on a compact window, three
  otherwise), with an attention dot and a disabled tile with its reason for an extension that cannot open here. A list
  of apps stops working when people install 30 to 50 extensions; a scrolling grid does, as in COSMIC's app library,
  which shows the apps as large icons with their name below and scrolls when they do not fit.
- **What cannot be shown**: a picture of a tab's content. An extension runs in a sandboxed frame of another origin
  whose pixels the window cannot read (spec 017), holzi ships no screenshot tool, and a tab that was never opened has
  no content to show (tabs mount on first use). A mobile browser has previews because it is the rendering engine;
  holzi is not.

## Decisions

The operator decided on 2026-11-11:

1. **One button, owned by holzi.** It stands beside the window buttons, is not part of an app's header description,
   and never moves into the overflow menu of spec 090. The app keeps the whole header bar to itself.
2. **Its two faces.** A window with one tab shows a "+". A window with two or more tabs shows a button with the
   number of its open tabs instead.
3. **"+" opens the apps as tiles, not as a list.** It shows the same tiles as the launcher, as an overlay over the
   window's content like the tab overview. They scroll vertically when they do not fit (as COSMIC's app library does), so 30
   to 50 installed apps stay reachable. An app that may only be open once and already is, is shown instead of opened a
   second time (spec 020).
4. **The number opens the tab overview, a grid of tiles of the window's tabs**, as an overlay over the window's content. The header
   stays, so the same button closes it again.
5. **No picture previews of tabs in this version.** A tile shows the app's icon, the tab's title and its place. A live
   preview (the open tabs shown scaled down) is a later step of its own.
6. **Tabs stay a per-window thing.** The tab overview shows the tabs of one window. Moving a tab to another window,
   reordering and groups are not part of this spec.
7. **No backward compatibility**: holzi has no users yet. The tab strip of spec 015 is not coming back in any form.

## Clarifications

### Session 2026-11-11

- Q: Does the launcher also get the grid? → A: Yes. It uses the same grid component and keeps its drawer, its pinning
  to the dock and its own tiles (voice assistant, lock), which stay in a fixed strip outside the scrolling grid so
  locking is always reachable (User Story 6, FR-022).
- Q: Pages with swipe, or a scrolling grid? → A: A vertical scroll, as in COSMIC's app library (operator, 2026-11-11,
  after the first draft had pages with a swipe like the password app). No pages, no swipe, no page indicator, no new
  gesture next to the launcher's long-press pin menu (FR-003a).
- Q: Groups like COSMIC's app library (Library, Office, System, "+ Group", edit, delete)? → A: Not in this spec; a
  follow-up spec. The grid leaves room for a bar below it.
- Q: How do the tiles look? → A: As in COSMIC's app library: the icon large, the name below it, up to two lines, no
  frame (FR-003).
- Informed default (no question): new tabs are appended at the end and become active (existing `wm.tab.new`), so the
  tiles are in tab order; the platform's back action, where there is one (Android), closes an open overlay like
  Escape (FR-009).

## User Scenarios & Testing _(mandatory)_

### User Story 1 - A "+" opens a second tab (Priority: P1)

A user has a window with one tab. Next to the window buttons is a "+". They press it, the apps appear as tiles over the
window's content, they pick one, and it opens in a new tab of the same window and is shown. The "+" is now a button with the number 2.

**Why this priority**: it is the entry to everything else; without it a person cannot make a second tab at all.

**Independent Test**: open a window with one tab, press the "+", choose an app; the window shows the new app, and the
button shows 2.

**Acceptance Scenarios**:

1. **Given** a window with one tab, **When** it shows, **Then** a "+" button is beside the window buttons, and it is
   there for every app, also one that gives its header bar a lot of controls.
2. **Given** the "+", **When** pressed, **Then** an overlay over the window's content shows one tile per app that can be
   opened (icon and name, in the launcher's order), with an app that may be open only once and already is marked as
   open, and one that cannot open on this device shown disabled with its reason.
3. **Given** the tiles, **When** the user picks an app that can be open several times, **Then** a new tab with it opens
   in this window and is shown, the overlay closes, and the button now shows the number of tabs.
4. **Given** the tiles, **When** the user picks an app that may be open only once and is open (in any window), **Then**
   that tab is shown (its window is brought forward) and no second one is made (spec 020).
5. **Given** the tiles are open, **When** the user presses Escape, presses the "+" again or picks nothing, **Then** they
   close and nothing changes.
6. **Given** more apps than fit the overlay, **When** it opens, **Then** the grid scrolls vertically inside the overlay
   (wheel, touchpad, finger, scroll bar or keyboard) while the header and the tab button stay fixed, and the tiles keep
   their size.
7. **Given** 50 apps, **When** the user looks for one at the end, **Then** they reach it by scrolling alone and pick it,
   and the grid always opens at the top.

---

### User Story 2 - The number opens the tab overview (Priority: P1)

A user has a window with three tabs. The button beside the window buttons shows 3. They press it: tiles cover the
window's content, one per tab, with the app's icon, the tab's title and its place, the open one marked. They pick a
tile; the tab overview closes and that tab is shown.

**Why this priority**: it is the way to switch tabs by hand, the thing spec 090 took away.

**Independent Test**: with three tabs, press the number, choose another tile; the window shows that tab, the tab overview
is gone, and the number is still 3.

**Acceptance Scenarios**:

1. **Given** a window with two or more tabs, **When** it shows, **Then** the button beside the window buttons shows the
   number of tabs instead of the "+".
2. **Given** the number button, **When** pressed, **Then** an overlay with one tile per tab covers the window's content
   area, the header with the button and the window buttons stays visible, and the active tab's tile is marked by
   more than colour.
3. **Given** the tab overview, **When** the user picks a tile, **Then** that tab becomes the active tab, the tab overview
   closes, and the tab's own location and history are as they were.
4. **Given** the tab overview, **When** the user presses the number button again, presses Escape, or picks the active
   tab's tile, **Then** it closes and nothing changes.
5. **Given** the tab overview, **When** the user picks the "+" tile, **Then** the overlay shows the app tiles as for the
   "+" (User Story 1), a choice opens a new tab and closes the overlay, and Escape or the number button goes back to the
   tab overview if it came from there.
6. **Given** the tab overview is open, **When** the app's own header controls (spec 090) are used, **Then** they do not
   act: they are inert while the tab overview is open, since they belong to a tab the user is leaving.
7. **Given** a tab that was never shown in this session (it is restored but not opened yet), **When** its tile is
   picked, **Then** it opens and loads as it does when it is chosen by the `wm.tab.activate` action.

---

### User Story 3 - Closing tabs from the tab overview (Priority: P2)

In the tab overview each tile has a close button. A user closes a tab they no longer need. When only one tab is left, the
tab overview closes by itself and the number turns back into a "+".

**Why this priority**: a switcher that cannot close tabs piles them up; it comes after switching.

**Independent Test**: open four tabs, close two from the tab overview; the number shows 2; close one more; the tab overview
closes, the "+" shows, and the remaining tab is shown.

**Acceptance Scenarios**:

1. **Given** a tile, **When** its close button is used, **Then** the tab closes through the same path as the action
   `wm.tab.close`: every close guard (unsaved work) asks first, and declining leaves the tab.
2. **Given** the active tab is closed from the tab overview, **When** it is gone, **Then** another tab becomes active (the
   neighbour, as `wm.tab.close` decides today) and the tab overview stays open with the remaining tiles.
3. **Given** the tab overview with two tabs, **When** one is closed, **Then** the tab overview closes, the remaining tab is
   shown, and the button turns into the "+".
4. **Given** the close button, **When** it is used, **Then** the window itself does not close: a window closes only
   through its window buttons.

---

### User Story 4 - A tab that wants attention is seen (Priority: P2)

An extension in a background tab asks for the user's attention (spec 017). The number button shows a mark, and in the
tab overview the tab's tile shows it too. Spec 090 had to suspend that mark when it took the tab strip away; this gives it
a place again.

**Why this priority**: without it a tab that waits for the user is invisible; it follows the basic switching.

**Independent Test**: with two tabs, make the inactive one request attention; the number button shows a mark; open the
tab overview, the tile shows it; show that tab, the mark goes.

**Acceptance Scenarios**:

1. **Given** an inactive tab with attention, **When** the window shows, **Then** the number button carries a mark that
   is not only a colour and that has an accessible name.
2. **Given** the tab overview, **When** it shows, **Then** every tile of a tab with attention carries the same mark.
3. **Given** the app of the tab clears its own attention (the chat when its prompt is answered, an extension with
   `extension_tab_attention {active: false}`; showing the tab does not clear it by itself, and this spec does not
   change that), **When** it does, **Then** the marks go if no other tab has attention.

---

### User Story 5 - Keyboard, screen reader and phone (Priority: P2)

The button and the tab overview are fully operable without a mouse. On a phone the tiles are large enough to hit.

**Why this priority**: holzi runs on Android and is used with the keyboard; the tab overview must not be a mouse-only thing.

**Independent Test**: with the keyboard alone, reach the button, open the tab overview, move to a tile, choose it, and
close another tab; on a compact window the tiles are touch-sized.

**Acceptance Scenarios**:

1. **Given** the button, **When** reached with the Tab key, **Then** it announces what it does ("New tab" for the "+",
   "Show N tabs" for the number) and whether the tab overview is open.
2. **Given** the tab overview, **When** opened, **Then** focus moves into it, the arrow keys move between tiles, Enter
   picks the tile, a key (Delete) closes the focused tile's tab, Escape closes the tab overview and returns focus to the
   button.
3. **Given** the tab overview or the app tiles are open, **When** the user uses back or forward (Alt+Arrow, the mouse
   side buttons), **Then** the overlay acts as it does for Escape and nothing else happens: the history of the tab
   behind it does not move. (No shortcut for switching tabs exists today, and this spec adds none.)
4. **Given** a compact window, **When** the tab overview shows, **Then** the tiles use fewer columns, their close buttons
   are full touch targets (as spec 015, T050), and the layout does not need a horizontal scroll.

---

### User Story 6 - The launcher scales the same way (Priority: P3)

A user with 40 installed apps opens the launcher. Its app tiles scroll as behind the "+", in COSMIC's look; the tiles
for the voice assistant and for locking the vault stay in a fixed strip, outside the scrolling grid.

**Why this priority**: the launcher has the same scale problem; sharing the grid makes it one behaviour instead of two.

**Independent Test**: with 40 apps open the launcher; scroll to the last app and open it; the lock tile is visible
at every scroll position.

**Acceptance Scenarios**:

1. **Given** more apps than fit the launcher, **When** it opens, **Then** the app tiles scroll vertically as
   behind the "+" (FR-003a).
2. **Given** any scroll position, **When** the launcher shows, **Then** its own tiles (voice assistant, lock the vault)
   are visible in a fixed strip and are not part of the scrolling grid.
3. **Given** a tile, **When** right-clicked or long-pressed, **Then** it pins to or unpins from the dock as today (spec
   045, FR-017), and a scroll gesture never opens a context menu or an app.
4. **Given** the launcher opens an app, **When** it is picked, **Then** it opens a window as today (spec 015, FR-002),
   not a tab.

---

### Edge Cases

- **Many tabs**: the tab overview scrolls vertically inside the window's content area; the button shows "99+" from 100
  tabs up; the header measures the fixed part of its row again when the button's width changes (a "+", a number,
  "99+"); nothing else changes.
- **A very long title**: cut with an ellipsis; the full title is the tile's tooltip and part of its accessible name.
- **A tab whose app is gone or not usable on this device**: spec 015 drops it on restore; a tab that is open and
  becomes unusable shows its tile as it shows its tab today (the unavailable reason of the app).
- **The set of tabs changes while the tab overview is open** (an agent runs `wm.tab.new` or `wm.tab.close`, a tab closes
  itself): the tiles follow; the tab overview closes by itself when one tab is left.
- **The window loses focus, is minimized or is moved with the tab overview open**: the tab overview stays with the window; a
  minimized window comes back with it still open, unless it was closed in between.
- **Session restore**: the tab overview is never restored open; every tab and the active one come back as spec 022 says.
- **A close guard asks** (spec 015): the confirmation shows over the tab overview as it shows over a tab; declining leaves
  everything as it was. If other tabs close while the confirmation waits (an agent, another window), the check that the
  window keeps a tab is made again after the confirmation: a close from the overview that would leave no tab is not
  carried out, and the window stays (FR-012).
- **Two windows**: each window has its own button and its own tab overview; opening one does not change the other.
- **Singleton apps** (Settings): a second window's "+" shows its tile marked as open; picking it shows the existing tab.
- **Apps installed or removed while the tiles are open**: the tiles follow; the scroll position stays valid.
- **Everything fits**: no scroll bar is offered.
- **A scroll gesture on a tile**: it scrolls and never picks the tile; a press without movement picks it.
- **Reduced motion**: the tab overview opens without animation.
- **Dark and light scheme, accent colour, window hint** (spec 035): the button and the tiles follow them.
- **The header is very narrow** (spec 090): the button never collapses into the overflow menu; the window buttons and
  this button are the fixed part of the row, so the app's controls give way before it does.
- **Right-to-left layout**: the grid and the arrow keys follow the layout direction.

## Requirements _(mandatory)_

### Functional Requirements

**The button**

- **FR-001**: Every window MUST show one tab button beside its window buttons. It MUST belong to holzi, not to the app,
  MUST NOT be part of an app's header description, and MUST NOT move into the overflow menu of spec 090 (it counts
  with the window buttons as the fixed part of the header row). Like the window buttons it MUST NOT start a window drag
  or toggle maximize: it stops `pointerdown` and `dblclick`, so a quick double press does not maximize the window. It
  keeps the header's control height (32 px, spec 090 FR-005); on a compact window its hit area grows to a touch
  target through padding, without making the header taller.
- **FR-002**: With exactly one tab the button MUST be a "+" that opens the app tiles (FR-003). With two or more
  tabs it MUST show the number of tabs of the window, and pressing it MUST toggle the tab overview (FR-006). From 100
  tabs up it shows "99+".
- **FR-003**: The "+" MUST show every app that can be opened as a tile (the icon large, the name below it in up to two
  lines, no frame, as in COSMIC's app library), in the order and with the marks of
  the launcher (an attention dot, a disabled tile with its reason for an extension that cannot open here), as an
  overlay over the window's content. A choice opens a new tab of that app in this window through the action
  `wm.tab.new`; an app that may be open only once and already is MUST be marked as open and be shown instead of
  opened again (spec 020). The launcher's own tiles for the voice assistant and for locking the vault are not apps
  and MUST NOT appear.
- **FR-003a**: When the tiles do not fit the overlay, the grid MUST scroll vertically inside the overlay (wheel,
  touchpad, finger, scroll bar, keyboard) with the header and the tab button fixed; it MUST open at the top; a tile
  MUST keep its size and stay a full touch target (it is never shrunk to fit more); the arrow keys MUST move between
  tiles and scroll the focused tile into view; nothing is shown when everything fits.
- **FR-004**: The button MUST carry an accessible name that says what it does and how many tabs there are, MUST be
  reachable by keyboard, and MUST announce whether the tab overview is open.
- **FR-005**: The button MUST carry a mark, not only a colour, while any inactive tab of the window has attention
  (spec 017's `requestAttention`), and the mark MUST go when no tab has attention any more.

**The tab overview**

- **FR-006**: The tab overview MUST be an overlay over the content area of its window, one tile per tab of that window, in
  the order of the tabs. The header with this button and the window buttons MUST stay visible and operable. The
  active tab's tile MUST be marked by more than colour.
- **FR-007**: A tile MUST show the app's icon, the tab's title (as the tab shows it today, spec 020) and its place
  (the location's path), an attention mark when the tab has attention, and a close button. It MUST NOT show a picture
  of the tab's content.
- **FR-008**: Picking a tile MUST activate that tab through the action `wm.tab.activate` and close the tab overview; the
  tab keeps its location and history. A tab that is not opened yet loads as with the action.
- **FR-009**: The tab overview MUST end the way it was started and in these ways only: the same button, Escape, the
  platform's back action where there is one (Android), picking the active tab's tile, picking a tile, or the number of
  tabs falling to one. The app tiles end the same way, except that app tiles opened from the tab overview lead back to the tab overview
  (FR-011) and end it only from there. Focus MUST return to the button.
- **FR-010**: While the tab overview is open, the app's own header controls (spec 090), including the header's overflow
  menu button (an open overflow menu closes), MUST be inert, and the app's content MUST NOT receive pointer or keyboard
  input; the window buttons and this button MUST stay operable.
- **FR-011**: The tab overview MUST contain a "+" tile that shows the app tiles as FR-003 says; a choice closes the
  overlay and shows the new tab, and leaving the app tiles without a choice returns to the tab overview.
- **FR-012**: A tile's close button MUST close the tab through the action `wm.tab.close`, so every close guard asks
  first (spec 015) and declining leaves the tab. Closing MUST NOT close the window: with one tab left the tab overview
  closes and the button becomes a "+". A `wm.tab.new`, `wm.tab.activate` or `wm.tab.close` that fails MUST show a
  `toast.error` with a localized message; a close that a close guard declines is the user's own choice and shows
  nothing.
- **FR-013**: The tiles MUST follow the window's tabs live: a tab opened, closed or renamed by an action, an agent or
  the app itself appears, disappears or changes its tile while the tab overview is open.
- **FR-014**: The tab overview MUST scroll inside the content area when the tiles do not fit, and MUST be usable at the
  smallest window size an app allows.

**Keyboard and touch**

- **FR-015**: On opening, focus MUST move to the active tab's tile. The arrow keys MUST move between tiles (following the
  layout direction), Enter MUST pick the focused tile, Delete MUST close the focused tile's tab (FR-012), and
  Escape MUST close the tab overview.
- **FR-016**: While the tab overview or the app tiles are open, back and forward (Alt+Arrow, the mouse side buttons)
  MUST act like Escape and do nothing else; the history of the hidden tab MUST NOT move. This spec adds no shortcut
  for switching tabs (none exists today); that is a follow-up.
- **FR-017**: On a compact window the tiles of the tab overview and of the apps MUST use at most two columns (the
  launcher's two), the close buttons of the tiles MUST be full touch targets (44 × 44 px), and neither surface MUST need
  a horizontal scroll. The tab button itself keeps the header's height (FR-001).

**Model and persistence**

- **FR-018**: This spec MUST NOT change the tab model, the tab actions, the history, the close guards or the session
  restore. The tab overview is never saved and never restored open.
- **FR-019**: The attention mark of spec 015 that spec 090 suspended (FR-029 there) MUST work again through FR-005 and
  FR-007; the active-tab emphasis of spec 040 MUST be reflected by FR-006 (the active tile is marked), and the notes
  in specs 015, 020 and 040 that spec 090 added MUST be updated to say so.

**Texts and tests**

- **FR-020**: Texts that holzi adds MUST be in German and English in step.
- **FR-021**: E2E scenarios MUST cover opening a second tab from the "+", switching from the tab overview, closing from the
  tab overview down to one tab, the attention mark, and the launcher's scrolling grid with the fixed strip.

**The launcher**

- **FR-022**: The launcher MUST show its app tiles through the same grid component as the "+" (FR-003a), scrolling
  the same way. It MUST keep its drawer, the pinning of an app to the dock (spec 045), the attention
  dot and the disabled tile, and the opening of an app as a window; its own tiles for the voice assistant and for
  locking the vault MUST stay in a fixed strip outside the scrolling grid.

### Key Entities

- **Tab button**: the host-owned button beside the window buttons; a "+" (one tab) or the number of tabs (two or more),
  with an attention mark when an inactive tab asks for attention. Toggles the tab overview or opens the app tiles.
- **Tab overview**: the overlay of tiles over a window's content area; belongs to one window, is never saved.
- **Tab tile**: the representation of one tab in the tab overview (icon, title, place, attention mark, close button), plus
  one "+" tile.
- **App tiles**: the scrolling grid of openable apps behind a "+", the same tiles as the launcher's, shown as an overlay.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: A person can open a second tab in two presses (the "+", then an app, plus scrolling when the app is far down) and switch to any other tab of a window in two presses (the number, then a tile), without a keyboard
  shortcut.
- **SC-007**: With 50 installed apps, every app is reachable from the "+" by scrolling alone, no tile is smaller than a touch target, and the overlay opens in under 300 ms on the reference laptop.
- **SC-002**: With 50 tabs in a window, the tab overview opens and every tile is reachable (by scrolling) and can be picked
  or closed, and the window stays responsive: opening takes under 300 ms on the reference laptop.
- **SC-003**: The button and the tab overview are operable with the keyboard alone: 100 % of the actions of User Stories
  1–3 can be done without a mouse.
- **SC-004**: A tab that asks for attention is visible from the window's header without opening anything, in 100 % of
  the attention scenarios.
- **SC-005**: A window with several tabs comes back after a restart with every tab and its place as before, and the
  tab overview is closed (spec 022 scenarios still pass, spec 090 SC-007).
- **SC-006**: On a phone-sized window the tiles show in at most two columns, no tile or tile button is smaller than a
  touch target (the tab button gets its touch area through padding, FR-001), and nothing needs a horizontal scroll.

## Assumptions

- Spec 090 is merged first: this spec builds on its header, its fixed window-button area, its removal of the tab strip
  and its notes in specs 015, 020 and 040. The room for this button is the area beside the window buttons, which spec
  090 leaves free for it.
- Spec 090 removes the tab strip's `NewTabMenu`; this spec does not bring the list back. It reuses the launcher's tile
  (`wm/Launcher.vue`) by extracting the grid of app tiles into a component both use, which is the existing candidate
  to extend rather than a third variant. The launcher keeps its drawer, its pinning to the dock and its assistant and
  lock tiles and also scrolls its app tiles (Clarifications, FR-022).
- This spec amends spec 090: 090 calls the window buttons "the one part of the header that belongs to holzi" (its
  Decision 1 and Key Entities). The tab button is a second part that holzi owns, beside them. Once both specs are
  merged, 090's text is changed to say so (tasks.md, T035).
- No new dependency is needed: a scrolling grid needs none.
- Groups like COSMIC's app library (a bar below the grid with Library, Office, System and "+ Group", with editing and
  deleting) are a follow-up spec of their own; they need their own data and a decision on syncing them between devices.
  Typing to filter the apps is also not part of this spec; with 50 apps both are likely follow-ups.
- A picture preview of the tabs is a later step. The way to do it, if wanted, is showing the already open tab panels
  scaled down inside the tiles (it works for extension frames too); it needs all tabs to be mounted and a way to show
  tabs that were never opened, which is why it is not in this spec.
- The mouse side buttons and Alt+Arrow walk the history of a tab, not the tabs; the actions (`wm.tab.*`) are the other
  way to move between tabs and stay as they are.
- Moving a tab to another window, reordering tiles, tab groups and pinned tabs are out of scope.
