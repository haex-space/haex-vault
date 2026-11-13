# Feature Specification: The header bar belongs to the app

**Feature Branch**: `090-app-header-bar`

**Created**: 2026-11-11

**Status**: Draft

**Input**: User description: "Das Window-Design der Apps nochmal anpassen, um den Platz im Window-Header besser
auszunutzen. Heute sind im Header nur der Tab-Name und die Fensterbuttons. Mehr Menüs (Buttons und Inputs) sollen dort
Platz finden, auch von Erweiterungen. Vorbild ist COSMIC." (operator, 2026-11-11), refined in a brainstorming session
the same day. The decisions are below.

## Background

- **What the header holds today**: the title row of a window (spec 015, FR-007) carries the tab strip on the left and,
  on the right, the tab-list menu and the minimize, maximize and close buttons. Nothing else fits there.
- **What the apps do instead**: each system app starts its content with its own first row. Settings has the sidebar
  button and the search (spec 023, FR-004, FR-023), the file browser has the sidebar button, the bar of the panel
  (back, forward, path, list or grid, sort) and the switch for two panels (spec 044, FR-003, FR-004; spec 062). Contacts
  and passwords have the same kind of row. That row is a second bar under the title row that shows what COSMIC puts in
  its header bar: the window loses two rows of height where COSMIC uses one.
- **Extensions have no such place at all**: an extension runs in a sandboxed frame inside the content area (spec 017).
  It cannot reach the header, and it cannot be given a frame there without a second frame per window. Its own first row
  costs the same height as a system app's.
- **The protocol is shared**: extensions talk to holzi over the protocol of the vault SDK that haex-vault 2.x speaks
  too (spec 017, ADR-0004). A new message must be additive: an extension that sends it must keep working in
  haex-vault 2.x, which ignores it.
- **The path bar already collapses**, but by the width of the screen, not of the window (spec 044, `max-md`). A window
  is resizable, so the header needs to react to its own width.

## Decisions

The operator decided on 2026-11-11:

1. **The whole header bar belongs to the app.** The tab strip leaves the header. The app decides what the bar shows;
   holzi keeps only the window buttons.
2. **Tabs stay in the model.** Session restore, the tab actions (`wm.tab.*`), history and the mouse side buttons keep
   working. A window shows only its active tab. Where a user switches tabs by hand comes in spec 091 (the direction, decided on 2026-11-11: a host-owned
   button next to the window buttons, a "+" with one tab and the number of open tabs with two or more, opening a tile
   overview of the tabs, like a mobile browser); until then, a window with several tabs has no visible tab strip.
   This gap is known and accepted.
3. **Declarative, rendered by holzi.** An app (or extension) hands holzi a description of its header controls; holzi
   draws them with its own building blocks and sends events back. There is no second frame for extensions. A free-form
   frame slot is not part of this spec.
4. **One description for both kinds of app.** System apps and extensions use the same description; only how they hand it
   over and receive events differs. Extensions are never worse off than system apps.
5. **The app decides how its header collapses.** As the window gets narrower, each control collapses by what the app
   said about it: the order, and the form (into one popover menu, to its icon only, narrower, or not at all while
   avoidable). The popover works like the path bar of the file browser does for the folders above the open one. holzi
   only guarantees that nothing is cut off and that the window buttons stay.
6. **Pilot**: this spec ships the mechanism, the extension interface, Settings and the file browser. Contacts,
   passwords, chat, marketplace and the other system apps follow one by one in their own specs.
7. **No backward compatibility**: holzi has no users yet (as in the specs before). The first row of Settings and of the
   file browser goes away; there is no setting to bring it back.

## Clarifications

### Session 2026-11-11

- Q: How is the start area kept visible on a narrow window? → A: It is not limited by holzi. Every control says for
  itself how and in which order it collapses (order, and form: `overflow`, `icon`, `shrink` or `keep`), across all three
  areas; holzi's safety net is that nothing is cut off and the window buttons and the overflow menu stay visible.
