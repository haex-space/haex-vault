# Feature Specification: The headers of Contacts, Passwords, Wallet and the Marketplace

**Feature Branch**: `093-app-headers`

**Created**: 2026-11-13

**Status**: Draft

**Input**: User description: "Header der Kontakte-, Passwörter-, Wallet- und Marktplatz-App: die erste Zeile jeder App
(Seitenleisten-Schalter, Suche, Filter, Neu-Menü, Menüs) zieht in den App-eigenen Window-Header aus Spec 090. Ein PR pro
App. Chat folgt getrennt. Menü-Einträge mit Häkchen (checked) und das Überlauf-Panel am Fenster existieren bereits."

## Background

- **Spec 090 built the header and moved two apps into it.** The header bar of a window belongs to its app; the app hands
  holzi a description of its controls and holzi draws them, collapses them when the window is narrow and keeps the
  window buttons. Settings and the file browser were the pilots. Contacts, passwords, chat, the marketplace and the
  other system apps were left "to follow one by one in their own specs" (spec 090, Decisions 6).
- **These four apps still carry a first row of their own** under the title row of the window, so each loses the height
  the header was meant to give back:
  - **Passwords** (spec 034): the sidebar button, the search, the tag filter, "+" (new entry or folder) and a menu
    (generator, import, tags, settings).
  - **Contacts** (spec 051): the sidebar button, the search and "+" (new contact, read a business card in).
  - **Wallet** (spec 092): the sidebar button, the search, the list filters (sort, kind, hide expired) and "+" (new pass).
  - **Marketplace** (spec 069): the search, a category choice, a marketplace choice (only with more than one switched
    on) and "Refresh". It has no sidebar.
- **What the header can do now that it could not when the pilots were built** (spec 090 and its follow-ups):
  a menu entry can be **checked** (a check mark at the end of the row), the **overflow menu is a panel on the window**
  that fills a narrow window and never stays open together with a sidebar, a **choice** is a segmented group, and a
  tab with a development version of an extension keeps its mark.
- **Chat is not part of this spec.** Its sidebar and its layout differ from these four apps; it follows on its own.

## Decisions

Taken in the session of 2026-11-13, from the pilots' look and the operator's feedback on them:

1. **The same layout as the pilots.** The sidebar button is the first control (never collapses, form `keep`). The search
   sits in the middle and is the first control to give way (it shrinks, then moves into the overflow panel). The
   actions of the app sit at the end. Settings is the model (`spec 090`, US1).
2. **Everything the row did stays possible** in the header: every button, menu entry and filter of today has a place in
   the header or, when the window is narrow, in the overflow panel. Nothing is removed, nothing is added.
3. **A state is a check mark, not a highlight.** A filter or a sort that is on is shown by the check mark at the end of
   its menu entry, as in the file browser's sort menu. A control no longer gets a coloured background when it is on
   (the operator's feedback on spec 090's toggle).
4. **One app, one pull request.** The four apps are independent. Order: Passwords first (the largest row, it proves the
   filter and the dialogs), then Contacts and Wallet (the same pattern), then the Marketplace (no sidebar, choices
   instead).
5. **The first row goes away.** As in spec 090, no setting brings the old row back (no backward compatibility, as in
   the specs before).

## Clarifications

### Session 2026-11-13

- Q: How does an active filter show in the header? → A: A small dot on the icon of the menu control (no background, no text).
- Q: Where is the dot when the filter has collapsed into the overflow panel? → A: On the overflow button and on the control's row in the panel.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Passwords: the bar is in the header (Priority: P1)

The user opens the password manager. The window's header shows the sidebar button at the left, the search in the middle
and, at the right, the tag filter, "+" and the menu, then the window buttons. There is no row under the header: the list
starts directly below it. Everything works as before: typing in the search narrows the list, the tag filter shows the
entries of a tag, "+" creates an entry or a folder, the menu opens the generator, the import, the tags and the
settings. When an entry is open, the search, the tag filter and "+" leave the header (as they leave the row today) and
the sidebar button and the menu stay.

**Why this priority**: the largest of the four rows and the one with most to prove: a filter with many entries, a "+"
menu, a menu whose entries open dialogs.

**Independent Test**: open the password manager in a window of 1000 px and in one of 460 px; use the search, the tag
filter, "+" and the menu in each; open an entry and check which controls remain.

**Acceptance Scenarios**:

1. **Given** the password manager is open and no entry is, **When** the user looks at the window, **Then** the header
   holds the sidebar button, the search, the tag filter, "+" and the menu, and no row of the app sits under the header.
