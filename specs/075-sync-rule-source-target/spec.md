# Feature Specification: Name the two places of a sync rule "source" and "target" in code

**Feature Branch**: `075-sync-rule-source-target`

**Created**: 2026-11-01

**Status**: Draft

**Input**: User description (operator, 2026-11-01): "Wir haben 'Ort A' und 'Ort B' doch schon lange in Quelle und
Ziel umbenannt. Ich möchte das auch im Code anpassen und source und target nennen."

## Terms

- **Source place**, **target place**: the two places of a sync rule, "Quellort" and "Zielort" in spec 061, shown
  as "Quelle" and "Ziel" in the dialog. A one-way rule copies from the source place to the target place; in a
  two-way rule the names stay (spec 061, Terms: "Richtung").
- **Old names**: every name that calls the places "A" and "B": the rule's `placeA` and `placeB` and their
  stored columns `place_a_*` and `place_b_*`, the sides `a` and `b`, the direction value `aToB`, and the
  per-file columns `a_size`, `a_mtime_ms`, `b_size`, `b_mtime_ms`.
- **Location**: the storage reference inside a place, "device or a storage of spec 038" (spec 044 FR-002). It
  is the field that is called `source` today and must not clash with the new name.

## Relationship to existing specs

- **Spec 061** (sync rules): already says source place and target place in prose and in the interface; this
  spec makes the code, the stored data and the agent tools say the same. No behaviour of a rule changes.
- **Spec 074** (sync rules for single files): builds on the new names and follows this spec.
- **Spec 073** (sync rule from the context menu): unaffected, it names no field.
- **Spec 044** (file browser): the type that names a storage reference keeps its name and meaning; only the
  field inside a sync rule place changes.

## User Scenarios & Testing _(mandatory)_

### User Story 1 - The code says source and target (Priority: P1)

A maintainer reads the rule code, the stored columns, the generated types and the agent tool schema. Wherever
the two places of a rule are named, they read "source" and "target", as in the interface. The maintainer never
has to remember that "A" means the source.

**Why this priority**: The whole request. Two vocabularies for one concept cost a translation step in every
change, and spec 074 will add more code around the two places.

**Independent Test**: Search the repository for the old names; none is left outside the history of migrations.
Build, lint and run all checks and tests.

**Acceptance Scenarios**:

1. **Given** the code after this change, **When** one searches for the old names, **Then** none is found in
   code, tests, translations, generated types, agent tool schemas and E2E scenarios; the only mentions are in the
   migrations of spec 061 and of this spec, in their tests, and in the test that shows an agent call with the
   old names is refused.
2. **Given** a sync rule place, **When** one reads its fields, **Then** they are `location` (the storage
   reference) and `path`, and the rule's two places are `source` and `target`.
3. **Given** the engine's two sides, **When** one reads the types for plans, state, pending deletions and the
   run log, **Then** the sides are `source` and `target` and the one-way direction is `sourceToTarget`.

---

### User Story 2 - Existing rules and their state survive (Priority: P1)

Anna has three rules from before this change. After updating, her vault opens, the three rules show with the
same names, source and target, direction, delete mode and interval, the same pending deletions and the same
run log. The next run does not copy everything again: it still knows what it synced last time.

**Why this priority**: Without it the rename would make every rule look new, re-copy everything and drop
pending decisions: data loss for a naming change.

**Independent Test**: Take a vault with rules, per-file state, pending deletions and a run log from before the
change, open it with the new version, compare the list, the state and the log; run a rule that had nothing to
do and see that it does nothing.

**Acceptance Scenarios**:

1. **Given** a vault with rules from before this change, **When** it opens, **Then** every rule shows the same
   values as before, with the old place A as source and the old place B as target.
2. **Given** a rule that had synced, **When** it runs after the change, **Then** it plans no more steps than it
   would have before the change.
3. **Given** a pending deletion found on the old side `a` or `b`, **When** the vault opens, **Then** it is
   still listed, on the same place.
4. **Given** a rule with direction one way, **When** the vault opens, **Then** it is still one way, from the
   same place to the same place.
5. **Given** a fresh vault, **When** the first rule is created, **Then** it is stored under the new names.

