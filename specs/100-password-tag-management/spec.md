# Feature Specification: Tag management by selection, with merge

**Feature Branch**: `feat/100-tag-management`

**Created**: 2026-11-25

**Status**: Draft

**Input**: User description: "Improve the tag management in the password manager. I do not want to see every tag in a possibly huge list, just a dropdown: I pick the tag there and then edit it. I also need a way to merge two tags."

## Relationship to existing specs

- **Spec 034** (password manager, US2, FR-011): this spec changes how the tag manager of FR-011 is
  presented (one tag at a time instead of every tag in a list) and adds merging. Renaming, coloring,
  deleting, the uniqueness of names (ignoring case and the form of umlauts) and the rule that only the
  user manages tags (FR-028) stay as they are.
- **Spec 017/021** (agent rights): an agent's password rights can name tags. Merging changes which
  entries carry which tag name, so it touches those rights (see FR-010).

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Pick one tag and edit it (Priority: P1)

A user with many tags opens "Tags verwalten". Instead of a list that shows every tag at once, they see
a single dropdown. They pick a tag in it, and below the dropdown the chosen tag appears with everything
that can be done to it: its name, its color, how many entries carry it, rename and delete. The
dropdown can be searched by typing, so a tag among hundreds is found in a moment.

**Why this priority**: This is the core of the request. Today the manager grows with every tag and
becomes unusable with a large vault; it is also the surface that merging (US2) lives on.

**Independent Test**: Create 60 tags, open the tag manager, type a few letters of one tag, pick it,
rename it and change its color. The manager never shows more than the dropdown and the one chosen tag,
and the entries carry the new name and color.

**Acceptance Scenarios**:

1. **Given** a vault with many tags, **When** the user opens the tag manager, **Then** it shows one
   dropdown and no list of all tags; no tag is selected yet and no edit controls are shown.
2. **Given** the dropdown is open, **When** the user types part of a name, **Then** only the tags whose
   name matches (ignoring case and the form of umlauts) remain, and picking one selects it.
3. **Given** a tag is selected, **Then** its name and color are shown, and the number of entries that
   carry it, split by app (see scenario 8), with the means to rename it, change its color and delete
   it.
4. **Given** a selected tag, **When** the user renames it to a name another tag already has (ignoring
   case and the form of umlauts), **Then** the rename is refused with a hint directly under the name
   field, as before.
5. **Given** a selected tag, **When** the user changes its color (applies on selection) or confirms a
   new name, **Then** the dropdown shows the tag with the new name and color.
6. **Given** a selected tag, **When** the user deletes it and confirms, **Then** the tag is removed from
   every entry, the selection is cleared, and the tag is gone from the dropdown.
7. **Given** a vault without any tag, **When** the user opens the tag manager, **Then** it says that
   there are no tags and shows no dropdown.
8. **Given** a tag carried by 5 logins and 5 Wallet passes, **When** it is shown in the dropdown or as
   the selected tag, **Then** the count appears as the password manager's icon with 5 and Wallet's
   icon with 5, not as one number 10; an app with no entry for the tag shows 0 in a muted style. The
   number for an app equals the number of that app's entries the tag filter of that app would show.
9. **Given** the Wallet window, **When** the user opens its header action for tags, **Then** the same
   tag manager opens, with the same tags and counts as in the password manager, and every change made
   there shows in both apps.
10. **Given** the rename field, **When** the user enters nothing or only spaces, **Then** the name is
    refused with a hint directly under the field and nothing changes; a name needs at least one
    character that is not a space.

---

### User Story 2 - Merge one tag into another (Priority: P1)

A user finds that "Work", "Job" and "Arbeit" all mean the same. They select "Job" and choose to merge it
into "Arbeit". After they confirm, every entry that carried "Job" carries "Arbeit" instead, and "Job" is
gone.

**Why this priority**: The second explicit request. Without it the only way to consolidate tags is to
re-tag every entry by hand.