2. **Given** the list is shown, **When** the user types in the search, **Then** the list narrows as before and the
   text is kept in the place's address as before (a step back in the history restores it).
3. **Given** a tag exists, **When** the user opens the tag filter and picks it, **Then** the list shows its entries,
   the picked tag has a check mark and so does "all tags" when none is picked.
4. **Given** the user opens "+" or the menu, **When** they pick an entry that opens a dialog (a new folder, the tags,
   the settings), **Then** that dialog opens as before and, when it closes, the focus returns to the header.
5. **Given** the window is narrowed, **When** the row no longer fits, **Then** the search shrinks first, then the filter,
   the menu and "+" move into the overflow panel (in that order); the sidebar button and the window buttons stay.
6. **Given** an entry is open, **When** the user looks at the header, **Then** the search, the tag filter and "+" are
   gone and the sidebar button and the menu remain.

---

### User Story 2 - Contacts: the bar is in the header (Priority: P2)

The address book shows the sidebar button, the search and "+" (a new contact, reading a business card in) in the
header. While a contact is open or a business card is being read, the search and "+" leave the header.

**Why this priority**: the same pattern as US1 with fewer controls; it confirms the pattern holds for a second app.

**Independent Test**: open the address book; search; use "+" for both entries; open a contact and check the header.

**Acceptance Scenarios**:

1. **Given** the address book is open, **When** the user looks at the window, **Then** the header holds the sidebar
   button, the search and "+", and no row of the app sits under it.
2. **Given** the user is in a group, **When** they choose "new contact" from "+", **Then** the new contact starts in
   that group, as today.
3. **Given** a contact is open, **When** the user looks at the header, **Then** only the sidebar button remains.

---

### User Story 3 - Wallet: the bar is in the header (Priority: P2)

Wallet shows the sidebar button, the search, the list filters (sort by date or title, by kind, hide expired) and "+"
(new pass) in the header. While a pass is open, the search, the filters and "+" leave the header.

**Why this priority**: it needs the checked entries in three groups (sort, kind, hide expired) in one menu.

**Independent Test**: open Wallet; pick a kind, switch the sort, hide expired passes; check the marks; narrow the window.

**Acceptance Scenarios**:

1. **Given** Wallet is open, **When** the user opens the filters, **Then** the current sort, the current kind and
   "hide expired" (when on) each have a check mark at the end of their entry.
2. **Given** a filter is on, **When** the user looks at the header, **Then** it can tell a filtered list from an unfiltered
   one (see FR-008: a dot on the filter icon).
3. **Given** a pass is open, **When** the user looks at the header, **Then** the search, the filters and "+" are gone.

---

### User Story 4 - Marketplace: the bar is in the header (Priority: P3)

The marketplace shows the search in the middle and, at the right, the category choice, the choice of marketplace (only
when more than one is switched on) and "Refresh". There is no sidebar button. While the catalogue loads, "Refresh" is
disabled.

**Why this priority**: no sidebar, and two long choices (categories with their counts, marketplaces) that are not
filters of a list but views of the catalogue.

**Independent Test**: open the marketplace with two marketplaces switched on; pick a category and a marketplace; refresh.

**Acceptance Scenarios**:

1. **Given** the marketplace is open, **When** the user looks at the window, **Then** the header holds the search, the
   category choice, "Refresh" and, with two or more marketplaces on, the marketplace choice; no row sits under it.
2. **Given** the category choice is open, **When** the user looks at it, **Then** every category is listed with its count
   and the current one has a check mark.
3. **Given** the catalogue is loading, **When** the user looks at "Refresh", **Then** it is disabled.
4. **Given** the window is narrow, **When** the row does not fit, **Then** the choices and "Refresh" move into the overflow
   panel before the search does.

---

### Edge Cases

- **A dot in a narrow window.** When the filter has moved into the overflow panel, the overflow button shows the dot
  (FR-008); when the filter is back at its default, the dot goes.
- **A long list in a menu.** The tag filter and the category choice can list many entries. The menu scrolls; it never
  grows past the window.
- **No tags yet.** The tag filter has nothing to show today and is not drawn; it is not drawn in the header either.
- **Search text and history.** A step back or forward in the history puts the stored search text into the header's
  search; text the user is typing is not overwritten by an older value coming back (spec 090, FR-013).
- **A dialog opened from a control.** The dialogs that the old row owned (new folder, tags, settings of the password
  manager) must outlive the control that opened them: closing the overflow panel or collapsing the control while a
  dialog is open does not close it.
- **A control that appears and disappears.** When the open entry changes, the controls that come and go do so without
  moving the ones that stay (spec 090, FR-016).