---

### User Story 3 - Agents use the new names (Priority: P2)

An agent that manages sync rules gets a tool schema whose fields are `source` and `target`, and the direction
`sourceToTarget` or `both`. A call with the old names is refused with an invalid-input error and changes no rule.

**Why this priority**: The tool schema is the contract the agent sees, and it is part of the vocabulary the
request covers. It matters less than the code and the data.

**Independent Test**: Read the tool schema and the model-eval fixture of the tools; call the tool with the new
names (works) and with the old ones (refused).

**Acceptance Scenarios**:

1. **Given** the sync rule tools of the agent, **When** an agent lists their parameters, **Then** it sees
   `source`, `target` and the new direction values, and the descriptions use the same words.
2. **Given** a call that names `placeA` or `placeB`, **When** the tool runs, **Then** it is refused with an
   invalid-input error (the required `source` and `target` are missing), and no rule changes.

---

### Edge Cases

- A vault that already had the change applied opens again: nothing is renamed twice.
- A rule that ran at the moment of the update: the run ends as on any restart; its state is read under the new
  names afterwards.
- A device that has not updated yet and an updated one share a vault: rules live on the device that runs them
  and never sync (spec 061 FR-003), so the two never see each other's tables.
- An extension or agent that stored the old direction value `aToB` in its own data: not covered; the tools hand
  out no such value to keep.

## Requirements _(mandatory)_

### Functional Requirements

- **FR-001**: Every name that calls a place of a sync rule "A" or "B" MUST become "source" or "target"
  respectively: types, fields, functions, variables, columns, event and command payloads, generated types,
  translation keys, agent tool schemas and fixtures, tests and E2E scenarios. The old place A is the source
  place and the old place B the target place.
- **FR-002**: The storage reference inside a sync rule place MUST be named `location`, so that no place has a
  field `source` inside a `source` place. The type of the reference (`SourceRef`) and every other use of it
  MUST stay as they are.
- **FR-003**: The two sides of the engine (plans, per-file state, pending deletions, the run log) MUST be named
  `source` and `target`; the one-way direction MUST be named `sourceToTarget`; the two-way direction `both`
  MUST stay.
- **FR-004**: A vault created before this change MUST keep all of its rules, per-file state, pending
  deletions and run log, with the old A as source and the old B as target, through a migration of the stored
  columns and values. The migration MUST run once and MUST NOT lose or reorder a row.
- **FR-005**: A rule's behaviour MUST NOT change: same places, same direction, same plan for the same state,
  same files copied, deleted or left alone.
- **FR-006**: The agent tools MUST accept the new names only; there MUST be no alias for the old names.
- **FR-007**: The interface texts ("Quelle", "Ziel") MUST stay as they are; only the translation keys move.
- **FR-008**: Spec 061 and the docs that name the old code names (for example its data model and contracts)
  MUST be brought in line.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: A search for the old names finds 0 hits outside the migrations of spec 061 and of this spec, their tests,
  and the test that refuses an agent call with the old names (SC-004).
- **SC-002**: All existing checks, tests and E2E scenarios of sync rules pass after the change with only their
  names changed, and no test asserts a different behaviour than before.
- **SC-003**: A vault with sync rules, state, pending deletions and run log from before the change opens with
  100% of its rows present and every rule showing the same values; a rule that had nothing to do plans 0
  steps.
- **SC-004**: An agent call that names the places with the old field names instead of `source` and `target` is
  refused in 100% of cases and changes no rule. Like any unknown field, an old name sent next to a complete `source`
  and `target` is ignored; there is no check for the old names (research R5).

## Assumptions

- The name `location` for the storage reference is the author's proposal; the operator only asked that the
  inner `source` be renamed, not what to.
- Holzi has no users yet, so there are no aliases, redirects or compatibility code for the old names (agent
  tools, wire values). The one exception is the stored data of existing vaults (FR-004): the operator's own
  vault and test vaults hold rules, and a changed schema must not strand them.
- The migration is a new one that renames the columns and rewrites the stored side and direction values; the
  already-applied migration of spec 061 is not edited.
- This is a pure rename: no field, state or behaviour is added. Spec 074 adds the new function afterwards.