**Independent Test**: Tag three entries with "Job" (one of them also with "Arbeit") and two with
"Arbeit". Merge "Job" into "Arbeit". Afterwards the four distinct entries carry "Arbeit" once each, "Job"
exists nowhere, and the entry that had both has "Arbeit" a single time.

**Acceptance Scenarios**:

1. **Given** a selected tag (the source), **When** the user chooses "Zusammenführen" and picks another
   tag as target, **Then** a confirmation names both tags and says how many entries are affected; only
   after confirming does anything change.
2. **Given** the user confirms, **Then** every entry that carried the source tag carries the target tag,
   an entry that carried both carries the target once, and the source tag no longer exists.
3. **Given** a merge, **Then** the target keeps its color and, unless the user named the result (scenario
   11), its name; the source's name and color are not carried over.
4. **Given** the user cancels the confirmation, **Then** nothing changes.
5. **Given** a merge has finished, **Then** the target is the selected tag, its entry count is the number
   of distinct entries that now carry it, and the source is gone from the dropdown and from the tag
   filter of the password list.
6. **Given** a vault with a single tag, **Then** merging is not offered, because there is no other tag
   to merge into.
7. **Given** the target picker, **Then** it lists every tag except the source, can be searched by typing
   like the main dropdown, and the source cannot be chosen as its own target.
8. **Given** a target is chosen, **Then** the confirmation shows the merge as a picture: the old tag
   (color, name and the number of its entries per app, as in FR-015), an arrow, and an input for the
   name of the merged tag, so it reads `old → new`.
9. **Given** that picture, **Then** the input holds the name of the target to begin with, and the user
   can change it.
10. **Given** the confirmation, **Then** the explanation that tags are shared by the password manager
    and Wallet, who is affected and that the merge cannot be undone is not on the screen but behind an
    info button; the warning about agent rights (FR-010) is not behind it.
11. **Given** the user changed the name and confirms, **Then** the merged tag has that name on every
    entry; a name of the source is allowed (it goes), the name of another tag that stays, an empty name
    or a name over 64 characters is refused with a hint directly under the input and nothing changes.
12. **Given** the user has several devices, **When** a merge happens on one device, **Then** the other
    devices show the same result after they sync, and no entry on any device loses its tag in the
    process.

---

### User Story 3 - Works in a narrow window and on the phone (Priority: P2)

The dropdown, the edit controls and the merge flow work in a narrow window and on Android, with the
same behavior as on the desktop, and every control can be reached without a pointing device.

**Why this priority**: HaexVault must work on every device and OS (a gap is a bug); this is a
requirement on P1, not an extra feature.

**Independent Test**: Run the tag scenarios of US1 and US2 in the narrow-window layout and on the
Android emulator.

**Acceptance Scenarios**:

1. **Given** a narrow window or a phone, **When** the user opens the tag manager, **Then** the dropdown,
   the selected tag's controls and the merge picker fit the width without horizontal scrolling and
   without hiding an action.
2. **Given** a keyboard, **When** the user works through select, rename, color, merge and delete,
   **Then** every step can be done with the keyboard alone.

---

### Edge Cases

- A tag is deleted or merged on another device while it is selected here: the selection falls back to
  "none" and the user sees the tag gone; no error and no stale controls remain.
- Two devices merge different pairs at the same time: both merges apply, no entry has the same tag
  twice. Two devices merge the same pair the other way round at the same time, or one tags an entry with
  the source while the other merges it: the result is decided per entry by the sync order, and an entry
  can lose that tag. This is the known limit of FR-008.
