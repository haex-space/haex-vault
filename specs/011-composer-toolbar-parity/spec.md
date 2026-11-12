# Feature Specification: Composer Toolbar Parity (Real Effort, Sub-Agent Activity, Attachments)

**Feature Branch**: `011-composer-toolbar-parity`
**Created**: 2026-09-19
**Status**: Draft
**Input**: User description: "I want our input toolbar's design to move closer to Claude Code's own.

1. The effort control should correspond to the model's actual effort. 2. When several (sub-)agents
   are spawned, I want that shown in batches too. 3. I want a '+' button to hand additional documents to
   the agent." Confirmed with the user: implement through the full spec-kit flow, and go end-to-end on
   all three (real backend semantics, not a visual-only pass).

## User Scenarios & Testing _(mandatory)_

### User Story 1 - The effort control does what it says (Priority: P1)

Today, holzi's composer shows a low/medium/high "effort" control regardless of which model or backend
is selected. For the built-in (local) model it only raises a response-length cap; for a connected
Claude Code or Codex delegate it is not passed to the subprocess at all — changing it does nothing. A
user who raises or lowers the effort level should see that choice genuinely change how much reasoning
the model does for whichever backend is answering, and when a selection has no such control at all,
the composer should say so rather than presenting one that quietly has no effect.

**Why this priority**: This is the trust-breaking bug at the center of the request — a control that
looks functional but silently isn't. Fixing it is valuable standalone, independent of the other two
stories, and every other visual change to the toolbar sits on top of getting this control honest.

**Independent Test**: Pick a model/backend that supports graded reasoning effort, change the level,
send a message, and confirm the response was actually generated at that level (not just capped
shorter/longer). Then pick a model/backend with no such control and confirm the composer doesn't show
one for it.

**Acceptance Scenarios**:

1. **Given** a directly-connected model that supports graded reasoning effort, **When** the user
   changes the effort level, **Then** the next response from that model is generated at the newly
   selected level, not merely capped to a different response length.
2. **Given** the Claude Code delegate is selected, **When** the user changes the effort level and
   sends a message, **Then** that request is answered by the delegate actually running at the
   selected level.
3. **Given** a model or backend with no real, controllable effort/reasoning setting, **When** the user
   opens the composer, **Then** the effort control is not shown as active for that selection — it is
   hidden or clearly marked unavailable rather than silently ignored.
4. **Given** the user has not touched the effort control for the active model/backend, **When** they
   send a message, **Then** that model/backend's own sensible default effort is used, not an
   arbitrary holzi-specific default that contradicts it.
5. **Given** a model that only supports a subset of effort levels (e.g. no "extra high"/"max"),
   **When** the user opens the control, **Then** only the levels that model actually supports are
   offered.

---

### User Story 2 - See sub-agent activity while a delegate is working (Priority: P2)

When the Claude Code delegate spawns one or more sub-agents to work on parts of a request, holzi today
gives no sign of it — the composer looks like a single plain request is running. Give the user live
visibility into that activity, grouped by the batch in which sub-agents were dispatched together, the
way Claude Code's own interface does.

**Why this priority**: Depends on nothing from Story 1 and delivers value on its own (visibility into
what's actually happening during a long delegate turn), but is naturally second because it only
matters once a delegate response is already running — Story 1's honesty fix is the more urgent gap.

**Independent Test**: Send a request to the Claude Code delegate that's known to dispatch sub-agents,
and confirm the composer shows a live, updating count of active sub-agents grouped by dispatch batch,
which clears once they finish.

**Acceptance Scenarios**:

1. **Given** a Claude Code delegate response spawns one or more sub-agents, **When** they start
   running, **Then** the user sees an indicator reflecting how many are currently active.
2. **Given** the delegate dispatches several sub-agents together in one step (a batch running in
   parallel), **When** the user views the indicator, **Then** it is clear those sub-agents belong to
   the same batch, rather than looking like one undifferentiated running total.
3. **Given** active sub-agents finish, **When** they complete, **Then** the indicator updates to the
   new active count, disappearing once none remain.
4. **Given** a request that spawns no sub-agents, **When** it runs, **Then** no agent-activity
   indicator is shown at all.
5. **Given** a backend with no sub-agent concept (the local model, a direct API model, or the Codex
   delegate), **When** a request runs, **Then** no agent-activity indicator is shown for it.

---

### User Story 3 - Attach documents to a message (Priority: P3)

Add a "+" control to the composer, positioned and behaving like Claude Code's own, that lets the user
attach one or more files to the message they're about to send so the model can use their content when
answering.

**Why this priority**: Independently useful and independently testable, but the least urgent of the
three — it adds a new capability rather than fixing a broken or invisible one, and neither other story
depends on it.

**Independent Test**: Open the composer, attach a file via the "+" control, confirm it's listed and
removable before sending, then send it to a model that can use that file's content and confirm the
response reflects it.

**Acceptance Scenarios**:

1. **Given** the composer is focused, **When** the user selects the "+" control, **Then** they can
   choose one or more files from their filesystem to attach to the message being composed.
2. **Given** one or more files are attached, **When** the user reviews the composer before sending,
   **Then** each attachment is listed and can be individually removed before sending.
3. **Given** an attached file's content is usable by the selected model/backend, **When** the message
   is sent, **Then** that content is made available to the model as part of answering the request.
4. **Given** an attached file's content is NOT usable by the selected model/backend (e.g. an image
   attached for a text-only local model), **When** the user attaches it, **Then** they are told it
   won't be usable before they send, rather than it being silently dropped.
