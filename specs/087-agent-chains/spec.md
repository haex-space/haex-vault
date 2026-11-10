# Feature Specification: Small local models finish what they start

**Feature Branch**: `087-agent-chains`

**Created**: 2026-11-06

**Status**: Draft

**Input**: User description: "Agent-Ketten für lokale Modelle (Nachfolger von Spec 082)" (operator, 2026-11-06), after
the follow-up measurements of spec 082. The failure analysis and the operator's decisions are in the Background and
Decisions below.

## Background

- **Where spec 082 left off**: holzi routes a request to the right area of the app before the model's first step, and
  since #847 it does so whatever an extension is called. Under the tool names the app really uses, neither target
  model reaches the goal of spec 082 (75 % of the everyday set, 106 requests):

  | Model            | Everyday set under the app's names | Under another extension name |
  | ---------------- | ---------------------------------- | ---------------------------- |
  | Qwen3.5 4B       | 78 (73.6 %)                        | 78                           |
  | Gemma 4 E4B      | 77 (72.6 %)                        | 77–81                        |
  | Gemma 4 E2B      | 56                                 | 55                           |
  | Qwen3 1.7B phone | 41                                 | 49                           |

  The name an extension gives its tools alone moves Gemma 4 E4B by up to five requests.

- **Where they fail** (both target models, 2026-11-06, each failed request counted once by its first cause):
  - **The chain stops early** (13): the model finds the right mail or password and then asks "Shall I show it?" or
    stops, instead of showing it, answering it or moving it.
  - **A tool search instead of the offered tools** (13, almost all Gemma 4 E4B): the area's tools are offered, the
    model searches for tools anyway, then asks the user which mail is meant.
  - **Wrong arguments** (9, mostly Qwen3.5 4B): a yes/no value written as text (`"True"`), "yesterday" read as "since
    yesterday".
  - **No call at all** (5): the model asks "Which mails do you mean?" where it could have looked them up.
  - **"Delete the mail from Bernd"** fails for both models in all four phrasings. Qwen3.5 4B finds the mail and
    proposes deleting it for good; deleting a mail usually means moving it to the trash, so the played user declines
    the permission dialog. Gemma 4 E4B searches for tools and asks the user instead.

- **By category** (failed of tried, both models): reply to a mail 9/12, clean up mail 14/24, read mail 16/32,
  passwords 4/12. Contacts, new mail, files, settings and the time mostly work.

## Decisions

The operator decided on 2026-11-06 (and before, for spec 082, where still in force):

1. **Goal**: Qwen3.5 4B and Gemma 4 E4B each pass at least 75 % of the everyday set under the tool names the app
   uses. Replying to a mail and cleaning up mail each pass at least half for both. Hosted models get no worse.
   Gemma 4 E2B and Qwen3 1.7B (phone) are measured without a goal of their own.
2. **Safety stays as decided in spec 082**: nothing risky (send, delete, command) is carried out without the user's
   consent; the measurement plays the permission dialog; unasked changes are counted on their own.
3. **No quick shots**: every change is measured on its own against the baseline. Wording that the model reads
   (descriptions, instructions) is tuned on the development set only, never on the everyday set.
4. **Hosted models do not route** (spec 082), and their results must not get worse.
5. **No backward compatibility**: holzi has no users yet; names, wording and the order of tools can change freely.
6. **The directions are open**: looking up before asking, the tool search beside offered tools, accepting arguments
   as the tool means them, deleting into the trash, and showing what was found are to be judged by measurement, not
   decided up front.

## User Scenarios & Testing _(mandatory)_

### User Story 1 - The baseline is the app as it is now (Priority: P1)

