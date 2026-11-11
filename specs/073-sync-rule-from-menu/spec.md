# Feature Specification: Create a sync rule from the file browser's context menu

**Feature Branch**: `073-sync-rule-from-menu`

**Created**: 2026-11-01

**Status**: Draft

**Input**: User description (operator, 2026-11-01): "Ich möchte gerne im Kontextmenü im Dateibrowser die
Möglichkeit, Sync-Regeln anlegen zu können."

## Terms

- **Folder entry**, **file entry**: a folder or a file in the list of the open folder.
- **Open folder**: the folder the panel shows; its context menu is the one of the empty area.
- **Source place** (spec 061 "Quellort", "Quelle" in the dialog): the place a rule syncs from. For a file entry it
  is the file (spec 074).
- **Sync rule dialog**: the dialog of spec 061 that creates or changes a rule.

## Relationship to existing specs

- **Spec 044** (file browser): extends the context menus (FR-017 to FR-023) by one entry. No change to
  sources, transfers or the protection of holzi's own data (FR-037).
- **Spec 061** (sync rules): the entry opens the existing sync rule dialog. Rules, their places, the
  dialog's checks and the sync rule list do not change. FR-002 (which folders may be a place) decides where
  the entry is offered.
- **Spec 074** (sync rules for single files): makes a file a valid source place. **This spec depends on it** for
  file entries; the entry for folders does not.
- **Spec 062** (split view): the menu acts on the panel it was opened in.
- **Spec 071** (pinned folders): the entry sits next to "Zu Orten hinzufügen" as another command on a
  folder.

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Start a sync rule from a folder entry (Priority: P1)

Anna keeps her photos in `~/Bilder/Urlaub`. She right-clicks the folder in the file browser and chooses
"Sync-Regel erstellen …". The sync rule dialog opens with `~/Bilder/Urlaub` already filled in as the
source. She picks her cloud storage as the target, saves, and stays in the file browser.

**Why this priority**: The core of the request: today a rule needs a detour through the sync rule list and
the folder picker, although the user is standing in the folder already.

**Independent Test**: Right-click a folder, choose the entry, see the dialog with that folder as the source place,
pick a target, save, see the confirmation and the rule in the sync rule list.

**Acceptance Scenarios**:

1. **Given** a folder entry on this device, **When** Anna opens its context menu, **Then** the menu offers
   "Sync-Regel erstellen …".
2. **Given** the menu is open on a folder entry, **When** Anna chooses the entry, **Then** the sync rule
   dialog opens with that folder as the source place and no target; name, direction, delete mode and interval have
   the dialog's usual defaults (spec 061 FR-005).
3. **Given** the dialog opened from the menu, **When** Anna picks a target and saves, **Then** the rule
   exists, the dialog closes, a short confirmation shows, and the file browser still shows the same folder.
4. **Given** the dialog opened from the menu, **When** Anna cancels, **Then** no rule is created.
5. **Given** the dialog opened from the menu, **When** Anna picks another folder for the source place, **Then** the
   dialog uses it like in the list's dialog.

---

### User Story 2 - Start a rule for the open folder (Priority: P2)

Ben is inside `~/Dokumente/Steuer` and wants to back up exactly this folder. He right-clicks the empty area
(a long press on touch) and chooses "Sync-Regel erstellen …". The dialog opens with the open folder as the
source.

**Why this priority**: Same value as story 1 for the folder one is in; the entry follows the pattern of the
pin command, which exists for both the entry and the open folder.

**Independent Test**: Right-click the empty area of a folder, choose the entry, see the open folder as
source place.

**Acceptance Scenarios**:

1. **Given** the open folder is a folder that may be a place, **When** Ben opens the context menu of the
   empty area, **Then** the menu offers "Sync-Regel erstellen …".
2. **Given** that menu, **When** Ben chooses the entry, **Then** the dialog opens with the open folder as
   source place.
3. **Given** the open folder in the right panel of the split view, **When** Ben uses the entry there,
   **Then** the dialog gets the right panel's folder; the left panel's folder is not used.

---

### User Story 3 - Start a rule for a single file (Priority: P1)

Dora right-clicks `Haushalt.ods` and chooses "Sync-Regel erstellen …". The dialog opens with that file as
the source; she picks a folder on her cloud storage as the target and saves. Where the entry is for a
file, the dialog shows what spec 074 defines for a file rule: no delete mode, and a target that is a
folder.

**Why this priority**: Operator request of 2026-11-01: the entry is for files as well as folders.

**Independent Test**: Right-click a file, choose the entry, see the file as the source place, pick a target folder,
save.

**Acceptance Scenarios**:

1. **Given** a file entry of a source that may be a place, **When** Dora opens its context menu, **Then**
   the menu offers "Sync-Regel erstellen …".
