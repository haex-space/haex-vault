# Feature Specification: Sync rules for single files

**Feature Branch**: `074-sync-rule-single-file`

**Created**: 2026-11-01

**Status**: Draft; the three assumptions below confirmed by the operator on 2026-11-01

**Input**: User description (operator, 2026-11-01): "Ich möchte gerne auch einzelne Dateien syncen können.
Sobald sie aber ihren Namen oder ihren Pfad ändern, fliegt die Sync-Regel raus bzw. die Quelle wird einfach
ungültig."

## Terms

- **File rule**: a sync rule (spec 061) whose source place is one file instead of a folder.
- **Source file**: the file the source place names, by its path: the folder it is in and its name.
- **Copy**: the file in the target folder that the rule keeps equal to the source file.
- **Invalid source**: the state of a file rule when no file is at the source file's path.
- **Place**, **source place**, **target place**, **run**, **interval**, **delete mode**: as in spec 061.

## Relationship to existing specs

- **Spec 061** (sync rules): extends FR-001 and FR-002 (a place may be a file), FR-004 (overlap), FR-022
  (a place that cannot be read) and FR-032 (the list). Everything not named here works for a file rule as
  it does for a folder rule: transfer (FR-014 to FR-017), conflicts (FR-012), the run log (FR-043), the
  watcher (FR-027, FR-050), the checks for encrypted folders and storages without encryption (FR-030a,
  FR-031). Folder rules do not change.
- **Spec 075** (source and target in code): this spec follows it and uses its names: `source` and `target`
  for the two places, `location` for the storage reference inside a place.
- **Spec 044** (file browser): the folder picker offers files as well, for the source place.
- **Spec 073** (sync rule from the context menu): offers "Sync-Regel erstellen …" for a file entry once this
  spec is in place.

## Clarifications

### Session 2026-11-09

- Q: A one-way file rule, the copy was changed at the target and the source is unchanged: what does the next run do?
  → A: It leaves the changed copy alone and reports a deviation, as a folder rule does (spec 061 FR-010).
- Q: May editing a rule switch its source between a folder and a file? → A: Yes; the picker offers both, and the
  rule's kind follows the new choice. Its per-file state starts afresh, as when a folder place is changed.
- Q: Besides the state in the rule list, does holzi tell the user about an invalid source? → A: No. The state, its
  reason and the run log are the only places, as for the states "waiting" and "error".

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Keep one file in sync with another place (Priority: P1)

Anna keeps her household budget in `~/Dokumente/Haushalt.ods`. She creates a rule with that file as the
source and a folder on her cloud storage as the target. From then on, whenever she saves the spreadsheet,
the next run puts the new version into the cloud folder as `Haushalt.ods`. The other files in her
Dokumente folder stay out of it.

**Why this priority**: The core of the request: a rule for exactly one file, without syncing its whole
folder.

**Independent Test**: Create a rule with a file as the source place and a folder as the target place, change the file, run
the rule, see the new version in the folder under the same name.

**Acceptance Scenarios**:

1. **Given** the sync rule dialog, **When** Anna picks a file as the source place and a folder as the target place, **Then**
   she can save the rule.
2. **Given** a file rule, **When** its source file changed since the last run, **Then** the next run copies
   it into the target folder under the same name and the run log says so.
3. **Given** a file rule whose source file did not change, **When** it runs, **Then** it copies nothing.
4. **Given** the source place is a file, **When** Anna opens the picker for the target place, **Then** it lets her pick
   folders only.
5. **Given** a source file in an encrypted folder and a target in a place without encryption, **When** Anna
   saves, **Then** the dialog asks for her consent as it does for a folder rule (spec 061 FR-030a).
6. **Given** a one-way file rule, **When** the copy was changed at the target and the source did not change,
   **Then** the next run leaves the copy as it is and reports a deviation (spec 061 FR-010); if the source
   changes later too, the changed copy is kept as a conflict copy (spec 061 FR-012).

---

### User Story 2 - A renamed or moved source makes the rule's source invalid (Priority: P1)

Ben renames `Haushalt.ods` to `Haushalt-2026.ods`. The rule does not follow the file: it knows the file by
its path, and nothing is at that path any more. The rule shows "Quelle ungültig" with the reason, and the
copy in the cloud folder stays as it is.

**Why this priority**: This is the second half of the request, and the safe behaviour: a rule never
guesses which file the user means, and never deletes a copy because the source went away.

**Independent Test**: Rename, move and delete the source file in turn; after the next run the rule shows
the invalid state each time and the target is unchanged.

**Acceptance Scenarios**:

1. **Given** a file rule, **When** the source file is renamed, moved to another folder or deleted, **Then**
   the next run does not copy or delete anything, and the rule shows "Quelle ungültig" with the reason
   that no file is at the stored path.
2. **Given** a file rule, **When** a folder above the source file is renamed or removed, **Then** the same
   happens.
3. **Given** a file rule, **When** a folder is at the source file's path, **Then** the same happens.
4. **Given** an invalid source, **When** a file is at the stored path again (restored, or saved again by an
   app that replaces files), **Then** the next run continues without a step by the user.
5. **Given** an invalid source, **When** Ben edits the rule and picks the renamed file as the source place, **Then**
   the rule is valid and runs.
6. **Given** an invalid source, **When** Ben deletes the rule, **Then** neither the former source nor the
   copy changes.
7. **Given** an editor that saves by writing a new file and replacing the old one at the same path,
   **When** the user saves, **Then** the source stays valid.

---

### User Story 3 - A file rule works both ways, and never deletes (Priority: P2)

