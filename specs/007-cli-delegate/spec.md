# Feature Specification: CLI Delegate Backend (Claude Code / Codex)

**Feature Branch**: `007-cli-delegate`
**Created**: 2026-09-16
**Status**: Draft
**Input**: User description: "CLI delegate backend: allow HaexVault's chat/agent runtime to delegate a
response to an external CLI-based coding assistant (Claude Code via `claude -p`, or OpenAI Codex via
`codex app-server --stdio`) as a chosen backend, alongside the existing local and api_key providers.
This was explicitly deferred out of scope from spec 003-agent-tool-loop. Prior design exploration
exists in `docs/plans/2026-09-11-agent-tool-loop-design.md` §8 (`cli_delegate`) and
`plans/001-desktop-mvp.md` (cli_delegate credential class)."

> **Partly superseded by spec 094 (CLI tools per vault).** Where this spec says that the tool is found on the
> host (on `PATH`, or through the provider's `base_url`) or that a missing tool is "not installed or not on PATH",
> the tool is now installed into the vault from Settings (`<vault folder>/cli-tools/`), found only there, and a
> delegate's `base_url` carries no program. The per-run folders are no longer in the operating system's temporary
> folder but under the vault home (`<vault folder>/home/`). See `specs/094-vault-cli-tools/spec.md`.

## Clarifications

### Session 2026-09-16

- Q: For a delegate backend that cannot hold a live per-tool-use approval round-trip open during a
  call (Claude Code's one-shot `-p` invocation), what should "always ask" / "ask only for sensitive
  actions" do? → A: Honor the posture as one upfront batch approval — before the call starts, show
  the user the full set of actions that call may take under the active posture, get one approve/deny
  decision for that whole set, then run the call uninterrupted if approved. **Superseded later the
  same session, see below.**
- Q (addendum, same session): The above assumed Claude Code's `-p` invocation cannot hold a live
  per-tool-use approval round-trip open. That assumption was tested directly against an installed
  `claude` CLI (v2.1.241): a `-p` run with `--permission-mode default` and a `--permission-prompt-tool`
  pointed at a purpose-built MCP server produced a genuine live, blocking, per-tool-call round-trip —
  the run paused mid-call, the MCP server received the pending `Bash` call's name and input, and the
  action only proceeded once that server responded. → A: Drop the upfront-batch design. Both delegate
  backends get live per-tool-use approval through HaexVault's existing permission gate (Codex via its
  `app-server` session; Claude Code via `--permission-prompt-tool` bridged into the same gate) — the
  approval posture (Manual/Auto/Plan) applies identically across local, api_key, and both delegate
  backends, with no backend-specific exception.

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Use an existing coding-assistant subscription as the chat backend (Priority: P1)

A user who already pays for a Claude Pro/Max/Team or OpenAI Codex/ChatGPT subscription wants to use
that subscription's own coding assistant, tools included, to answer a request inside HaexVault — instead
of being limited to the local model or having to pay again through a separate API key.

**Why this priority**: This is the core value of the feature — reusing a subscription the user
already has, rather than requiring a second, metered API key just to get comparable capability.

**Independent Test**: Connect a Codex or Claude Code subscription, pick it as the backend for a
message, send it, and confirm the answer is grounded in that assistant's own tool use.

**Acceptance Scenarios**:

1. **Given** a delegate backend has already been connected, **When** the user selects it for a
   request, **Then** the request is answered by that CLI-based assistant, including any tool use it
   performs, and appears in the conversation the same way any other backend's turn does.
2. **Given** no delegate backend has been connected yet, **When** the user opens backend selection,
   **Then** it is shown as "not connected" rather than directly usable, and selecting it starts the
   one-time connection flow (Story 2) instead of sending the request.
3. **Given** a request was answered by a delegate backend, **When** the user reviews the
   conversation afterward, **Then** it is clear which backend answered that turn.

---

### User Story 2 - Connect an existing subscription once, use it anywhere (Priority: P1)

A user connects their existing Claude or Codex subscription to HaexVault a single time, and from then on
can use it as a backend from any copy of their vault — including on a machine that has never logged
into that provider before — without repeating the login.

**Why this priority**: Without portable, one-time credentials, the feature would require
re-authenticating on every device, breaking HaexVault's "carry your vault anywhere" design and making the
backend impractical to actually use.

**Independent Test**: Complete the one-time connection on one machine, open the same vault on a
machine that has never run `claude login`/`codex login`, and confirm the delegate backend works there
without any further login step.

**Acceptance Scenarios**:

1. **Given** a user with an active Claude or Codex subscription, **When** they complete the one-time
   connection flow inside HaexVault, **Then** the resulting credential is stored encrypted in their vault
   and no separate host-level login file is required afterward.
2. **Given** a vault with a connected delegate credential, **When** it is opened on a different
   machine that has never logged into that provider, **Then** the delegate backend works immediately,
   with no additional host-side setup.
3. **Given** a delegate credential has expired or been revoked, **When** the user next tries to use
   that backend, **Then** they are told the connection needs to be renewed and guided back into the
   connection flow, rather than seeing a raw error or a silent failure.

---

### User Story 3 - Delegate actions stay under the user's approval control (Priority: P1)

A user who has already chosen how much the assistant can do on its own (always ask, ask only for
sensitive actions, or block sensitive actions — from 003-agent-tool-loop) expects that same control
to apply when a delegate backend is answering, since a delegate can also run commands and make
changes.

**Why this priority**: Ships alongside Story 1 for the same reason the equivalent story shipped with
003's Story 1 — a tool-capable backend is only acceptable to add alongside real user control over its
sensitive actions.

**Independent Test**: Set an approval posture, pick a delegate backend, trigger a request that would
need a sensitive action, and confirm the outcome matches the chosen posture.

**Acceptance Scenarios**:

1. **Given** the posture blocks sensitive actions, **When** a delegate backend would need to perform
   one, **Then** it is not granted the ability to, and the user is told the action was not available
   for that response.
2. **Given** the posture asks only for sensitive actions, **When** the delegate backend needs to
   perform one, **Then** the user is asked to approve or deny it before it runs.
3. **Given** the posture always asks, **When** the delegate backend wants to use any tool including
   read-only ones, **Then** approval is requested before it runs and the delegate call pauses until
   that decision is made — for both Codex and Claude Code.
4. **Given** a pending approval request from a delegate backend, **When** the user denies it,
   **Then** that specific action does not run, the delegate is told it wasn't approved, and the call
   continues rather than ending outright — the same as a denied tool use ends for the built-in tool
   loop (003-agent-tool-loop, Story 2).

---

### User Story 4 - Delegate sees only what HaexVault gives it, not the host machine (Priority: P2)

A privacy-conscious user expects that when HaexVault hands a request to Claude Code or Codex, that
assistant sees only the conversation and context HaexVault explicitly provides — not the host computer's
own configuration, other projects' instructions, or global settings that happen to already exist on
that machine.

**Why this priority**: Matters for trust and portability but doesn't block Story 1 from delivering
value on a clean machine; becomes important once HaexVault runs on machines that already have their own
Claude Code/Codex setups.

**Independent Test**: Run HaexVault on a machine that already has host-level Claude Code/Codex
configuration and instruction files, trigger a delegate request, and confirm none of that host-level
content affected the response.

**Acceptance Scenarios**:

1. **Given** a machine with existing host-level Claude Code or Codex configuration or instructions,
   **When** a delegate backend answers a request, **Then** none of that host-level content affects
   the response.
2. **Given** a delegate request has completed, **When** the user checks the host machine afterward,
   **Then** no new files or settings were left behind by that request outside HaexVault's own storage.
3. **Given** HaexVault needs to give the delegate background or context for a request, **When** that
   request is sent, **Then** HaexVault passes that context explicitly as part of the request — never by
   relying on the delegate discovering it from files already on disk.

---

### User Story 5 - Stop a delegate response mid-run (Priority: P3)

A user who started a request answered by a delegate backend decides they want it to stop immediately,
the same way they already can for a local or api_key response.

**Why this priority**: Consistency with existing cancellation behavior, but a delegate response can
be waited out if this isn't ready yet, so it doesn't block the core capability from shipping.

**Independent Test**: Start a delegate-backed request doing something observable, stop it, and
confirm the underlying process actually terminates rather than just disappearing from the interface.

**Acceptance Scenarios**:

1. **Given** a running delegate-backed response, **When** the user stops it, **Then** the underlying
   delegate process is terminated and no further output from it is added to the conversation.
2. **Given** a stopped delegate response, **When** the user looks at the conversation afterward,
   **Then** it is marked stopped the same way a stopped local/api_key response is.

---

### Edge Cases

- What happens if the delegate CLI isn't installed on the machine, or an installed version is missing
  a flag HaexVault relies on? The backend is reported unavailable with a specific reason; the system does
  not silently fall back to a different backend.
- What happens if the subscription backing a delegate credential lapses or is revoked mid-response?
  That response ends with a delegate-specific error (comparable to any other tool/generation failure)
  rather than crashing the app; the user is directed to reconnect (Story 2).
- What happens if the user disconnects a delegate credential while a response using it is still in
  flight? The in-flight response is unaffected; disconnecting only applies to requests made
  afterward.
- What happens when the user switches between backends (local, api_key, a delegate) across messages
  in the same conversation? Each request uses whatever backend is selected for it at send time, the
  same as switching already works between local and api_key today.
- What happens if a delegate assistant attempts an action HaexVault has no existing classification for?
  It is treated as sensitive by default (fail-safe), consistent with the tool classification rule in
  003-agent-tool-loop.
- What happens if the process or connection HaexVault uses to receive a delegate's live approval request
  (Story 3) itself fails mid-call (e.g. the bridge to Claude Code's `--permission-prompt-tool` MCP
  server crashes)? The pending action is treated as denied (fail-safe) rather than left hanging
  indefinitely or silently defaulting to allowed.

## Requirements _(mandatory)_

### Functional Requirements

- **FR-001**: The system MUST let the user connect an existing Claude or Codex subscription through a
  one-time login flow, and MUST NOT store the resulting credential in plaintext or unencrypted form.
- **FR-002**: A connected delegate credential MUST be usable from any machine holding that vault,
  including one that has never authenticated with that provider before, without any further
  host-level login step.
- **FR-003**: Connecting or using a delegate backend MUST NOT leave behind any host-level login
  state, configuration file, or credential outside HaexVault's own encrypted storage.
- **FR-004**: The user MUST be able to select a connected delegate backend (Claude Code or Codex) as
  the backend answering a given request, the same way they select the local model or an api_key
  provider today.
- **FR-005**: A response answered by a delegate backend MUST appear in the conversation the same way
  any other backend's response does, including a record of any tool use it performed, and MUST make
  clear which backend answered it.
- **FR-006**: The system MUST apply the user's chosen approval posture to a delegate backend's tool
  use with the same live, per-tool-call approval behavior as the built-in tool loop — pausing that
  specific action until the user decides, for both Codex and Claude Code — rather than deciding tool
  access for a whole call upfront.
- **FR-007**: When the posture blocks sensitive actions, a delegate backend MUST NOT be granted the
  ability to perform them for that request.
- **FR-008**: If a delegate backend is unavailable (not installed, not connected, subscription
  lapsed) when selected, the system MUST tell the user clearly and MUST NOT silently substitute a
  different backend.
- **FR-009**: A delegate backend's request MUST NOT be influenced by, or expose the assistant to, any
  host-level configuration, instructions, or settings belonging to that provider's own CLI, beyond
  what HaexVault explicitly provides for that request.
- **FR-010**: Any project or conversation context a delegate backend needs MUST be passed explicitly
  as part of the request; the system MUST NOT rely on the delegate discovering it from files on the
  host machine.
- **FR-011**: The user MUST be able to stop an in-progress delegate-backed response; stopping MUST
  terminate the underlying delegate process rather than merely hide it from the interface.
- **FR-012**: A stopped delegate-backed response MUST NOT resume or retry automatically, consistent
  with existing stop behavior for other backends.
- **FR-013**: The user MUST be able to disconnect a delegate backend's credential at any time;
  disconnecting MUST NOT affect a response already in progress, only requests made afterward.
- **FR-014**: When a delegate credential has expired or been revoked, the system MUST guide the user
  back into the connection flow rather than failing without explanation.

_Out of scope for this feature_ (see `docs/plans/2026-09-11-agent-tool-loop-design.md` §8-9 and
`specs/003-agent-tool-loop/spec.md`):

- Resolving whether driving each provider's raw CLI (instead of its official Agent SDK) as a
  third-party product complies with that provider's terms of service. This is an accepted, already
  decided open risk at the product level (design doc §8.1); this feature does not block on or
  attempt to resolve it.
- "Subagent-style" full autonomous delegation — the delegate running an entire task fully on its own
  with no approval crossing back to HaexVault. Rejected as this feature's default tool-call path; could
  resurface later as its own, separately-labeled feature (design doc §9).
- Any delegate provider beyond Claude Code and Codex.
- Remembering or persisting an individual approval decision across future requests — an inherited
  limitation from 003-agent-tool-loop, unchanged here.
- Using a delegate backend from a mobile (iOS/Android) HaexVault instance. Spawning `claude`/`codex` as a
  subprocess is categorically impossible on mobile OS sandboxing, with no workaround at the
  implementation level — this feature is desktop-only (plan.md Technical Context). Reverse-engineering
  each vendor's private subscription-authenticated HTTP API to bypass the subprocess requirement was
  considered and rejected (2026-09-16, operator decision): it would be a materially larger ToS risk
  than the already-accepted raw-CLI risk (no official software involved at all) against an
  undocumented, unversioned protocol. Mobile access is intended as a later, separate feature: a mobile
  instance relaying chat and approval prompts to a desktop-hosted delegate session over HaexVault's
  planned peer-sync layer (iroh) — not addressed by this spec.

### Key Entities

- **Delegate Backend**: An external CLI-based coding assistant (Claude Code or Codex) usable as a
  chat backend. Distinguished from the local model and api_key providers by using the user's existing
  subscription rather than per-token billing, and by running as a separate process rather than a
  direct API call.
- **Delegate Credential**: The one-time-obtained, encrypted-at-rest proof of an existing subscription
  (e.g. a Claude OAuth token or a Codex login result), stored in the vault, portable across machines,
  usable without any host-level login state.
- **Delegate Invocation**: One request answered by a delegate backend — a held-open exchange capable
  of live, per-tool-call approval round-trips for the duration of that single request, for both
  Codex and Claude Code.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: A user with an existing Claude or Codex subscription can go from a fresh connection to
  a completed, delegate-answered request in one sitting, without touching anything outside the app.
- **SC-002**: A vault carrying a connected delegate credential produces a working delegate backend on
  a newly set up machine with zero additional provider-side login steps.
- **SC-003**: 100% of sensitive actions a delegate backend attempts are blocked when the user's
  posture blocks sensitive actions, with no exceptions.
- **SC-004**: No host-level file or setting belonging to a delegate provider's own CLI is created,
  modified, or read as a side effect of using that delegate backend.
- **SC-005**: A user who stops a delegate-backed response sees the underlying process actually end,
  not merely disappear from view, within the same short window stopping already takes for other
  backends.
- **SC-006**: When a delegate backend is unavailable, the user always gets a specific, actionable
  explanation rather than a generic error or a silent switch to another backend.

## Assumptions

- Builds directly on the tool loop, permission gate, and turn/cancellation mechanics established in
  003-agent-tool-loop; this feature adds delegate backends as a new way to answer a turn, not a new
  turn model.
- Codex's `app-server` protocol supports live, answerable approval requests over a persistent
  session. This was verified against codex-cli 0.147.0 on 2026-09-16 using
  `item/commandExecution/requestApproval` and `{"decision":"accept"|"decline"}` responses.
- Claude Code's `-p` invocation supports a live, blocking, per-tool-call approval round-trip within a
  single call via `--permission-prompt-tool` (an MCP tool HaexVault provides) — verified directly against
  an installed CLI (v2.1.241) on 2026-09-16: a pending `Bash` call genuinely paused until the
  MCP-side response arrived. Also verified the same day: an isolated, empty `CLAUDE_CONFIG_DIR`
  blocks both credential lookup ("not logged in" despite a logged-in host) and host-level hook
  execution (a `SessionStart` hook defined in the host's `~/.claude/settings.json` fired in a
  non-isolated run and did not fire once `CLAUDE_CONFIG_DIR` pointed at an empty directory). This
  feature therefore uses `CLAUDE_CONFIG_DIR` + a disposable `cwd` (not `--bare`), since `--bare` does
  not support subscription/OAuth login at all (API key only) and this feature is specifically the
  subscription path (FR-001). Not yet verified: whether every kind of host-level discovery (skills,
  plugins, MCP auto-discovery) is equally blocked by this mechanism the way `--bare` guarantees, or
  only hooks/credentials/settings as tested — worth a narrow pre-implementation check, not a
  scope-affecting unknown.
- The isolated `CLAUDE_CONFIG_DIR` also blocks host-level skills and plugins: the 2026-09-16
  comparison found only the CLI's bundled skills and no user plugins in the isolated run. Regression
  coverage retains this result alongside the existing credential and hook isolation checks.
- The ToS-compliance question for driving the raw CLI as a third-party product is treated as an
  already-decided, accepted risk at the product level (design doc §8.1), not something this feature
  re-litigates before shipping — but current official docs (`code.claude.com/docs/en/headless`,
  fetched 2026-09-16) now describe `claude -p` itself as "the Agent SDK via the CLI," which is new
  information since the design doc's 2026-09-11 "genuinely unclear" framing and makes the ToS clause
  more likely to apply, not less. Carried forward as an accepted risk per the existing operator
  decision, not re-opened here, but the risk is now better-informed than when that decision was made.
- One-time connection uses each provider's own subscription-login mechanism (`claude setup-token` for
  Claude, `codex login` for Codex), not an API key — this feature is specifically the "bring your
  existing subscription" path, distinct from the existing api_key provider.
- Backend selection (local / api_key / a connected delegate) is made per request, consistent with how
  provider choice already works elsewhere in HaexVault.
- A delegate process that doesn't gracefully acknowledge a stop request at the protocol level is
  still expected to be killed outright at the OS process level on stop, as a safe upper bound.
