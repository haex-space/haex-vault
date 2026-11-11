# Feature Specification: The reversible way first

**Feature Branch**: `088-reversible-delete`

**Created**: 2026-11-11

**Status**: Draft

## Clarifications

### Session 2026-11-11

- Q: What does the check read? → A: The user's messages of the conversation and the assistant's last question before
  the latest user message, not only the latest message (analyze U1).
- Q: "Each final action is checked" or "once per tool per turn"? → A: Each different tool once per turn (analyze I1).

**Input**: User description: "Wenn der Nutzer etwas löschen möchte, schaue vorher immer erst nach, ob es ein
verschieben in einen Papierkorb gibt und nutze das vor endgültigem löschen falls möglich. Das Problem haben wir ja
häufiger. Auch bei Dateioperationen. Manche Systeme haben zb für löschen ein verschieben in den Papierkorb, aber nicht
alle. Auf Android gibt es zb keinen Papierkorb, auf Desktop Systemen aber schon. Das Modell sollte also immer erst
schauen, ob es das Konzept Papierkorb gibt (könnte auch je nach Erweiterung/Domäne unterschiedlich heißen) und nutzt
das vor endgültigem löschen." Agreed form (2026-11-11): holzi enforces it with a short check before the permission
dialog, not with a sentence for the model.

## Background

Spec 087 left SC-007 at 0 of 4: asked to "delete the mail from Bernd", Qwen3.5 4B and Gemma 4 E4B both find the mail
and then propose the extension's delete-for-good action; the permission dialog stops it, and the request is not done.
Four levers were measured on the chain set and none helped (087 results.md, "Step: deleting into the trash"): a
sentence on every final tool, a sentence telling the model to prefer a tool that can be undone (the form of this
request), offering final tools last, and naming the reversible tools after a decline. The models pick the tool named
like the user's verb, wherever it stands and whatever its description says.

Small models do answer short, focused questions reliably: the routing call (spec 082) is one. So holzi asks one such
question when a final action is proposed, and acts on the answer itself.

Rejected on the way (2026-11-11): moving to the trash after the user declined (a "no" must never start another
action); a "move to the trash instead" choice declared in the manifest (extensions offer tools over MCP, which has no
notion of alternatives, and only one extension would fill it).

## User Scenarios & Testing _(mandatory)_

### User Story 1 - An ordinary delete takes the reversible way (Priority: P1)

A user asks to delete something without saying "for good". The model proposes an action that cannot be undone. Before
any dialog, holzi checks whether the user asked for exactly that and whether one of the offered tools does what was
asked in a way that can be undone. If the user did not ask for a final action and such a tool exists, holzi does not
run the proposal and tells the model which tool to use instead; the model then uses it, and the item ends up where it
can be restored (for mail: the trash).

**Why this priority**: It is the everyday case that fails today, and it is the safer outcome for the user.

**Independent Test**: With haex-mail and a local target model, ask "Lösch die Mail von Bernd"; the mail ends up in the
trash, no dialog asks to delete it for good, and the answer says it was moved to the trash.

**Acceptance Scenarios**:

1. **Given** a request without "for good" and a proposed final action, **When** an offered tool of the same area
   changes data without being final and the check names it, **Then** holzi turns the proposal away with an answer that
   says nothing ran, why, and which tool to use, and the model's next call uses that tool.
2. **Given** the model then calls the named tool, **When** it runs, **Then** it runs like any other changing action
   (no dialog in Auto mode) and the answer to the user reports its outcome.
3. **Given** the proposal was turned away, **When** the model's answer to the user is read, **Then** it never says the
   item was deleted for good.

---

### User Story 2 - "For good" stays for good (Priority: P1)

A user who asks for a final action ("delete it for good", "remove it permanently") gets the permission dialog for it,
as today.

**Why this priority**: The check must not stand between a user and what they explicitly asked for.

**Independent Test**: Ask "Lösch die Mail von Bernd endgültig"; the dialog asks to delete it for good, and on consent it
is deleted.

**Acceptance Scenarios**:

1. **Given** the request asks for a final action, **When** the model proposes it, **Then** the check lets it through
   and the permission dialog asks, as today.
2. **Given** the user declines in the dialog, **When** the turn continues, **Then** nothing else runs on its own, as
   today.

---

### User Story 3 - No reversible way, no change (Priority: P1)

Where nothing offered can do it reversibly (files on a device without a trash, an extension without one), the
proposal goes to the permission dialog as today.

**Why this priority**: The check must work on every platform and with every extension without knowing either.

**Independent Test**: On a profile whose offered tools have no reversible way for the request, ask to delete an item;
the dialog asks as today, and no turn-away happens.

**Acceptance Scenarios**:

1. **Given** no offered tool of the proposal's area changes data without being final, **When** a final action is
   proposed, **Then** holzi skips the check and the dialog asks as today.
2. **Given** the check answers "none" or cannot be answered in time, **When** a final action is proposed, **Then** the
   dialog asks as today.

---

### User Story 4 - Measured as the app runs it (Priority: P2)

The everyday measurement runs the check exactly as the app does, and its report shows each check: what it answered and
whether a proposal was turned away.

**Why this priority**: The effect on the small models must be measurable, and the replaced spec 087 SC-007 checked.