- Q: When does a search field tell the extension about typing? → A: On every change, merged by holzi to the latest text
  at most about every 100 ms, at once on Enter and on losing focus (FR-012).
- Q: What does the file browser's header show with two panels? → A: Only the sidebar button and the switch for two
  panels; each panel keeps its own bar (spec 062), so no control appears twice (FR-026). (Plan, 2026-11-11: the first
  wording had the header follow the focused panel, which showed that panel's controls twice.)
- Q: Which icons may an extension show in the header? → A: Only icons it ships itself (a file of its own signed
  bundle, or an inline picture in a development version), each at most 256 KiB, with no limit on how many or on their total, because the files ship in the bundle and
  nothing is loaded from outside; never a name from holzi's icon set, a URL or markup (FR-009, FR-022). (Operator, 2026-11-11; replaces
  the plan's first idea of a closed list of holzi icons.)
- Q: Do header controls get keyboard shortcuts in this spec? → A: No. They are operable by keyboard (FR-006); a shortcut
  such as "focus the header search" is a follow-up spec. Existing shortcuts of Settings and the file browser stay.

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Settings has its search in the header (Priority: P1)

A user opens Settings. The title row shows the sidebar button, the search and the window buttons, in one row, as COSMIC
Settings does. The content starts right under that row; the extra row that held the sidebar button and the search is
gone.

**Why this priority**: it is the model for everything else and the smallest app that shows the idea. If it does not
look and feel right here, the mechanism is wrong.

**Independent Test**: open Settings at its default size; there is exactly one bar above the content, with the sidebar
button, the search and the window buttons; search for a field and open a hit; collapse and show the sidebar.

**Acceptance Scenarios**:

1. **Given** a Settings window, **When** it opens, **Then** the header shows the sidebar button at the start, the
   search in the middle and the window buttons at the end, and no row of Settings' own sits between the header and the
   content.
2. **Given** the search is used, **When** the user types, presses Enter, presses Escape or uses the clear button,
   **Then** it behaves as the search in spec 023 FR-023 does today (hits in the sidebar, Enter opens the first hit,
   Escape clears the field first and then returns the focus to where it was, as the search is always an open field).
3. **Given** the sidebar button, **When** it is used, **Then** the sidebar toggles and the button's state and label
   follow it.
4. **Given** a Settings window the user drags by an empty part of the header, **When** they drag or double-click,
   **Then** it moves or maximizes like any window.

---

### User Story 2 - A narrow window keeps everything reachable (Priority: P1)

A user makes a window narrower. The controls of the app do not get cut off or overlap the window buttons; each one
gives way as its app said: some move into a menu button, some lose their text and keep the icon, some get shorter. The
app decides which goes first. Opening the menu shows what moved, with names. Making the window wider again puts
everything back.

**Why this priority**: without it more controls in the header only work on large windows. A phone is always narrow.

**Independent Test**: resize a window from its minimum width to the full screen and back; at every width check that no
control is clipped, that the window buttons stay visible and that every control is reachable either in the row or in
the menu.

**Acceptance Scenarios**:

1. **Given** controls that do not fit, **When** the window is narrowed, **Then** they collapse one at a time in the
   order the app gave (the lowest first, across start, middle and end), each in the form the app gave, and the menu
   button shows only while at least one control is inside.
2. **Given** the menu is open, **When** the user picks a button, toggle, choice or menu entry, **Then** it acts exactly
   as it does in the row, and the menu closes (except a search field, which stays open while it has focus).
3. **Given** a control that is to shrink (a search field, a path, a title), **When** space runs short, **Then** it gets
   narrower down to its smallest width (a title is cut with an ellipsis) and only then moves into the menu.
4. **Given** a button with text that is to keep its icon, **When** space runs short, **Then** it loses the text, keeps
   the icon and the accessible name, and its tooltip shows the text.
5. **Given** controls the app marked to stay, **When** space runs short, **Then** the others collapse first; only when
   nothing else is left to collapse and the rest still does not fit, holzi collapses them too, so that nothing is cut
   off.
6. **Given** any width, **When** the user looks at the header, **Then** the window buttons (on a compact window: the
   close button) are visible, and so is the menu button while something is inside.
7. **Given** the window is made wide again, **When** space is there, **Then** the controls return in the reverse order
   and form.

---

### User Story 3 - An extension puts controls in the header (Priority: P1)

The author of an extension describes a few controls for the header (a toggle for its sidebar, a search, a menu with
actions). holzi draws them in the header of the extension's window. When the user uses one, the extension is told and
reacts, changes what it shows or sends an updated description.

**Why this priority**: this is the point of the spec for extensions: the first row costs them height today and cannot
look like the rest of holzi.

**Independent Test**: a reference extension sends a description with every kind of control; check that each appears,
that its events arrive, that sending a new description changes the header and that a second send does not flicker or
reset what the user has typed.

**Acceptance Scenarios**:

1. **Given** an extension's frame that is ready, **When** it sends a description, **Then** the header of its window
   shows the controls in the three areas (start, middle, end) in the order given.
2. **Given** a control the user uses, **When** it is clicked, toggled, typed into, submitted or a choice is made,
   **Then** the extension receives an event that names the control and carries the value where there is one.
3. **Given** the extension sends a new description, **When** it replaces the old one, **Then** the header shows only
   the new one; a search field that the user is typing in keeps its text and caret unless the extension's value differs
   from the one it was last told.
4. **Given** an extension that sends no description, **When** its window shows, **Then** the header shows the title and
   the window buttons only.
5. **Given** a window with several tabs, **When** the active tab changes, **Then** the header shows the description of
   the new active tab, and the old tab's description comes back when it is active again.
6. **Given** the same extension in haex-vault 2.x, **When** it hands over a description, **Then** the SDK call does
   nothing and reports that the host does not support it (it does not throw), so the extension keeps running.
7. **Given** an extension whose frame reloads or is closed, **When** that happens, **Then** its header controls
   disappear until a new description arrives.

---

### User Story 4 - The file browser has its bar in the header (Priority: P2)

A user opens the file browser. With one panel, the header carries the sidebar button, back and forward, the path, list
or grid, sort and the switch for two panels. With two panels the header carries only what belongs to the whole window
(the sidebar button and the switch for two panels); each panel keeps its own bar inside it as today (spec 062), so no
control appears twice.

**Why this priority**: it is the demanding case for the catalog (a path that collapses, a segmented choice, a menu) and
proves the mechanism beyond a search field. It follows Settings because a simple app should prove the idea first.

**Independent Test**: use every control of the file browser's first row in the header, with one and with two panels,
and compare the result to the behaviour in specs 044 and 062; narrow the window until the path collapses.

**Acceptance Scenarios**:

1. **Given** one panel, **When** the file browser opens, **Then** the controls of the first row (spec 044 FR-003,
   FR-004) are in the header and no first row of its own remains.
2. **Given** the switch for two panels is turned on, **When** the second panel opens, **Then** the panel's bar leaves
   the header and stands in each panel (spec 062), the header keeps the sidebar button and the switch, and turning
   the switch off puts the one panel's bar back into the header.
3. **Given** the path in the header, **When** the window is narrow, **Then** the path becomes the open folder's name
   with the menu of the folders above it, editing and reload (as spec 044 does on a small screen), decided by the
   width of the window, not of the screen.
4. **Given** every control that was in the first row, **When** used, **Then** it runs the same catalog action it runs
   today.

---

### User Story 5 - Tabs are out of the header but not lost (Priority: P2)

A user who had several tabs in a window finds the tab strip gone from the header. The window shows its active tab.
Nothing was closed or forgotten: after a restart the same tabs are back, and the keyboard shortcuts, the mouse side
buttons and the agent still switch, open and close tabs.

**Why this priority**: it keeps the cost of decision 1 at "no strip", not "no tabs", and it protects session restore.

**Independent Test**: build a window with three tabs, restart, check all three are restored with their history, switch
with `wm.tab.activate`, back with a side button, close one with `wm.tab.close`.

**Acceptance Scenarios**:

1. **Given** a window with several tabs, **When** it shows, **Then** no tab strip and no tab-list menu is in the header,
   and the active tab's content fills the window.
2. **Given** a restart, **When** the session is restored, **Then** every tab, its location, its history and which tab
   was active are as before (spec 022).
3. **Given** the tab actions, **When** run by the agent, by a shortcut or by a side button, **Then** they act as before
   and the window shows the tab they make active.
4. **Given** a tab's close guard (unsaved work), **When** the tab is closed through an action, **Then** the guard still
   asks (spec 015).

---

### User Story 6 - The header is safe from the app inside (Priority: P2)

The header is part of the window frame, outside the sandbox of an extension. A buggy or hostile extension cannot make
it show anything but plain controls, cannot make it flood the window, and cannot reach another window's header.

**Why this priority**: an extension that could draw there could imitate holzi's own buttons or hide the window buttons.

**Independent Test**: send forged, oversized and malformed descriptions from a test frame; the header stays as it was,
the window buttons stay in place and nothing throws.

**Acceptance Scenarios**:

1. **Given** a description with a text that looks like markup, **When** shown, **Then** it appears as text.
2. **Given** a description over the limits (too many controls, too long a text, too large in all), with a missing
   accessible name, with the same identifier twice or in the wrong shape, **When** it arrives, **Then** it is dropped as
   a whole, the previous one stays, and the extension's developer console says why (spec 017, US12).
3. **Given** a control type holzi does not know, **When** it arrives in an otherwise valid description, **Then** that
   control is left out and the rest is shown.
4. **Given** a frame of one tab, **When** it sends a description or an event for another tab, **Then** it changes
   nothing outside its own tab.
5. **Given** any description, **When** shown, **Then** the window buttons are neither hidden, covered nor moved.
6. **Given** an extension's icon that is not a file of its own bundle, a name from holzi's icon set, a URL, a file over
   the size limit or a type that is not a picture, **When** the description
   arrives, **Then** it is refused as a whole like any other violation (FR-021) and the previous header stays.
7. **Given** a picture in SVG form, **When** shown, **Then** it is drawn as an image only: no script in it runs and it
   loads nothing from outside.

---

### Edge Cases

- **A frame that is not ready yet**: it has no SDK channel, so it cannot send a description; until the first one
  arrives, the header shows the title and the window buttons only.
- **An event for a control that is gone**: no event is sent; a click on a control while a new description replaces it
  goes to the new one if the identifier still exists, otherwise nowhere.
- **A tab that is not active**: its description is kept but not shown; a background tab's description does not appear in
  another tab's header.
- **A window with no title and no controls**: the header is still one row with the window buttons, so the window can be
  moved.
- **A compact window or a phone**: the same rules apply with only the close button on the end; the narrow-width rules
  of User Story 2 apply from the start.
- **Controls that depend on state** (a toggle on, a disabled button, a search with text): shown in the menu the same way
  as in the row.
- **A form that does not fit the control**: `icon` on a control without an icon, or `shrink` on a button, counts as
  `overflow`; the description is not rejected.
- **The same order twice**: controls with the same order collapse from the far side of the end area inward first, then
  the middle, then the start area.
- **An app that marks everything `keep`**: nothing collapses by the app's choice; when space runs out holzi collapses
  them anyway (FR-014), in the order of a control that says nothing.
- **A very long title in the middle**: cut with an ellipsis; the full text is the window's accessible name and a
  tooltip.
- **Two windows of one extension**: each window has its own header description; neither changes the other.
- **An icon the extension names but its bundle does not hold** (a wrong path, a file still being transferred): the
  description is refused as a whole with the path in the message (FR-021).
- **Many icons**: an extension may use as many icons as it has controls and menu entries; the same file on several
  controls is read and sent once.
- **A one-colour icon in a dark scheme**: drawn as it is unless the control asks for the primary colour; the extension
  decides, holzi does not guess.
- **Dark and light scheme, accent colour, window hint** (spec 035): the header follows them like the rest of the window;
  the focused window's hint still shows.
- **Right-to-left text** in a title or a field: the order of areas follows the layout direction.
- **An app's own description changing very often** (a search that echoes every key): the header applies it without
  losing focus or caret.
- **Settings narrow-window and sidebar behaviour** (spec 023): the sidebar's collapse on a narrow window keeps working;
  the sidebar button in the header stays the way to show it.

## Requirements _(mandatory)_

### Functional Requirements

**The header**

- **FR-001**: A window's title row MUST be one header bar made of a start area, a middle area that takes the free space,
  an end area and the window buttons, in that order, and MUST NOT contain the tab strip or the tab-list menu.
- **FR-002**: The window buttons MUST stay on the far end, belong to holzi and not to the app, and keep the rules of
  spec 015 (a compact window shows only the close button). Closing still runs every close guard.
- **FR-003**: Every free part of the header MUST move the window on drag and toggle maximize on double-click where the
  window allows it today (spec 015); controls in the header MUST NOT start a drag.
- **FR-004**: When the middle area is empty, the header MUST show the title of the active tab (as the tab shows it
  today). When the app has controls there, the title MUST stay as the window's accessible name and as a tooltip on the
  header.
- **FR-005**: The header MUST have one fixed height that is not larger than the title row's height today, so that an
  app that moves its first row into the header gains that row's height.
- **FR-006**: The header MUST be one toolbar for assistive technology: every control reachable and operable by
  keyboard, in the order start, middle, end, with a visible focus.

**The description**

- **FR-007**: An app MUST be able to describe its header as three lists of controls (start, middle, end; the keys of the
  description are `start`, `center` and `end`). Every new description MUST replace the previous one completely.
- **FR-008**: The first catalog MUST contain these controls and no others: **button** (icon and/or text, disabled,
  tooltip), **toggle** (a button with an on and off state), **search** (a text field with a search icon, a clear button,
  value and placeholder), **menu** (a button that opens a list of entries with icon, label, disabled and separators),
  **choice** (a small group of exclusive options, for example list or grid), **breadcrumb** (path segments; choosing one
  goes there; it may be editable, turning into a text field with the whole path as in spec 044), **title** (a text) and **separator**. More types MAY be added later without breaking an app that does not
  use them.
- **FR-009**: Every control except a title and a separator MUST carry an accessible name (`label`), also when it shows
  text; a control without one MUST be rejected (FR-021). A system app names its icons from the set holzi ships. An extension shows only icons it ships
  itself: a picture file of its own bundle, or (for a development version, which has no stored bundle) a picture given
  inline (an installed extension MUST NOT hand over an inline picture); it MUST NOT name an icon from holzi's set, point to a URL or hand over markup. A picture is a small image
  (PNG, JPEG, WebP, GIF or SVG), at most a few hundred kilobytes each (FR-022); a control MAY ask for its picture to be
  drawn in the primary colour, so that a one-colour icon follows the theme. A system app's icon is always drawn in the
  primary colour (amended 2026-11-12), and a toggle shows its state by its label and `aria-pressed`, not by a pressed
  background.
- **FR-010**: Every control MUST have an identifier, unique within the description, that comes back in its events.
- **FR-011**: Every control MUST be able to say how it collapses: an **order** (a number; the lowest collapses first,
  counted across all three areas) and a **form**: `overflow` (moves into the overflow menu; the default), `icon` (a button
  with text loses the text and keeps the icon; only for a control that has an icon), `shrink` (gets narrower down to a
  smallest width, then moves into the menu; for search, breadcrumb and title) or `keep` (is collapsed only when
  nothing else is left, FR-014). A control that says nothing is `overflow` with an order by its place, after every control that gives
  an order of its own: the end area from its far side, then the middle, then the start area from its far side.

**Events and state**

- **FR-012**: Using a control MUST tell the app: a button was clicked, a toggle was switched (with its new state), a
  search was typed in (with the text) or submitted, a menu entry or a choice was selected (with which), a breadcrumb
  segment was picked (with which). Each event names the control. A search field MUST report every change of its text,
  but holzi MUST merge quick changes so that the app gets the latest text at most about every 100 ms, and at once on
  Enter and when the field loses focus; the app does not debounce itself. A disabled control, and a disabled menu entry,
  MUST NOT send an event.
- **FR-013**: A search field MUST keep what the user typed between descriptions. A new description changes it only when
  its value differs from the value it last handed over; a description that repeats the text the field already has MUST
  NOT move the caret or drop focus.

**Narrow windows**

- **FR-014**: When the header's controls do not fit, the header MUST collapse them one at a time in the order and form
  the app gave (FR-011), controls marked `keep` last. holzi MUST NOT hide or reorder anything for its own reasons
  except as the safety net: the window buttons and the overflow menu button (while it holds something) MUST always be
  visible, and when only `keep` controls are left and still do not fit, holzi MUST collapse them into the menu too, so
  that no control is ever cut off or overlaps another.
- **FR-015**: The menu MUST show each moved control with its name and state and MUST act exactly as the control does in
  the row. It MUST be usable by keyboard and MUST show only while at least one control is inside.
- **FR-016**: The decision MUST depend on the width of the header (of the window), not on the width of the screen, and
  MUST change live while the window is resized, without moving the focus of a control that stays.
- **FR-017**: Controls that come back into the row MUST return in the reverse of the order and form in which they
  collapsed, and keep their place in their area.

**Extensions**

- **FR-018**: The extension interface MUST offer an extension two additions to the protocol of the vault SDK: handing
  over a description (FR-007) and receiving the events (FR-012). Both MUST be additive: an extension that does not use
  them is unchanged, and an extension that does keeps running in haex-vault 2.x, where the call has no effect and the
  SDK says so without an error.
- **FR-019**: A description MUST belong to the tab of the frame that sent it, and MUST show only while that tab is the
  active tab of its window. A frame MUST NOT be able to set, read or trigger the header of another tab.
- **FR-020**: When a frame reloads, closes or is removed, its description MUST be dropped.
- **FR-021**: A description MUST be dropped as a whole, leaving the previous one in place, when it is not the expected
  shape, has a control without a required accessible name, repeats an identifier, or exceeds the limits (FR-022). A
  control of an unknown type MUST only be left out. Every drop MUST be explained in the extension's developer console.
- **FR-022**: The limits MUST bound the number of controls, the length of every text, the number of menu entries and the
  size of a description as a whole, and the size of one icon file (a few hundred kilobytes); there is no limit on
  the number of icons or on their total size; the plan fixes the numbers, which MUST be generous for the pilots and tested at the
  boundary.
- **FR-023**: Texts in a description MUST be shown as plain text only; a text with a control character (C0, C1) or a
  bidi-format character MUST be refused as a whole (FR-021). An extension's header MUST NOT need a new
  permission, because it can change nothing outside its own window's header.

**System apps**

- **FR-024**: A system app MUST describe its header with the same description as an extension (FR-007 to FR-011), and
  receive the same events, through a way that fits a Vue app.
- **FR-025**: A system app's header control MUST run the same catalog action as the control it replaces (spec 020
  FR-024), so shortcuts, the agent and the tests keep a single path.