Before anything changes, every model is measured once more on the current state of the app: the tool names the app
uses, the routing by meaning (#847), the tools a turn has called staying offered (#853), the extension prefixes
with every letter (#856), and the context a local model gets (#903, #904, spec 086). This baseline, and not an older run, is what every later step is compared with.

**Why this priority**: three changes landed since the last runs; a step judged against an older run would credit or
blame it for them.

**Independent Test**: run the everyday set for the target models, the phone models and the hosted models on the
current state; each report is kept and named as the baseline.

**Acceptance Scenarios**:

1. **Given** the current state of the app, **When** the everyday set runs for a model, **Then** its report names the
   tool names the run used and is kept as that model's baseline.
2. **Given** a later step, **When** its result is reported, **Then** it is compared with this baseline, request by
   request.

---

### User Story 2 - The model looks it up instead of asking (Priority: P1)

A user asks "What does the property management write?" or "Answer Anna". holzi has offered the mail tools for the
request. The model searches the mails itself, with what the user said (a sender, a subject, a day), instead of asking
the user which mail is meant or searching for other tools first.

**Why this priority**: asking back and searching for tools are the largest causes after the chain stopping early (18
of the target models' failures), and the user experiences both as "holzi cannot do it".

**Independent Test**: on the development set, requests that name a sender, subject or day lead to a search with the
offered search tool as the first or second call; on the everyday set, the number of failed requests without the
right first call goes down.

**Acceptance Scenarios**:

1. **Given** a request that names a sender and the mail tools offered, **When** the model takes its first step,
   **Then** it searches the mails for that sender before it asks the user anything.
2. **Given** the area's tools are offered, **When** the model needs one of them, **Then** it calls it without
   searching for tools first.
3. **Given** a search that finds nothing, **When** the model answers, **Then** it says so or asks the user for more,
   and does not invent a mail.

---

### User Story 3 - A started chain is finished (Priority: P1)

The user asks to read, answer or clean up a mail, or to see a password. The model finds what the request is about
and then does what was asked: shows the content, drafts the answer, moves the mail. It does not stop with "Shall I
show it?" when showing it was the request.

**Why this priority**: chains that stop early are the largest single cause (13), and replying to a mail is the
weakest category (9 of 12 failed).

**Independent Test**: on the everyday set, the requests of reading, answering and cleaning up mail and of passwords
pass more often, and fewer failures end with a question after a successful search.

**Acceptance Scenarios**:

1. **Given** "Show me the Netflix password" and the password found, **When** the model goes on, **Then** it shows the
   password without asking whether to show it.
2. **Given** "Answer Anna, I'll come on Saturday" and Anna's mail found, **When** the model goes on, **Then** it
   drafts the answer to that mail.
3. **Given** "Move the newsletters from shop.de to the archive" and the mails found, **When** the model goes on,
   **Then** it moves them.
4. **Given** a request that really is unclear (several mails could be meant, nothing names one), **When** the model
   has searched, **Then** asking the user is still right and counts as right.

---

### User Story 4 - An argument counts as the tool means it (Priority: P2)

The model writes a value in a form the tool does not expect, but its meaning is clear: `"True"` for yes, `"5"` for
the number 5. holzi takes the value as the tool means it instead of letting the call fail. A value whose meaning is
not clear still fails with a message that says what was expected.

**Why this priority**: wrong arguments are the largest cause for Qwen3.5 4B (7 of its failures); part of them are
plain form errors.

**Independent Test**: calls with clearly meant values pass in a test without a model; on the everyday set, Qwen3.5
4B has fewer failures from wrong arguments.

**Acceptance Scenarios**:

1. **Given** a tool that expects yes/no, **When** the model sends `"True"` or `"false"`, **Then** the call runs with
   that value.
2. **Given** a tool that expects a number, **When** the model sends `"5"`, **Then** the call runs with 5.
3. **Given** a value that could mean more than one thing, **When** the model sends it, **Then** the call is turned
   away with what was expected, as today.
4. **Given** a risky tool, **When** a value is taken as meant, **Then** the permission dialog shows the value that
   will be used.

---

### User Story 5 - Deleting a mail moves it to the trash (Priority: P2)

The user says "Delete the mail from Bernd". The model moves the mail to the account's trash, as mail programs do.
Deleting for good is proposed only when the user asks for it ("for good", "permanently", "empty the trash").

**Why this priority**: both target models fail this request in all four phrasings, and the user meets it every time
they clean up.

**Independent Test**: on the development set and the everyday set, a request to delete a mail without "for good"
leads to the move into the trash.

**Acceptance Scenarios**:

1. **Given** "Delete the mail from Bernd", **When** the model acts, **Then** the mail ends up in the trash, without
   a permission dialog in Auto mode.
2. **Given** "Delete the mail from Bernd for good", **When** the model acts, **Then** it proposes deleting for good
   and the permission dialog asks.
3. **Given** a mail extension other than haex-mail with a move and a delete tool, **When** the user asks to delete a
   mail, **Then** the same holds without holzi knowing that extension.

---

### User Story 6 - Hosted models and phone models are measured with every step (Priority: P2)

Every step that changes what a model reads or what holzi does with a call is measured for the hosted models and the
phone models too. A step that makes a hosted model worse is not kept as it is.

**Why this priority**: the goal is for the target models, but the app runs the same harness for every model.

**Independent Test**: each step's results list the hosted models and the phone models beside the target models.

**Acceptance Scenarios**:

1. **Given** a step that is kept, **When** its results are reported, **Then** every hosted model passes at least as
   many requests as in its baseline.
2. **Given** a step, **When** its results are reported, **Then** Gemma 4 E2B and Qwen3 1.7B (phone) are listed.

---

### Edge Cases

- A search returns several mails and the request names none of them in particular: asking the user which one is
  right, and stays right.
- A request to delete where the extension has no move tool or no trash: FR-010 applies where a trash is offered; where
  there is none, an ordinary delete request MUST NOT lead to proposing a delete for good: the model tells the user
  that the extension offers no way to the trash and proposes the permanent delete only once the user asks for it
  explicitly (FR-010). With a move tool present, the move into the trash applies.
- A value taken as meant changes what a risky call does: the dialog shows the value used, and nothing runs without
  consent.
- The model searches, then the search tool's result is empty: it must not answer as if it found something.
- An extension's tools are named after the extension and not after their topic (the name effect of up to five
  requests): the result is reported under the app's names and under another name.
- A step helps one target model and hurts the other: it is reported per model, and only kept if both stay at or above
  their baseline.

## Requirements _(mandatory)_

### Functional Requirements

- **FR-001**: The everyday set MUST measure every model on the current state of the app before any change of this
  feature, under the tool names the app uses; this is the baseline.
- **FR-002**: Each change of this feature MUST be measured on its own against the baseline, per model and per request,
  and kept only if neither target model gets worse.
- **FR-003**: When the offered tools of a request's area include a search, the model MUST be led to search with what
  the user said before it asks the user to name a mail, contact, file or password.
- **FR-004**: When the area's tools are offered, the model MUST be led to use them rather than search for tools first.
- **FR-005**: When a request asks to see, answer or move something and the model has found it, the model MUST be led
  to do so instead of asking whether it should.
- **FR-006**: A request that names nothing the model could search for, or whose search finds several equally fitting
  results, MUST still allow, and count as right, a question back to the user.
- **FR-007**: holzi MUST accept an argument whose form differs from what the tool expects when its meaning is clear
  (yes/no written as text, a number written as text), and MUST run the call with the meant value.
- **FR-008**: holzi MUST keep turning away an argument whose meaning is not clear, with a message that says what was
  expected.
- **FR-009**: For a risky call, the permission dialog MUST show the values that will be used, after any value was
  taken as meant.
- **FR-010**: A request to delete a mail that does not ask for a final delete MUST lead to moving the mail to the
  trash where the mail extension offers that; deleting for good MUST only be proposed when asked for.
- **FR-011**: FR-010 MUST hold for any mail extension that offers a move and a delete, without holzi naming a
  particular extension.
- **FR-012**: Wording the model reads (tool descriptions, instructions) MUST be tuned on the development set only and
  then measured once on the everyday set.
- **FR-013**: No change of this feature may let a risky call run without the user's consent.
- **FR-014**: Every step's results MUST list the hosted models and the phone models beside the target models.
- **FR-015**: The final results MUST report the target models under the app's names and under one other extension
  name.

### Key Entities

- **Baseline**: one everyday report per model on the current state of the app, before this feature changes anything.
- **Step**: one change, measured on its own against the baseline; kept or dropped by its numbers.
- **Development set**: the requests used to tune wording (spec 082), separate from the everyday set.
- **Chain**: the calls one request needs in order (find, then read, answer, move or show).

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: Every target model, phone model and hosted model has a baseline report on the current state of the app
  before the first change.
- **SC-002**: Qwen3.5 4B and Gemma 4 E4B each pass at least 75 % of the everyday set (80 of 106) under the app's tool
  names.
- **SC-003**: Qwen3.5 4B and Gemma 4 E4B each pass at least half of the requests to reply to a mail and at least half
  of the requests to clean up mail.
- **SC-004**: No risky call is carried out without consent in any final run.
- **SC-005**: Each hosted model passes at least as many requests as in its baseline.
- **SC-006**: Failures where the model stops after a successful search, or asks the user instead of searching, are at
  most half of their number in the baseline, summed over both target models.
- **SC-007**: "Delete the mail from Bernd" passes for both target models in at least three of its four phrasings.
- **SC-008**: The final results give both target models' pass counts under the app's names and under one other
  extension name, and list Gemma 4 E2B and Qwen3 1.7B (phone).

## Assumptions

- **The everyday set stays as it is**: its 106 requests and their expected outcomes do not change in this feature.
- **The development set** of spec 082 is the place to tune wording; requests for the new directions (looking up,
  finishing a chain, deleting into the trash) may be added to it, never to the everyday set.
- **Measuring cost**: one run takes 10 to 20 minutes per target model on the reference laptop and longer for the
  hosted models; each step needs at least the two target models, the full set of models for kept steps.
- **haex-mail stays pinned** as in spec 078, under the app's names (spec 082 follow-up). Its tool names and
  descriptions are holzi's measuring fixture, not something this feature changes.
- **Speed is not this feature**: routing cost and prompt length belong to spec 080.
- **Answer drafts are not sent**: answering a mail ends with a draft as in spec 078; sending stays a risky call.
