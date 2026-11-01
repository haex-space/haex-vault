# Feature Specification: Pinned folders in the file browser sidebar and "My file shares" as a list

**Feature Branch**: `071-files-pins-shares`

**Created**: 2026-10-30

**Status**: Implemented; revised 2026-10-31 (added folders live in "Orte", any source); manual M1 open

**Input**: User description (operator, 2026-10-30): "In the file explorer I want 1. to add any folder to
the sidebar for quick access and remove it again (also via context menu and by drag & drop); 2. my
'Dateifreigaben' should not be shown in a dialog but as a list in the browser, like the sync rules."

## Revision 2026-10-31

The first version put the pinned folders into a section "Favoriten" of their own and allowed only this
device and the vault's cloud storages. The operator rejected that: "Das Ziel war NICHT einen extra
Bereich für Favoriten zu machen, sondern den Bereich 'Orte' entsprechend zu erweitern … beliebige
Ordner aus beliebigen Quellen (eigener Host, Cloud Storage, anderer Vault Storage) via Drag & Drop".
So the added folders are entries of **Orte** below the known places, there is no "Favoriten" section,
and every source of the sidebar may be added (FR-001, FR-002, US1–US3 below are the revised text).
In a second round the same day the operator found that dropping did not work in the app and that only
added folders, not the known places, could be removed: "ich kann nur zusätzlich hinzugefügte Ordner per
Kontextmenü löschen, nicht aber die bereits vorhandenen". So every entry of "Orte" is removable now
(FR-015), and the drop uses the "copy" effect the entries drag allows (FR-005).

## Terms

- **Added folder** (pin): a folder the user added to the sidebar's section **Orte**, below the known
  places of the device. It is a shortcut: removing it never touches the folder.
- **Place**, **source**, **entry**, **panel**, **frame**: as in specs 044 and 062.
- **Share list**: the list of the vault's own file shares (spec 056 US4, FR-020) as the content of a
  panel, laid out like the sync rule list of spec 061.

## Relationship to existing specs

- **Spec 044** (file browser): extends the sidebar (FR-002) and the context menus (FR-017 to FR-023). No
  change to sources, transfers or the protection of holzi's own data (FR-037).