- **FR-026**: Settings MUST move its first row (sidebar button, search) into the header and the file browser its first
  row (sidebar button, the panel's bar with one panel, the switch for two panels). With two panels the header MUST show
  only the sidebar button and the switch, and each panel's own bar MUST stay in its panel, so that no control appears
  twice (Clarifications).
- **FR-027**: The other system apps are not changed by this spec. An app that gives no description keeps its own first
  row and the header shows the title and the window buttons.

**Tabs**

- **FR-028**: Tabs MUST stay in the model: the tab actions, history, mouse side buttons, close guards, the model's attention
  flag and session restore MUST work as before (what shows it is FR-029). The window MUST show only its active tab. Nothing in this spec removes or
  renames a tab action.
- **FR-029**: The tab-list menu and the active-tab emphasis of spec 040 are removed from the header with the tab strip;
  the requirement that an inactive tab with attention is shown (spec 015) is suspended until spec 091 (tab switching) is
  done, and this MUST be noted in those specs.

**Texts and tests**

- **FR-030**: Texts that holzi adds MUST be in German and English in step. An app's own control texts come from the app.
- **FR-031**: The E2E scenarios that refer to the removed first rows, the tab strip or the tab-list menu MUST be moved to
  the header, and a scenario MUST cover an extension's header end to end.

### Key Entities

- **Header description**: what an app hands holzi for its window's header: three ordered lists of controls (start,
  middle, end). It belongs to one tab and replaces the previous one whole.
