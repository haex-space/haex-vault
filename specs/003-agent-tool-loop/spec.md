# Feature Specification: Agent Tool Loop

**Feature Branch**: `003-agent-tool-loop`
**Created**: 2026-09-11
**Status**: Draft
**Input**: User description: "Agent tool loop: turn/step model, tool-calling (built-in tools,
MCP-client tools, host-CLI tool), permission gate (Manual/Auto/Plan modes), LLM-request retries with
backoff, and turn cancellation — for holzi's chat/agent runtime. Based on the agreed design in
docs/plans/2026-09-11-agent-tool-loop-design.md. Scope: the local + api_key provider tool loop
(turn/step, tool registry, permission gate, storage extensions, retry, cancellation). Delegating to
an external CLI-based coding assistant (e.g. Claude Code / Codex) as a backend is a separate,
still-partially-open-risk track and is explicitly out of scope here."

## Clarifications

### Session 2026-09-11

- Q: Wie weit reicht der Zugriff des Host-CLI-Tools technisch — nur durch das Freigabe-Gate begrenzt, oder zusätzlich technisch eingeschränkt (Arbeitsverzeichnis, Befehls-Blacklist)? → A: Nur Freigabe-Gate — kein technisches Sandboxing; die Freigabe (Manual/Auto/Plan) ist der einzige Schutzmechanismus, wie bei Claude Codes eigenem Bash-Tool.
- Q: Soll es eine harte Obergrenze für Tool-Nutzungs-Runden innerhalb einer einzelnen Antwort geben? → A: Ja, feste Obergrenze — nach einer festen Anzahl Runden bricht die Antwort mit einer klaren Fehlermeldung ab, analog zu Claude Codes `maxTurns`/`--max-turns` und zum bereits beschlossenen Retry-Limit (FR-012).

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Assistant completes a task using tools (Priority: P1)

A user asks the assistant something that requires more than generating text — for example,
inspecting a file the assistant has access to, or running a command on the user's computer. Instead
of the assistant describing what the user should do manually, it performs the action itself and
uses the result to produce its answer, possibly going back and forth (act, observe, act again)
before giving a final answer.

**Why this priority**: This is the core capability being added. Without it, the rest of the feature
(permissions, retries, cancellation) has nothing to govern.

**Independent Test**: Ask the assistant a question that requires reading a piece of information it
does not already have (e.g., "what's in file X" for a tool it has access to, or a request that
implies running a command). Verify the assistant performs the action and its final answer reflects
the actual result, not a guess.

**Acceptance Scenarios**:

1. **Given** a conversation with an assistant that has at least one tool available, **When** the
   user asks something that requires that tool, **Then** the assistant uses the tool and its final
   answer is grounded in the tool's actual result.
2. **Given** an assistant response that required two separate tool uses to answer fully, **When**
   the response completes, **Then** the conversation shows both tool uses and their results, in
   order, alongside the assistant's final answer.
3. **Given** a tool use that fails (e.g., a file does not exist), **When** the assistant receives
   that failure, **Then** it continues the response (e.g., explains the failure or tries another
   approach) instead of the whole response ending in an unrecoverable error.

---

### User Story 2 - User controls which actions need approval (Priority: P1)

A user wants to decide how much the assistant can do on its own versus how much needs their
explicit go-ahead — especially for anything that runs a command on their computer or changes data,
as opposed to actions that only read information. The user picks one of three postures: always ask
first, ask only for sensitive actions, or don't allow sensitive actions at all right now.

**Why this priority**: Tool use that can run commands or change things is only acceptable to ship
alongside real user control over it — this is not a follow-up, it ships with Story 1.

**Independent Test**: Set the approval posture to "always ask", trigger a tool use, and verify the
assistant waits for an explicit yes/no before doing anything. Switch posture and repeat to see the
behavior change accordingly.

**Acceptance Scenarios**:

1. **Given** the posture is "always ask", **When** the assistant wants to use any tool (including
   read-only ones), **Then** the user is asked to approve or deny before it runs.
2. **Given** the posture is "ask only for sensitive actions", **When** the assistant wants to use a
   read-only tool, **Then** it runs without interrupting the user; **When** it wants to run a
   command on the user's computer or another sensitive action, **Then** the user is asked first.
3. **Given** the posture is "don't allow sensitive actions", **When** the assistant wants to run a
   command on the user's computer or another sensitive action, **Then** it is not run, no approval
   prompt appears, and the assistant is told the action was not allowed right now.
4. **Given** a pending approval request, **When** the user denies it, **Then** the assistant is told
   the action was denied and continues the response instead of stopping entirely.