- **Spec 056** (file sharing): FR-020 (overview of the vault's own shares) is satisfied by a list in the
  panel instead of a drawer. Everything the overview did stays (state, serving device, contacts with
  level, revoke at once FR-024, waiting state FR-026, edit in the share dialog).
- **Spec 061** (sync rules): the sync rule list is the model for the share list; both are "views" a
  panel shows instead of a folder, kept in the tab location.
- **Spec 062** (split view): both new views belong to a panel, so each panel may show one.
- **Spec 022** (session restore): the share list view comes back with the tab like the sync rules.

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Pin a folder and open it from the sidebar (Priority: P1)

Anna works in `~/Projekte/holzi/docs` every day. She right-clicks the folder in the file browser and
chooses "Zu Orten hinzufügen". The folder shows in the sidebar under "Orte", below Bilder, Downloads and
the other known places; a click opens it. She can add any folder of any source: this device, her cloud
storages, her other devices, a share a contact gave her.

**Why this priority**: The core of the request: a quick way to the folders one really uses.

**Independent Test**: Right-click a folder, pin it, see it in the sidebar, click it, restart the app and
see it again.

**Acceptance Scenarios**:

1. **Given** a folder entry, **When** Anna opens its context menu, **Then** the menu offers "Zu
   Orten hinzufügen"; once added, it offers "Aus Orten entfernen" instead.
2. **Given** the open folder, **When** Anna opens the context menu of the empty area, **Then** it offers
   the same for the open folder.
3. **Given** a pinned folder, **When** Anna clicks it in the sidebar, **Then** the active panel opens
   it; while it is open the entry is highlighted like a place.
4. **Given** pinned folders, **When** the app restarts, **Then** they are still there, in the order they
   were pinned.
5. **Given** a folder that is already one of the known places (Bilder, Downloads, …) or already pinned,
   **When** Anna pins it again, **Then** nothing is added twice.
6. **Given** a pinned folder that no longer exists (deleted, drive not plugged in), **When** Anna
   clicks it, **Then** the panel behaves as for any vanished folder (spec 044, spec 064 FR-025); the pin
   stays until Anna removes it.
7. **Given** a folder of another own device or of a share of a contact, **When** Anna adds it, **Then**
   it shows in "Orte" with the name of its source next to it; while that source cannot be reached the
   entry is greyed out and does not open, but its context menu still removes it.
8. **Given** an added folder of a cloud storage, device or share that is no longer in the sidebar (removed,
   card deleted), **Then** the entry is hidden but kept, as the source may return.

---

### User Story 2 - Pin and unpin by drag & drop (Priority: P1)

Ben drags a folder from the list onto the "Orte" section of the sidebar. It is added below the known
places. To remove a
pinned folder he drags it from the sidebar onto the strip "Hierher ziehen, um aus Orten zu entfernen" that shows
while he drags.

**Why this priority**: Named explicitly by the operator.

**Independent Test**: Drag a folder row onto the section; drag the pinned entry onto the removal strip.

**Acceptance Scenarios**:

1. **Given** a folder entry (or a selection with folders), **When** Ben drops it on the "Orte"
   section, **Then** every folder of the drag is added; files in the selection are ignored. The
   cursor shows "copy" (the only effect besides "move" the entries drag allows; "link" would make the
   webview refuse the drop), never "move": nothing is copied or moved.
2. **Given** a drag with only files, **When** it hovers over the section, **Then** the drop is not
   offered.
3. **Given** a pinned entry, **When** Ben starts dragging it, **Then** the removal strip appears below
   the list; dropping the entry there removes the pin; dropping it anywhere else, or cancelling with
   Esc, changes nothing.
4. **Given** no added folders, **Then** "Orte" shows only the known places, with no hint and no empty
   section; it is the drop target all the same.

---

### User Story 3 - Unpin from the sidebar's context menu (Priority: P1)

Clara right-clicks an added folder in "Orte" and chooses "Aus Orten entfernen".

**Independent Test**: Right-click a pinned entry, remove it, see it gone and the folder untouched.

**Acceptance Scenarios**:

1. **Given** a pinned entry, **When** Clara opens its context menu, **Then** it offers "Aus Orten
   entfernen" (and nothing that changes the folder).
2. **Given** the menu was used, **Then** the entry leaves the sidebar at once and the folder on the disk
   is unchanged.

---

### User Story 4 - "Meine Dateifreigaben" as a list in the browser (Priority: P1)

David clicks "Meine Dateifreigaben" in the sidebar. The panel shows a list of his shares instead of a
folder, like "Sync-Regeln": one row per share with its name, source path and serving device, its state,
and its contacts with level; a filter narrows many shares down; a row's actions edit the share (the
existing share dialog) or take a contact's access away.

**Why this priority**: Named explicitly by the operator; a dialog hides the list from the rest of the
browser and cannot be used next to a folder in the split view.

**Independent Test**: Share a folder with a contact, open "Meine Dateifreigaben": the list shows it;
revoke the contact in the list; edit the share from its row; restart: the tab shows the list again.

**Acceptance Scenarios**:

1. **Given** the sidebar, **When** David clicks "Meine Dateifreigaben", **Then** the active panel shows
   the share list in place of the folder (path bar shows the title, the search bar is hidden) and the
   sidebar entry is highlighted; no dialog opens. Going back or opening any place shows the folder
   again.
2. **Given** no shares, **Then** the list shows the empty text of spec 056.
3. **Given** shares, **Then** each row shows what the former overview showed: name, kind, path and
   serving device, state (aktiv, Änderung wartet, Quelle fehlt, Quelle nicht verfügbar), contacts with
   their level and the button to take access away at once.
4. **Given** the list, **When** David types into the filter, **Then** rows whose share name, path or
   contact name do not contain the text are hidden; with no match a text says so.
5. **Given** a row, **When** David chooses "Bearbeiten", **Then** the existing share dialog opens for that
   share; the list reflects the saved change without reload.
6. **Given** two panels (spec 062), **Then** one may show the share list while the other shows a folder.
7. **Given** the tab with the list, **When** the session is restored (spec 022), **Then** the list is
   shown again.

## Requirements _(mandatory)_

### Functional Requirements

- **FR-001**: The sidebar section "Orte" MUST list the added folders below the known places, in the order
  they were added. There MUST NOT be a section of their own. "Orte" is the drop target for adding.
- **FR-002**: Any folder of any source of the sidebar MAY be added: this device, a cloud storage of the
  vault (spec 038), another own device (spec 056 US1), a share of a contact (US2). An entry of another
  source MUST show that source's name; while the source cannot be reached it MUST be greyed out and not
  open, but stay removable. Entries of sources no longer listed are hidden, not deleted.