- **Control**: one entry of the catalog (button, toggle, search, menu, choice, breadcrumb, title, separator) with an
  identifier, an accessible name where it has no text, its state and how it collapses (order and form, FR-011).
- **Header event**: what holzi tells the app when a control is used: the control's identifier, what happened, and the
  value where there is one.
- **Overflow menu**: the menu button that holds the controls that did not fit, shown only while it holds one.
- **Window buttons**: minimize, maximize or restore, close; the one part of the header that belongs to holzi.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: Settings and the file browser each have one bar above their content instead of two; at the same window
  size their content gains at least the height of the removed row.
- **SC-002**: At every window width from the app's minimum to the full screen (checked at no fewer than ten widths),
  no control is clipped or overlaps another or the window buttons, every control is reachable in the row or in the
  menu, and controls collapse in the order and form their app gave in 100 % of the checked widths.
- **SC-003**: A reference extension needs one message to get a working header with every control of the catalog, and the
  same extension runs in haex-vault 2.x without an error, the call reporting that the host does not support it.
- **SC-004**: 100 % of a test set of forged, oversized, wrongly shaped, duplicated and cross-tab descriptions and events
  change nothing outside the sender's own header; the window buttons stay in place and no error reaches the user.
- **SC-005**: Every control of the header can be reached and used with the keyboard alone, in the row and in the menu.
- **SC-006**: A control's effect (the event reaching the extension, or the action of a system app) shows within one
  tenth of a second on the reference device; the `input` events of a search field are exempt, as FR-012 merges them
  for about 100 ms.