5. **Given** a file that's too large or an unsupported type, **When** the user tries to attach it,
   **Then** they get a clear, specific reason it can't be attached.
6. **Given** an image or one or more files are on the clipboard and the composer's text field is
   focused, **When** the user pastes (Ctrl+V / Cmd+V), **Then** each becomes an attachment exactly as
   if it had been picked with the "+" control, and the paste inserts no text for it. A paste of plain
   text is unaffected.

---

### Edge Cases

- What happens if the effort level is changed while a response is already streaming? It applies
  starting with the next request; the response already in flight is unaffected.
- What happens when the user switches models/backends mid-conversation, each supporting a different
  set of effort levels? The control refreshes to that selection's own supported set, re-clamping a
  now-unsupported prior choice down to the nearest level that selection does support.
- What happens with a sub-agent that itself spawns a further sub-agent (nesting)? It's counted within
  the same overall active total; visualizing the nesting as a tree is out of scope for this feature.
- What happens to the agent-activity indicator if the user stops an in-progress delegate response
  while sub-agents are still active? It clears immediately, together with the rest of that response's
  in-progress state.
- What happens when the same file is attached twice, or a file disappears from disk before send? Two
  attachments of the same file are both allowed (the user's call); a file that's become unreadable or
  missing by send time is reported to the user and excluded from that send, without failing the rest
  of the message.
- What happens to attachments if the user switches models/backends after attaching but before sending?
  Each attachment's usability (Story 3, Scenario 4) is re-evaluated against the newly selected
  model/backend.

## Requirements _(mandatory)_

### Functional Requirements

**Effort**

- **FR-001**: The system MUST determine, per active model/backend, the real set of reasoning-effort
  levels it actually supports, including the case where it supports none.
- **FR-002**: The effort control MUST offer exactly the set of levels the active model/backend
  supports, and MUST re-clamp a previously chosen level that the newly active selection doesn't
  support down to the nearest level it does support.
- **FR-003**: When the active model/backend supports no controllable reasoning effort, the system MUST
  hide or disable the effort control rather than display one that has no effect.
- **FR-004**: Selecting an effort level MUST cause the underlying request to that model/backend to
  actually run at that level — the real per-request effort parameter for a directly-connected model,
  and the delegate's own effort flag for the Claude Code delegate — rather than only affecting an
  unrelated setting such as a response-length cap.
- **FR-005**: When the user has not chosen an effort level for the active model/backend, the system
  MUST defer to that model/backend's own documented default rather than substituting an arbitrary
  internal default.
- **FR-006**: The response-length limit (maximum output tokens) MUST be presented and controlled
  separately from reasoning effort; the system MUST NOT conflate the two under one "effort" label.

**Sub-agent activity**

- **FR-007**: The system MUST recognize when a Claude Code delegate response spawns one or more
  sub-agents while that response is running.
- **FR-008**: The system MUST group sub-agents by the batch in which they were dispatched together,
  distinguishing several sub-agents running concurrently as one batch from a single sub-agent running
  alone.
- **FR-009**: The system MUST show the user a live count of currently active sub-agents, updating as
  they start and finish, and MUST show nothing once none remain active.
- **FR-010**: The system MUST NOT show a sub-agent indicator for a response that spawns no sub-agents,
  or for a backend with no sub-agent concept.
- **FR-011**: Stopping an in-progress delegate response MUST immediately clear its sub-agent indicator
  along with the rest of that response's in-progress state.

**Attachments**

- **FR-012**: The composer MUST provide a control, positioned and behaving like Claude Code's own
  attach affordance, that lets the user pick one or more files to attach to the message being
  composed.
- **FR-013**: The system MUST list every currently attached file before sending and let the user
  remove any of them individually.
- **FR-014**: The system MUST make an attached file's content available to the model/backend answering
  the request whenever that model/backend can use content of that file's kind.
- **FR-015**: When an attachment's content type is not usable by the currently selected model/backend,
  the system MUST tell the user before the message is sent rather than dropping it silently.
- **FR-016**: The system MUST reject an attachment that exceeds a defined size limit or is an
  unsupported file type, with a clear, specific reason.
- **FR-017**: An attachment MUST be scoped to the single message being composed; it MUST NOT be
  silently carried over and reused on a later message.
- **FR-018**: If an attached file becomes unreadable or goes missing by send time, the system MUST
  tell the user and exclude only that attachment, without failing the rest of the send.
- **FR-019**: Pasting an image or files from the clipboard into the composer MUST attach them the way
  the "+" control does (FR-013 to FR-016 apply unchanged). A pasted image that has no file of its own
  is saved in the app's cache folder (and removed from there after a day); a paste that contains no file
  behaves as before.

_Out of scope for this feature_:

- Adding a reasoning-effort control for the Codex delegate, or for local (in-process) models beyond
  the response-length control they already have — neither is documented to expose a comparable
  setting today (see Assumptions).
- Visualizing nested sub-agent trees beyond a flat active count grouped by dispatch batch.
- New multimodal inference capability for local (in-process) models — an attachment is simply marked
  unusable for that backend (FR-015) rather than this feature building new local multimodal support.
- Persisting a chosen effort level across app restarts, or per model/backend — out of scope the same
  way today's effort setting already resets each session; this feature does not regress or improve
  that.
- holzi originating its own multi-agent orchestration (e.g. a Claude-Code-"ultracode"-style feature).
  This feature only visualizes sub-agent activity the delegate itself already decided to do.

### Key Entities

- **Reasoning Effort Level**: The named degree of reasoning depth (e.g. low/medium/high/extra-high/
  max) a given model/backend actually supports and can be asked to run a request at.
- **Sub-Agent Batch**: A group of one or more sub-agents dispatched together by a single delegate
  response, tracked from when its members start until every member has finished.
- **Message Attachment**: A file the user has attached to the message currently being composed, with a
  name, size, kind, and a usability state (usable / unusable / unreadable) for the active
  model/backend.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: For every model/backend the user can select, the effort control shown always matches
  what that selection actually supports — no user-visible level exists that has zero effect when
  chosen.
- **SC-002**: A user who raises or lowers the effort level observes a real difference in how a
  subsequent response is produced, for both the directly-connected and the Claude Code delegate
  backend.
- **SC-003**: When a delegate response spawns sub-agents, a user can tell — without opening any other
  view — how many are currently active and whether they were dispatched together as a batch.
- **SC-004**: The agent-activity indicator never appears for a response that spawns no sub-agents, and
  always clears within moments of the last sub-agent in a batch finishing or the response being
  stopped.
- **SC-005**: A user can attach a file to a message and send it in the same number of steps Claude
  Code's own interface takes: pick file(s), see them listed, send.
- **SC-006**: A user is told before sending, not after, whenever an attachment they picked won't
  actually be usable by the model/backend they've selected.

## Assumptions

- Grounded in Anthropic's documented `output_config.effort` request parameter (`low`/`medium`/`high`/
  `xhigh`/`max`; not every model supports `xhigh`/`max`, and setting `high` is equivalent to omitting
  the parameter) for the direct API adapter, and in Claude Code's documented `--effort` CLI flag
  (same value set, confirmed to apply in non-interactive/`-p` mode) for the Claude Code delegate —
  verified against Anthropic's and Claude Code's public documentation on 2026-09-19.