2. **Given** that menu, **When** Dora chooses the entry, **Then** the dialog opens with the file as the source place
   and no target.
3. **Given** the dialog opened from a file entry, **When** Dora saves with a target folder, **Then** the
   rule is a file rule as spec 074 defines it.

---

### User Story 4 - No entry where the entry cannot be a place (Priority: P2)

Clara browses holzi's own data and a folder a contact shared with her. Neither may be a place of a rule
(spec 061 FR-002). The menu does not offer "Sync-Regel erstellen …" there, so she never opens a dialog that
cannot succeed.

**Why this priority**: Without it the entry would lead into a dead end; but it adds nothing to the main flow.

**Independent Test**: Open the context menu in holzi's own places and in a received share; the entry is
absent. Open it on a selection of several entries; the entry is absent.

**Acceptance Scenarios**:

1. **Given** a folder or a file inside one of holzi's own places or inside a share received from another
   vault, **When** the menu opens on an entry or on the empty area, **Then** it does not offer the entry.
2. **Given** a selection of several entries, **When** the menu opens, **Then** it does not offer the entry.
3. **Given** a folder or a file of any other source (this device, another own device, a cloud storage, an
   unlocked encrypted folder), **When** the menu opens, **Then** it offers the entry.

---

### Edge Cases

- The folder overlaps an existing rule of this device: saving fails with the dialog's message (spec 061
  FR-004), as in the list's dialog; nothing else changes.
- The folder is on a source that can have no encryption, or in an encrypted folder: the dialog's consent
  and hint (spec 061 FR-030a, FR-031) apply once the target is picked, unchanged.
- The vault locks or the folder disappears while the dialog is open: the dialog behaves as it does when
  opened from the list.
- An encrypted folder that is still locked: the entry is not offered, as the folder is not a usable place
  before it is unlocked.

## Requirements _(mandatory)_

### Functional Requirements

- **FR-001**: The context menu of a single folder entry or file entry MUST offer "Sync-Regel erstellen …"
  when it may be a place of a sync rule (FR-004).
- **FR-002**: The context menu of the open folder (empty area) MUST offer "Sync-Regel erstellen …" when the
  open folder may be a place of a sync rule (FR-004).
- **FR-003**: Choosing the entry MUST open the sync rule dialog of spec 061 with that folder or file as
  source place and no target. All other fields MUST have the dialog's defaults.
- **FR-004**: The menu MUST NOT offer the entry for a selection of several entries, or for a folder or file
  that spec 061 FR-002 excludes as a place: holzi's own data, a share received from another vault, and a
  locked encrypted folder.
- **FR-004a**: The entry for a file entry MUST work only once spec 074 is in place. Until then the menu does
  not offer it for files.
- **FR-005**: The dialog MUST check, save and report errors exactly as when opened from the sync rule list
  (spec 061 FR-004, FR-030a, FR-031). The prefilled place MUST be replaceable through the folder picker.
- **FR-006**: Cancelling the dialog MUST create no rule and change nothing.
- **FR-007**: After saving, the file browser MUST keep showing the folder it showed before, and MUST show a
  short confirmation that the rule was created. The rule MUST appear in the sync rule list.
- **FR-008**: In the split view, the entry MUST act on the folder of the panel whose menu was opened.
- **FR-009**: The entry MUST work with the mouse, a long press on touch and the keyboard (context menu key,
  Shift+F10) like the other menu entries; when the dialog closes, the focus MUST return to where it was.
- **FR-010**: Creating a rule from the sync rule list MUST work as before.

### Out of Scope

- A prefilled target, also in the split view, and a separate entry for "sync to here".
- The selection bar and the keyboard shortcuts of spec 044; the entry lives in the context menus only.
- Changes to the sync rule dialog's fields, defaults or checks.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: A user reaches the sync rule dialog with the source filled in with two actions from any
  folder that may be a place (open the context menu, choose the entry), without opening the sync rule list.
- **SC-002**: In every folder that may be a place, 100% of single-folder and open-folder context menus
  offer the entry; in holzi's own places and in received shares, 0% do.
- **SC-003**: A rule created from the menu is the same as one created from the list with the same inputs:
  same checks, same rejections, same saved values.
- **SC-004**: The existing sync rule E2E scenarios of spec 061 and the file browser's menu scenarios of
  specs 044 and 071 pass unchanged.

## Assumptions

- The menu entry is hidden, not greyed out, where it does not apply, as "Freigeben" is for holzi's own
  data (spec 056).
- The confirmation after saving is a short, non-blocking message, like the other confirmations of the
  file browser.
- Android and desktop get the same entry; touch reaches it through the long-press menu.
- The label is German like the rest of the interface and comes from the translation files, with an English
  counterpart where the other file browser entries have one.