- **FR-003**: The context menu of a single folder entry, and of the open folder (empty area), MUST offer
  "Zu Orten hinzufügen" or, if the folder is added, "Aus Orten entfernen". Neither is offered for
  files, several entries, or the internal `device:` storages of encrypted folders. A folder that already is one of the
  known places (Orte) is neither pinned nor offered.
- **FR-004**: The context menu of an added entry in "Orte" MUST offer "Aus Orten entfernen".
- **FR-005**: Dropping a folder drag of holzi's entries on the "Orte" section MUST pin every folder
  of the drag (not files), without duplicates; it MUST NOT move or copy anything.
- **FR-006**: A pinned entry MUST be draggable; while one is dragged a removal strip MUST show, and
  dropping the entry there MUST remove the pin. Any other drop or Esc MUST leave it.
- **FR-007**: Pins MUST be stored per device (ADR 0001, like the other file browser preferences), never
  synced, as the paths exist only on this device. They survive a restart.
- **FR-008**: Clicking a pin MUST open it in the active panel (spec 062) like a place. A pin whose folder
  is gone behaves like any vanished folder; the pin itself stays.
- **FR-009**: Removing a pin MUST NOT change the folder.
- **FR-010**: Pin commands MUST work with mouse and keyboard (context menu key / Shift+F10 opens the same
  menus); every action has a text label for screen readers; the drop strip is not the only way (FR-003,
  FR-004).
- **FR-011**: "Meine Dateifreigaben" MUST show as a list in a panel, in the tab location (spec 061 style:
  one more view key), instead of a drawer. The drawer MUST be removed.
- **FR-012**: The share list MUST offer everything the overview of spec 056 US4 did (FR-020, FR-024,
  FR-026) and the filter of Acceptance Scenario 4 of US4. It MUST update live when the vault's shares
  change (as the overview did).
- **FR-013**: While a panel shows the share list the search bar and the folder content are not shown;
  navigating (a place, a path, back) returns to a folder.
- **FR-015**: Every entry of "Orte" MUST be removable by its context menu and by the removal strip,
  the known places of the device included. A removed known place MUST be hidden (stored per device like
  the pins) and MUST come back when the same folder is added again; the menus then offer "Zu Orten
  hinzufügen" for it.
- **FR-016**: Every entry of "Orte", the known places included (the home place asks for "home"), MUST show the icon its folder shows in the folder list, and MUST follow a change of the icon set like the list. An added folder MUST show the icon its entry shows in the folder list (the chosen icon
  set of spec 053, e.g. the picture folder for "Bilder"), not a generic folder. Paths of known places
  MUST be compared without trailing separators (Windows paths case-insensitively), and a known place
  that is also stored as an added folder MUST show once, as the known place with its own icon.
- **FR-014**: All new texts MUST exist in German and English; no new dependency; no change to Rust.

### Key Entities

- **Pin** (added folder): `{ source, path }`; `source` as `lib/files/split.ts` `sourceToken`. The label
  is the folder's name, or the source's name for its root.
- **View key** `sh`: the panel shows the share list (like `sr` for the sync rules).

## Success Criteria _(mandatory)_

- **SC-001**: A folder is pinned and opened again in at most 2 interactions (menu + click), also after a
  restart.
- **SC-002**: Drag a folder onto the section pins it; drag a pinned entry onto the strip removes it; both
  leave the files untouched.
- **SC-003**: "Meine Dateifreigaben" never opens a dialog; the list shows next to a folder in the split
  view.
- **SC-004**: Existing sync rule and share dialog behaviour does not change (their E2E scenarios stay
  green).

## Assumptions

- Pins are not reordered by hand in this spec (order = pinning order). Reordering may follow.
- A removed known place is hidden on this device only (FR-007) and is not deleted; it comes back when
  it is added again (by its parent folder's entry menu, the open folder's menu, or a drop).
- A pinned folder on a drive that is not plugged in is opened like any restored place of that drive
  (spec 064 FR-025) without the drive key; if the path is missing, the panel moves up to the nearest
  existing folder.
- Added folders of sources that are no longer listed are hidden but kept, as the source may return.
- Phones: "Orte" is the same; drag & drop is a mouse feature there, the context menu (long
  press) is the way.