5. **Given** a pending approval request, **When** the user takes no action for an extended period,
   **Then** the response remains paused waiting for that decision (no silent timeout that runs or
   skips the action on the user's behalf).

---

### User Story 3 - User stops a response mid-action (Priority: P2)

A user realizes partway through a response — including while the assistant is actively running
something — that they want it to stop right away, not once the current step finishes.

**Why this priority**: Builds directly on existing cancellation behavior for plain text generation;
becomes more important once actions (especially commands) can run, but is not required for Story 1
and 2 to deliver value.

**Independent Test**: Start a response that involves a running action, stop it partway through, and
verify the action itself is halted immediately rather than left running in the background.

**Acceptance Scenarios**:

1. **Given** a response with a running action, **When** the user stops it, **Then** the running
   action is halted immediately and the response ends there — no further action or reply is
   generated for that request.
2. **Given** a stopped response, **When** the user looks at the conversation afterward, **Then** it
   is clearly marked as stopped, and the assistant does not automatically continue or retry it on
   its own — a stopped response only continues if the user sends a new message.
3. **Given** a response that is paused waiting for the user's approval decision (Story 2), **When**
   the user stops it instead of answering, **Then** it stops the same way as a running action would.

---

### User Story 4 - Assistant recovers from temporary connection problems (Priority: P3)

A user's response fails because of a brief network or service hiccup, not because of anything wrong
with the request itself. Instead of immediately showing an error, the assistant quietly tries again
a bounded number of times.

**Why this priority**: Improves reliability but the feature is fully usable without it — a user can
already manually resend a failed message today.

**Independent Test**: Simulate a temporary failure partway through generating a response and verify
the user sees a normal completed answer rather than an error, with no partial/broken attempt visible
in the conversation afterward.

**Acceptance Scenarios**:

1. **Given** a response generation that fails due to a temporary problem, **When** a retry
   succeeds, **Then** the user sees only the final successful answer — no failed attempt appears in
   the conversation history.
2. **Given** a response generation that keeps failing beyond the retry limit, **When** the assistant
   gives up, **Then** the user sees a clear error, distinguishable from a response that was
   deliberately stopped (Story 3).

---

### Edge Cases

- What happens if the assistant tries to use a tool that no longer exists or is no longer available
  (e.g., an MCP server disconnected mid-conversation)? The attempt fails as a tool error (Story 1,
  scenario 3), not a whole-response failure.
- What happens if two sensitive actions are requested in the same response? Each is approved or
  denied on its own; approving one does not silently approve the other.
- What happens if the user changes the approval posture while a response is already in progress?
  The change applies to the next tool use onward within that response; a request already sent for
  approval keeps waiting under the posture that was active when it was asked.
- What happens if the assistant keeps needing more tool-use rounds than the fixed maximum allows
  (FR-016)? The response ends with a "limit reached" message rather than looping indefinitely; this
  is treated the same as any other terminal outcome, distinguishable from a user-initiated stop
  (Story 3) and from a retry exhaustion (Story 4).
- What happens if a stopped response's action had already produced a partial, real-world effect
  (e.g., a command that was already halfway done)? The system stops the process immediately but
  cannot undo an effect the action already had — this is a known limitation, not a defect.

## Requirements _(mandatory)_

### Functional Requirements

- **FR-001**: The assistant MUST be able to use tools — including one that runs a command on the
  user's computer, and tools made available through the user's configured external tool servers —
  as part of producing a response.
- **FR-002**: A single response MUST be able to involve more than one round of tool use (act,
  observe the result, act again) before reaching a final answer.
- **FR-003**: The system MUST let the user choose one of three approval postures at any time:
  always ask before any tool use, ask only before sensitive actions, or block sensitive actions
  entirely.
- **FR-004**: The system MUST classify every tool as either read-only/safe or sensitive; a tool that
  runs a command on the user's computer MUST always be classified as sensitive, never read-only.
- **FR-005**: Whenever the active approval posture requires it, the system MUST pause and wait for
  an explicit user decision before running a tool, and MUST NOT run it, skip it, or auto-decide on
  timeout.
- **FR-006**: When the posture blocks sensitive actions, the system MUST NOT run them and MUST NOT
  prompt the user for them; the assistant is informed the action is unavailable right now instead.
- **FR-007**: The system MUST record every tool use in the conversation — what was used, what it was
  given, and what it returned (or that it was denied/blocked) — visible to the user afterward.
- **FR-008**: If a tool use itself fails (distinct from the approval being denied), the system MUST
  let the response continue rather than ending it outright.
- **FR-009**: The user MUST be able to stop a response at any point, including while a tool is
  actively running or while an approval decision is pending.
- **FR-010**: Stopping a response MUST immediately halt any tool currently running for it, and MUST
  NOT trigger any further tool use or reply generation for that same request.
- **FR-011**: A stopped response MUST NOT resume or retry automatically; only a subsequent user
  message continues the conversation.
- **FR-012**: The system MUST automatically retry a response's generation a bounded number of times
  when it fails for a transient/temporary reason, before surfacing an error to the user.
- **FR-013**: A response that only succeeds after one or more automatic retries MUST appear to the
  user as one normal, complete answer — earlier failed attempts MUST NOT appear in the conversation.
- **FR-014**: A response that exhausts its retries MUST show the user a clear failure, distinguishable
  from one the user deliberately stopped.
- **FR-015**: The host-command tool MUST NOT be technically restricted (no confined working
  directory, no command blocklist) beyond the approval posture itself — the approval decision (FR-003
  through FR-006) is the sole safeguard, matching how the user already expects this to work from
  comparable coding-assistant tools.
- **FR-016**: A single response MUST be bounded to a fixed maximum number of tool-use rounds; when
  that maximum is reached without a final answer, the response MUST end with a clear "limit reached"
  marker rather than continuing indefinitely. The last step offers no tools and asks the model to say
  what it found and what is left, so the response never ends empty; that instruction is not part of the
  conversation.

_Out of scope for this feature_ (see `docs/plans/2026-09-11-agent-tool-loop-design.md` §8-9):
delegating a response to an external CLI-based coding assistant (e.g. Claude Code, Codex) as a
backend; any specific catalog of built-in tools beyond the host-command tool (individual read/write
tools are separate, incremental feature work built on this same mechanism); remembering/persisting
an approval decision across future requests ("always allow this specific action").

### Key Entities

- **Tool**: Something the assistant can invoke to produce a real-world effect or fetch information
  it does not already have. Has a name, a description of what it does, and a safety classification
  (read-only/safe vs. sensitive). Comes from one of: a fixed set the app provides, a server the user
  has configured, or the ability to run a command on the user's computer.
- **Approval Posture**: The user's current choice governing whether tool use needs explicit
  confirmation — always ask, ask only for sensitive actions, or block sensitive actions.
- **Response (Turn)**: Everything produced for one user message, from the first action through to a
  final answer, possibly spanning several rounds of tool use. Stopping applies to the whole response,
  not to one round within it.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: A user can ask for something requiring at least one tool use and get a complete,
  accurate answer without performing any action themselves outside the conversation.
- **SC-002**: With "always ask" selected, 100% of tool uses — read-only or sensitive — pause for the
  user's explicit approval before running.
- **SC-003**: With "ask only for sensitive actions" selected, read-only tool use never interrupts the
  user, while every sensitive action still pauses for approval.
- **SC-004**: With sensitive actions blocked, no sensitive action ever runs and no approval prompt
  for one ever appears.
- **SC-005**: A user who stops a response with a running action sees that action stop within the
  same short window stopping already takes today for plain text generation — not only once the
  action would have finished on its own.
- **SC-006**: A stopped response never continues or produces further output on its own afterward.
- **SC-007**: The large majority of transient generation failures are resolved automatically without
  the user ever seeing an error.
- **SC-008**: Every tool use in a conversation can be inspected afterward — what ran, with what
  input, and what it returned — with no gaps.

## Assumptions

- Default approval posture for a user who has not chosen one is "always ask" (the safest option),
  matching the brainstorming session's bias toward user control by default.
- The retry limit and backoff timing for transient failures are an implementation default (a small,
  bounded number of attempts with increasing delay), not a user-facing setting in this feature.
- The maximum number of tool-use rounds per response (FR-016) is likewise an implementation default,
  not a user-facing setting in this feature — chosen generously enough not to interrupt normal
  multi-step tasks.
- The specific catalog of built-in, safe/read-only tools shipped at first is a separate, follow-on
  decision; this feature's requirements hold for whatever tools exist, including zero built-ins
  beyond the host-command tool and whatever the user's configured external tool servers provide.
- "The user's configured external tool servers" refers to MCP servers the user has already set up
  elsewhere in the app; configuring those servers themselves is not part of this feature.
- Approval decisions are not remembered between requests in this feature (see Out of scope above) —
  each sensitive action under "always ask" or "ask only for sensitive actions" is judged on its own
  each time it occurs.