- **SC-007**: A window with several tabs comes back after a restart with every tab, location, history and the active tab
  as before, in 100 % of the restore scenarios of spec 022.
- **SC-008**: A user who knows COSMIC Settings finds the sidebar button and the search of holzi's Settings where they are
  there, without being told (checked by the operator by hand).

## Assumptions

- The operator reviews the pilots by hand for look and feel (the COSMIC reference screenshot is the model for Settings);
  an automated test cannot judge it.
- A change in the vault SDK (the two additions of FR-018) is needed; the SDK is a separate repository and its change is
  made and published there. holzi works without the SDK change (an extension simply has no header yet).
- Spec 091 builds the way to switch tabs by hand (a button beside the window buttons and a tile overview, see Decision
  2). This spec leaves the hooks (the tab model and actions) as they are and keeps the room beside the window buttons
  free for that button.
- The catalog is deliberately small. Sliders, date fields, free layout and a frame of the extension's own in the header
  are out of scope; a type that is missing is added in a later spec, additively.
- Shortcuts for header controls (for example focusing the search) are not part of this spec (Clarifications); the
  follow-up has to settle collisions with an extension's own shortcuts.
- The numbers of FR-022 and the exact height of FR-005 are decided in the plan, within the rules above.
- Agents do not see the header's controls as tools of their own; the catalog actions stay the path for them (FR-025).
- A phone is a compact window (spec 015, FR-036); the header rules apply there with its narrow-width case from the start.
- Contacts, passwords, chat, the marketplace and the other system apps keep their own first row until their own spec
  moves it (FR-027).