Clara chooses "beidseitig". If she edits the copy in the cloud folder, the next run brings the change back
to the source file; if both changed, the conflict is handled as for folder rules. If the copy is deleted,
the next run puts it back from the source file.

**Why this priority**: The dialog already offers both directions, so file rules must give a defined answer
for them; it is not needed for the first use.

**Independent Test**: Change the copy, run, see the change in the source; delete the copy, run, see it
back.

**Acceptance Scenarios**:

1. **Given** a two-way file rule, **When** only the copy changed, **Then** the next run copies it over the
   source file.
2. **Given** a two-way file rule, **When** both changed since the last run, **Then** the run does not
   overwrite either and lists a conflict (spec 061 FR-012).
3. **Given** a file rule in either direction, **When** the copy is missing, **Then** the next run copies it
   again from the source file.
4. **Given** a file rule in either direction, **When** the source file is gone, **Then** the rule does not
   delete the copy (User Story 2).

---

### Edge Cases

- The target folder already holds a different file of that name when the rule first runs: handled as the
  first run of a folder rule (spec 061 FR-012a): the user decides before anything is overwritten.
- The source file's place cannot be reached (another device is off, no network, drive not mounted): the
  rule reports this as for a folder rule (spec 061 FR-022, FR-024), not as an invalid source. It resumes
  when the place is back.
- The source is a symbolic link: the dialog rejects it, since links are not followed (spec 061 FR-018).
- The same file is the source of two rules, or lies inside a folder that is a place of another rule, or two
  rules would write the same copy: saving is rejected like overlapping folder rules (spec 061 FR-004).
- A file in holzi's own data or in a share received from another vault: not allowed as a place (spec 061
  FR-002).
- The source file is a `.haex-ignore` or its name matches a pattern of an exclusion file: it is synced
  anyway, since the user named the file.
- The target folder disappears: as for a folder rule (spec 061 FR-022).

## Requirements _(mandatory)_

### Functional Requirements

- **FR-001**: The source place of a rule MUST be allowed to be one file of any source that spec 061 FR-002 allows.
  The target place of a file rule MUST be a folder; the copy lies in it under the source file's name.
- **FR-002**: A file rule MUST support both directions, the interval, pause and resume, and deleting, as a
  folder rule does.
- **FR-003**: A run MUST copy the source file when it differs from the copy (spec 061 FR-008), with the
  safeguards of spec 061 FR-014 to FR-017, and MUST log the run (FR-043). A one-way file rule MUST leave a
  copy that was changed at the target alone and report the deviation (spec 061 FR-010).
- **FR-004**: A file rule MUST NOT delete anything: not the source file, not the copy, in no direction and
  no state. It MUST have no delete mode; the dialog MUST NOT ask for one. A missing copy MUST be copied
  again.
- **FR-005**: The source file MUST be identified by its path alone. The rule MUST NOT follow a rename or a
  move, and MUST NOT look for the file by size, time or content.
- **FR-006**: When the source is reachable and no file is at the source file's path (nothing there, a folder
  there, or a folder above it missing), the run MUST NOT copy or delete anything, and the rule MUST show the
  state "Quelle ungültig" with the reason in the rule list and the run log. holzi sends no notification of it.
- **FR-007**: The rule MUST check the source file anew at every run. A file at the path again MUST end the
  invalid state without a step by the user. Editing the rule MUST allow picking another file, or a folder, as the source place; the rule's kind follows the
  choice, and its per-file state starts afresh, as when a folder place is changed.
- **FR-008**: An unreachable place MUST stay distinct from an invalid source (spec 061 FR-022, FR-024).
- **FR-009**: The folder picker MUST offer files for the source place and folders only for the target place. A file rule MUST
  show in the sync rule list with the source file's path, and its actions MUST be those of a folder rule
  without the exclusions (FR-010).
- **FR-010**: The exclusion file (spec 061 FR-037 to FR-042) MUST NOT apply to a file rule; its editor MUST
  NOT be offered for one.
- **FR-011**: Saving MUST reject a file rule that overlaps another rule of this device, as spec 061 FR-004
  does for folders, and a source that is a symbolic link.
- **FR-012**: The checks and hints of spec 061 FR-030a and FR-031 (encrypted source, storage without
  encryption) MUST apply to a file rule.
- **FR-013**: A change of the source file MUST start a run as a change in a folder place does (spec 061
  FR-027, FR-050).
- **FR-014**: Folder rules MUST work as before.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: After the source file is renamed, moved or deleted, 100% of the following runs leave the
  target unchanged, and the rule shows the invalid state no later than the next run.
- **SC-002**: A file rule deletes no file in any of the scenarios above (0 deletions of source or copy).
- **SC-003**: A saved change of the source file is in the target folder after the next run, with no step
  by the user.
- **SC-004**: The existing sync rule scenarios of spec 061 pass unchanged.

## Assumptions

The first three were confirmed by the operator on 2026-11-01.

- "Fliegt die Sync-Regel raus bzw. die Quelle wird ungültig" is taken as: the rule stays and shows an
  invalid source; it is not deleted behind the user's back. Both readings give the same safety: no copy
  and no deletion. If the operator wants the rule removed instead, only FR-006 and FR-007 change.
- The target is a folder and the copy keeps the source file's name; renaming on copy is out of scope.
- The rule follows the path, not the file: a different file at the same path is the source.
- Deleting is left out of file rules altogether, because "the source went away" is the invalid state and not
  a deletion to mirror.
- Agents that manage rules (spec 061 US5) handle file rules like any rule; one kind of rule, one set of
  tools.