- **Two windows of the same app.** Each window has its own header with its own state (a search, a filter).
- **Keyboard.** The shortcuts of each app (for example the focus of the search) keep working and reach the field in the
  header.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Each of the four apps MUST describe its controls to the window header (spec 090, FR-001) and MUST NOT draw
  a first row of its own under it.
- **FR-002**: Passwords, Contacts and Wallet MUST show the sidebar button as the first control of the header, and it
  MUST NOT collapse while any other control can; the Marketplace has no sidebar button.
- **FR-003**: The search of each app MUST sit in the middle of the header, behave as the field of the old row did (the
  text kept in the place's address, cleared by Escape, restored by the history) and be the first control to collapse.
- **FR-004**: Every control, filter and menu entry of the old rows (spec 034 FR-007, spec 051 FR-002, spec 092 FR-010,
  spec 069 FR-012) MUST be reachable in the header or, when the window is narrow, in the overflow panel, with the same
  effect; the test ids of today MUST be kept.
- **FR-005**: The controls that the old rows hid while an entry, a contact or a pass is open MUST be absent from the
  header while it is open, and back when it closes.
- **FR-006**: A filter, a sort or a view switch that is on MUST be shown by a check mark at the end of its menu entry;
  the Marketplace's category and marketplace choices MUST mark the current entry the same way.
- **FR-007**: The overflow order MUST put the search first, then the filters and choices, then the menus and "Refresh",
  and the creating action ("+") last, so that "+" is the last action to leave the row.
- **FR-008**: A menu control whose filter, sort or view is not at its default MUST show a small dot on its icon, so a
  filtered list is recognisable from the header without opening the menu. The dot is the only mark on the control (no
  background, no text), is shown in every place the control can be (row, collapsed to its icon), and is absent when the
  menu is at its defaults. When such a control sits in the overflow panel, the overflow button carries the dot and the
  control's row in the panel carries it too, so a filtered list stays recognisable in a narrow window. The check marks of
  the entries inside the menu carry the same information.
- **FR-009**: The dialogs that a control opens MUST keep working as today, and MUST NOT depend on the control being drawn
  (it can be in the overflow panel or collapsed when the dialog is open).
- **FR-010**: Each app MUST be moved in its own change, and the other three MUST be unaffected by it.
- **FR-011**: The window of each app MUST give the height of the removed row to its content (spec 090, SC-001), and
  the content MUST start directly below the header.

### Key Entities

- **Header description of an app**: the three areas (start, center, end) with the controls above, per app and per place
  (an open entry gives another description than the list).
- **Filter state**: what the menu entries mark (the picked tag, the sort, the kind, "hide expired", the category, the
  marketplace). It is the app's own state; the header only shows and changes it through events.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: In each of the four apps, the content starts directly below the header: the distance between the header
  and the first element of the app is at most 1 px more than in Settings, and no app row is drawn in between.
- **SC-002**: Every control of the old rows can be used in a window of 460 px, either in the header or in the overflow
  panel (counted against the list in FR-004: none missing).
- **SC-003**: In a window of 1000 px the header of Passwords shows all its controls in the row (none in the overflow
  panel).
- **SC-004**: For each app, the E2E scenarios of today pass with only their selectors for the moved controls adapted
  (the test ids are kept), and one scenario per app checks the header at a narrow and a wide window.
- **SC-005**: A user who sets a filter and then looks at the window can say, without opening the filter, whether the list
  is filtered (an operator check by hand in each of Passwords and Wallet).

## Assumptions

- The pilots' mechanism (spec 090) is complete: the controls `button`, `toggle`, `search`, `menu` (with `checked`),
  `choice`, `breadcrumb`, `title` and `separator`, the collapse model and the overflow panel on the window need no
  extension for these apps, except the dot of FR-008: a menu control gets an on/off flag that draws it. It is added to the
  mechanism once (in the first app that needs it, Passwords) and reused by the others.
- The Marketplace's category and marketplace choices are menus with the current entry checked, not a new control: a
  catalogue can have many categories, which a segmented `choice` (at most 8 options) cannot hold.
- The old rows have no persisted state of their own: the search text lives in the place's address, the view settings of
  Wallet in its own store. Moving the controls changes neither.
- The text of every label stays as it is (same translation keys), so no new translation is needed for the controls
  themselves.
- Chat, and the dock and launcher of spec 091, are out of scope.

## Out of Scope

- Chat (its own spec).
- New features in the four apps: no new filter, no new action, no new setting.
- The tab switcher (spec 091) and the SDK for extensions (spec 090).