- A tag that only Wallet passes carry (the password manager's tag filter leaves it out) is listed in
  the manager like every other tag, with 0 for the password manager and its passes for Wallet.
- Equal names created on two devices that the next opening of the vault has not yet merged show as one
  tag (the one with the smallest id); a merge, rename or delete acts on that one, and the leftover
  twin shows up again afterwards until it is merged, which the user can do the same way.
- The source tag has no entries: merging still removes it (it is the same as deleting an unused tag),
  and the confirmation says "0 entries".
- The target already carries every entry of the source: nothing is re-tagged, the source simply
  disappears.
- Tags whose names differ only in case or umlaut form cannot exist side by side (spec 034 FR-011);
  merging therefore never needs to resolve a name clash.
- The confirmation for a merge or a delete is open and the vault locks or the window closes: nothing
  has changed.
- An entry in the trash carries the source tag: it carries the target after the merge, like any other
  entry, so restoring it later gives the merged tag.
- An agent or an extension asks to merge or manage tags: refused, as for renaming and deleting
  (spec 034 FR-028).

## Requirements _(mandatory)_

### Functional Requirements

- **FR-001**: The tag manager MUST present tags through a single dropdown from which the user selects
  one tag; it MUST NOT render a row or control per tag for all tags at once.
- **FR-002**: The dropdown MUST be searchable by typing, matching names ignoring case and the form of
  umlauts, and MUST show each tag's color and its entry count per app (FR-015).
- **FR-003**: With a tag selected, the manager MUST show its name, color and entry count per app and offer
  rename, color change, delete and merge for that tag only. With no tag selected, it MUST offer none of
  these.
- **FR-004**: Rename, color change and delete MUST keep their current behavior (names unique ignoring
  case and the form of umlauts; delete asks first and applies to every entry; a wrong name is told
  under its field; every other failure is a toast).
- **FR-005**: The manager MUST let the user merge the selected tag (source) into another tag (target)
  chosen from a searchable picker that excludes the source.
- **FR-006**: A merge MUST ask for confirmation before changing anything, showing the old tag with the
  number of its entries per app, an arrow and the name of the merged tag as an input that starts with
  the name of the target (scenarios 8 and 9); the explanation of what it means for both apps and that
  it cannot be undone MUST be reachable through an info button. It MUST change nothing when the user
  cancels.
- **FR-007**: A confirmed merge MUST give every entry of the source the target tag, MUST NOT give an
  entry the target twice, MUST remove the source tag, and MUST leave the color of the target unchanged;
  the name of the target changes only when the user entered another name (FR-018). It MUST be all or
  nothing: a failure, including a refused name, leaves both tags and every entry as they were.
- **FR-008**: A merge MUST reach the user's other devices through the normal sync as one unit, so that
  every entry that carried the source when the merge ran carries the target on every device, and no
  entry carries the target twice. A change made at the same time on another device that has not synced
  yet (tagging an entry with the source tag, or merging the same two tags the other way round) is not
  protected: it behaves like a concurrent delete of the tag does today (see Assumptions).
- **FR-009**: Only the user MAY merge tags, from the app itself; agents, extensions and the external
  bridge MUST be refused, as for renaming and deleting (spec 034 FR-028).
- **FR-010**: When an agent's password rights name the source or the target tag, the merge confirmation
  MUST say so and MUST keep saying it outside the info button, because rights go by tag name: agents that
  may read the target tag then also reach the entries that only carried the source, and a new name of
  the result is a name those rights do not carry. The merge itself does not change any agent's rights.
- **FR-011**: After a merge, rename, color change or delete, every place that shows tags (password
  list, tag filter, entry details, Wallet) MUST show the new state without a manual refresh.
- **FR-012**: The manager MUST work, with the same behavior, on every platform HaexVault runs on,
  including a narrow window and Android, and MUST be fully operable by keyboard.
- **FR-013**: Success MUST be visible only through the changed state (the tag renamed, merged or gone);
  failures MUST show as toasts, except a wrong name, which shows under its field (UI feedback rule).
- **FR-014**: All user-facing texts MUST be available in every language the app supports.
- **FR-015**: Wherever the manager shows how often a tag is used, it MUST show the number per app: the
  password manager's icon with the number of logins (and other non-pass entries), Wallet's icon with the
  number of passes. The icons are the very images the two apps have in the dock, the launcher and their
  tabs, taken from the same source, so they look the same there and here and follow if the app icons
  ever change. A different icon set chosen in the settings does not make them differ, each with an accessible name such as "Passwortmanager: 5 Einträge". It MUST NOT
  show one combined number.
- **FR-016**: The tag manager MUST be reachable from the password manager and from Wallet, as the same
  screen, so tags that Wallet passes carry can be managed from Wallet.
- **FR-017**: A tag name MUST have at least one character that is not a space. An empty or all-space
  name is refused wherever a tag is made or renamed (rename in the manager, adding a tag to an entry,
  bulk tagging, import), the manager tells the user under the field, and no tag with an empty name
  can exist.
- **FR-018**: The merged tag MAY get a new name in the merge itself: the name the user enters follows the
  rules of a rename (FR-017, at most 64 characters, unique ignoring case and the form of umlauts), with
  the one difference that the name of the source is free because the source goes. The rename and the
  merge happen together or not at all.

### Key Entities

- **Tag**: A name (at least one character that is not a space, at most 64; unique ignoring case and
  the form of umlauts), an optional color, and the entries that carry it. Shown in the dropdown with
  color and the count per app.
- **Tag usage**: For one tag, how many password-manager entries and how many Wallet passes carry it.
  Derived, not stored.
- **Entry–tag link**: The fact that one entry carries one tag; an entry carries a given tag at most
  once. A merge moves the links of the source to the target.
- **Merge**: One user action turning a source tag and a target tag into the target tag alone, carrying
  the union of the entries of both and, when the user entered one, a new name.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: With 500 tags, the tag manager opens showing no more than the dropdown, and the user
  reaches any tag by typing the start of its name until it is unique and picking it, without
  scrolling through the list.
- **SC-002**: A user merges two tags in at most three steps after opening the manager (select the
  source, choose the target, confirm).
- **SC-003**: After a merge, 100 % of the entries that carried either tag carry the target exactly
  once, and the source tag exists nowhere, on this device and on every synced device.
- **SC-004**: Neither a cancelled merge nor a failed one changes any tag or any entry.
- **SC-005**: Every scenario of US1 and US2 passes in the narrow-window layout and on Android with the
  same result as on the desktop.
- **SC-006**: For every tag, the number shown for an app equals the number of that app's entries that
  carry the tag, and the two numbers add up to the entries that carry it in the vault.

## Assumptions

- The tag manager stays reachable from where it is today (the "Tags verwalten" menu entry); only its
  content changes. Wallet gets its own header action for it (FR-016).
- Tags are one namespace for the password manager and Wallet (operator decision, 2026-11-25): rename,
  color, delete and merge act on the tag in both apps. That is why the counts are split by app
  (FR-015): the tag filter of the password manager shows only its own entries, and a combined number
  would not match it.
- A merge takes the target's color and, by default, its name, because the user chose the target by
  name; the user may change the name in the merge (FR-018). Moving the source's color to a target
  without one is not done, to keep the result predictable.
- Merging is directional (source into target); the user who wants the other direction selects the other
  tag first. A "merge these two" without a direction is not offered.
- Undo of a merge or a delete is not part of this spec; both ask for confirmation first.
- Agent rights name tags by name and are not rewritten by a merge or a rename (their behavior today);
  FR-010 only makes the effect visible before the user confirms.
- Creating a tag stays where it is today (by adding it to an entry); the manager does not create tags.
- The existing automatic merge of tags with the same name created on two devices (spec 034) stays and
  is not touched.
- Protecting a merge against a simultaneous change on another, not yet synced device would need a
  synced record of the merge and its own spec; it is not part of this feature. Until then a merge is
  as safe as deleting a tag is today.
- The FR-010 warning covers agent profiles. Approvals of browser extensions that also scope by tag are
  not covered.