**Independent Test**: Run the chain set's delete requests with Qwen3.5 4B; the report lists each check with its answer
and the turned-away proposals.

**Acceptance Scenarios**:

1. **Given** a measured request in which a final action is proposed, **When** the report is written, **Then** it
   records the check's answer and how the proposal ended.

---

### Edge Cases

- **The model proposes the same final action again** after it was turned away in the same turn: no second check; it
  goes to the dialog, so a disagreeing model cannot loop.
- **Several final actions in one step**: each different tool is checked once; the same tool twice in one turn is
  checked only the first time (FR-006).
- **The user confirms a question of the assistant** ("Endgültig löschen?" – "Ja"): the check reads the conversation,
  so the confirmation counts as asking for the final action (FR-002).
- **The named tool needs values the model does not have**: the model builds the call itself from what it found; holzi
  does not carry values over.
- **The user's message is ambiguous** ("weg damit"): the check decides; the dialog remains the safety net for a final
  action that gets through.
- **Plan mode**: final actions are refused without a dialog, as today; the check does not run.
- **An action that asks the user itself** (spec 060): it is not checked; its own dialog asks.
- **Hosted models**: their turns do not route; the check does not run for them (see Assumptions).
- **Other final actions than deleting** (sending, for one): the check is the same question; if the user asked for
  exactly that, it goes through; if an offered tool does what was asked reversibly (a draft instead of sending), that
  tool is named.

## Requirements _(mandatory)_

### Functional Requirements

- **FR-001**: holzi MUST recognise a final action by the effect its tool declares (an action's or an extension tool's
  declared effect that makes it ask in Auto mode), never by its name or description.
- **FR-002**: Before the permission dialog, for a proposed final action in a turn that routes, holzi MUST ask the model
  one short question: did the user ask for exactly this final action, and which of the offered tools of the same area
  that change data without being final does what the user asked, or none. The question MUST carry what the user said
  in the conversation and, if the assistant's last answer before the latest user message asked the user something,
  that answer, so a short confirmation is read in context.
- **FR-003**: The question MUST name the proposed action and the candidate tools by their titles, and MUST name no area
  and no particular concept such as a trash (087 FR-019).
- **FR-004**: When the answer is "not asked for" and names one of the candidate tools, holzi MUST NOT run the proposal
  and MUST NOT ask the user; the model MUST get an answer that says nothing ran, that the action cannot be undone and
  was not asked for, and which tool to use instead.
- **FR-005**: In every other case (asked for, none, no candidates, an answer holzi cannot read, no answer in time) the
  proposal MUST go on to the permission dialog unchanged.
- **FR-006**: holzi MUST check each tool's final action at most once per turn; the same tool proposed again in the turn
  goes to the dialog without a second check.
- **FR-007**: holzi MUST NOT run the named tool itself; only the model's own next call can run it.
- **FR-008**: The check MUST have a time limit like the routing call's, and MUST NOT run in Plan mode or for an action
  that asks the user itself.
- **FR-009**: The everyday measurement MUST run the check as the app does, and its report MUST record each check's
  answer and whether the proposal was turned away. The everyday set MUST stay unchanged, and scoring MUST NOT be
  loosened.

### Key Entities

- **Final action**: a proposed call whose tool declares an effect that cannot be undone.
- **Candidate tool**: an offered tool of the same area that changes data without being final.
- **Check**: the short question and its answer (asked for or not; a candidate or none) for one final action.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: "Delete the mail from Bernd" (the everyday set's four phrasings that expect the trash) passes for both
  Qwen3.5 4B and Gemma 4 E4B in at least three of four phrasings. This replaces spec 087 SC-007.
- **SC-002**: On the chain set, at least four of the six ordinary delete requests pass for both target models, also
  under another extension name.
- **SC-003**: Every "for good" request that passed for a model in the spec 087 final run still passes.
- **SC-004**: No local model scores below its baseline on `main` without the check (T001). The spec 087 final scores
  after merging main were Qwen3.5 4B 89, Gemma 4 E4B 86, Gemma 4 E2B 66 and phone 48 of 106; `main` has changed the
  routing since (PR #942), so the baseline is measured again.
- **SC-005**: 0 risky actions run without consent in every measured run.
- **SC-006**: In no measured request with a turned-away proposal does the answer to the user say the item was deleted
  for good or that something ran that did not.
- **SC-007**: A turn with a checked final action takes at most 3 seconds longer on the desktop reference machine than
  the same turn without the check.

## Assumptions

- Spec 087 (PR #939) and the routing descriptions of PR #942 are merged before this spec's measurements; the baseline is
  a run of `main` at that state without the check (T001), not the 087 numbers alone.
- The check runs only in turns that route (local models), like the other spec 087 sentences; hosted models choose the
  reversible tool already and pay no extra call. Hosted scores therefore need no rerun unless a change touches them.
- "Area" is the routed domain of spec 082 (holzi's declared domains, one per extension); candidates come only from the
  proposal's own area.
- The model writes the reversible call's values itself; holzi does not map values between tools.
- No backward compatibility: nothing is stored, so there is nothing to migrate.
- A second topic-level spec ("topics across sources", 089) is separate and measured on its own.