- The existing binary "does this model support reasoning at all" allowlist and the
  adaptive-vs-manual-budget `thinking` wiring predate this feature; models with no thinking/effort
  support keep no control shown at all (FR-003), rather than this feature inventing a synthetic one.
- Codex (the app's other CLI delegate) is not documented to expose a comparable effort control; this
  feature does not add one for it. If Codex later documents one, extending parity is a follow-up, not
  part of this feature.
- Sub-agent detection for the Claude Code delegate is based on the documented `parent_tool_use_id`
  field the delegate's stream already carries on sub-agent messages; a batch is the set of sub-agent
  dispatches issued together within one delegate turn. This feature does not require holzi to know
  the exact name of the tool that spawns a sub-agent, only that a later message references an earlier
  tool call as its parent.
- Local (in-process) models and the Codex delegate have no sub-agent concept in scope here; showing no
  indicator for them (FR-010) is the natural, correct behavior, not a gap.
- Attachment usability follows what each backend can already accept: the direct Anthropic API already
  supports image/document content in a request; the Claude Code delegate can be given file content as
  part of what holzi passes it for a turn; local (in-process) models have no attachment/multimodal
  handling today, so an attachment is marked unusable for that backend (FR-015) rather than this
  feature adding new local multimodal capability.
- A per-file size limit and an allowed-file-type list are needed to satisfy FR-016 but their exact
  values are a planning-level decision, not a spec-level one — expected to land in the same range as
  what the providers involved already accept for a single file.
