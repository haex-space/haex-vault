# Feature Specification: Autonomous Delegate Mode

**Feature Branch**: `009-autonomous-delegate-mode`
**Created**: 2026-09-17
**Status**: Draft
**Input**: User description: "Autonomous delegate mode: let a connected CLI delegate backend (Claude
Code or Codex, from 007-cli-delegate) run with its own native tool autonomy instead of holzi's live
per-tool-call approval gate, chosen explicitly per request rather than as a new global
default. Two variants: an ungated mode that spawns the delegate with its own native full-autonomy
flag and only captures final output, and a gated-but-permissive mode that reuses 007's existing
--permission-prompt-tool/app-server approval bridge but auto-allows everything except an
operator-configured deny list, giving a full persisted tool-call audit log for the same
latency cost 007 already pays. This does not change any of 007-cli-delegate's shipped behavior (its
live per-tool-call approval stays the default); it is a second, explicit opt-in. Does not reintroduce
ACP (already rejected for Claude Code's Agent-SDK ToS clause). Desktop-only, same as 007. Full prior
design exploration, including empirical verification against installed Claude Code v2.1.274 and
Codex v0.147.0, is captured in `docs/plans/2026-09-17-autonomous-delegate-mode-design.md`."

## Amendment (2026-11-08, spec 083 PR J3)

The autonomy mode and the deny rules are no longer device- or vault-wide preferences
(`chat.autonomy_mode`, `cli_delegate.deny_rules`): they are the delegate rights group of the agent
profile of the model the turn runs with (`specs/083-agent-profiles`, FR-030). The pages
"Autonomie für Abo-Agenten" and "Verbotsregeln für Abo-Agenten" are gone; the group is in the editor
of a profile. Existing settings are moved once into "Standard" and into profiles assigned to existing
Abo-Agent models; profiles with an explicitly configured `delegate` group keep their value. A synced
vault marker makes the migration one-time across devices, including devices that retain a stale local
setting; a failed migration write leaves both settings and marker unchanged so the next open retries.
This preserves previous restrictions without overwriting profile choices. The mechanics of the modes
(FR-003 to FR-013) are unchanged.

## Amendment (2026-09-19)

Operator decision: the per-request autonomy picker in the chat composer added the user's second
permission-style menu next to Manual/Auto/Plan, which was confusing rather than useful. It has been
removed from the composer. Autonomy mode is now a single device-scoped default, chosen on the
Settings page (`settings.autonomyMode`, preference key `chat.autonomy_mode`), defaulting to
`ungated` — not `standard` — when unset. This directly supersedes **FR-002** ("no autonomy mode
selected → existing live approval behavior") and **FR-008**/User Story 4 ("autonomy never silently
carries over" — it now deliberately does, as an explicit, always-visible Settings default rather
than an easy-to-forget per-request toggle). The rest of this spec — the ungated/gated-permissive
mechanics themselves (FR-003 through FR-007, FR-009 through FR-013) — is unaffected; only _how the
mode is selected_ changed.

## Clarifications

### Session 2026-09-17

- Q: What determines whether an action needs extra scrutiny under the gated-but-permissive mode? →
  A: The delegate's own existing default risk classification (the same distinction 007-cli-delegate
  already uses today to decide what needs a human's approval) — unchanged. The operator's deny rules
  are an optional, additional filter layered on top of that existing classification, for the rare
  case the operator wants to lock something down regardless of what the delegate itself considers
  safe — not a replacement classification system holzi builds and maintains itself.
- Q: Where does a configured deny rule live, and how long does it apply? → A: Persisted as a
  device-wide setting, the same way the existing approval posture is — applied automatically to
  every gated-but-permissive run until the operator changes it. This is deliberately different from
  the autonomy mode selection itself (FR-008), which never persists: a deny rule only ever narrows
  what a run can do, so a forgotten one is at most overly cautious, not a surprise loss of
  supervision the way forgotten autonomy would be.
- Q: Should a rule be able to allow-but-flag an action (a "notify" kind) in addition to blocking it
  outright ("deny")? → A: No — deny only. The gated-but-permissive mode's full tool-call record
  (Story 2) already gives after-the-fact visibility into everything that ran; a separate live-notify
  channel would be a distinct, unrequested feature (asynchronous alerting) rather than something this
  spec needs, consistent with the earlier decision to defer the circuit-breaker until a concrete need
  arises rather than build speculative machinery.

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Let a trusted delegate work without pausing for every action (Priority: P1)

A user who trusts a connected delegate backend to work on a task wants it to use its own tools
(file edits, shell commands) without being interrupted for an approval decision on each individual
action, the way it already must be for the standard, gated behavior.

**Why this priority**: This is the core value of the feature — the standard live per-tool-call
approval is exactly what makes multi-step delegate work slow to supervise; removing that friction
for tasks the user is willing to trust is the whole point.

**Independent Test**: Select an autonomous mode for a delegate-backed request that needs several
tool calls to complete, send it, and confirm it finishes without any individual approval prompt.

**Acceptance Scenarios**:

1. **Given** the ungated autonomous mode is selected for a request, **When** the delegate needs to
   use any of its own tools, **Then** it proceeds without any approval prompt from holzi.
2. **Given** the gated-but-permissive autonomous mode is selected, **When** the delegate uses a tool
   that does not match a configured deny rule, **Then** it proceeds without pausing for a decision.
3. **Given** no autonomous mode is selected, **When** a delegate needs to use a tool, **Then** the
   existing live per-tool-call approval behavior applies exactly as it does today.

---

### User Story 2 - Review what an unsupervised run actually did (Priority: P1)

A user who let a delegate run autonomously wants to be able to check afterward what it did, rather
than only seeing a final answer with no record of the individual actions taken.

**Why this priority**: Ships alongside Story 1 for the same reason 007's own approval story
shipped with its first capability — autonomy without any way to review it afterward is a much
harder thing to trust than autonomy with a record.

**Independent Test**: Run a gated-but-permissive autonomous request that performs multiple tool
calls, then open that conversation afterward and confirm each action is visible.

**Acceptance Scenarios**:

1. **Given** a request ran under the gated-but-permissive mode, **When** the user reviews that
   conversation afterward, **Then** every tool call the delegate made during that run is present in
   the record, and it is clear which autonomy mode that turn ran under.
2. **Given** a request ran under the ungated mode instead, **When** the user reviews that
   conversation afterward, **Then** only the delegate's final answer is present — the absence of a
   per-action record is expected for this mode, not an error.

---

### User Story 3 - Keep specific actions off-limits even while otherwise autonomous (Priority: P2)

A user who wants broad autonomy for a task still wants certain categories of action represented by
the delegate's approval callbacks — the ones they consider genuinely sensitive — to stay blocked,
without having to give up autonomy for everything else to get that protection.

**Why this priority**: This is the "optional gateway" the feature exists to offer — valuable, but
the feature already delivers its core value (Story 1) without it, since a user who wants no
restriction at all can use the ungated mode instead.

**Independent Test**: Configure a rule blocking one category of action, run a gated-but-permissive
request that produces a vendor approval callback carrying the documented matching signal, and
confirm that callback is denied while non-matching approval callbacks proceed without a pause.

**Acceptance Scenarios**:

1. **Given** a deny rule is configured for a category of action, **When** a vendor approval callback
   processed by `approval_bridge::request_approval` carries a documented signal matching that
   category during a gated-but-permissive run, **Then** the requested action is not permitted, and
   the run continues rather than ending outright.
2. **Given** an approval callback reaches `approval_bridge::request_approval` and no configured rule
   matches its documented signals, **When** the bridge evaluates it, **Then** it proceeds without a
   pause. Delegate tool calls for which the vendor emits no approval callback are outside deny-rule
   evaluation and are not claimed as intercepted by this story.

---

### User Story 4 - Autonomy never silently carries over to the next request (Priority: P2)

A user who used an autonomous mode for one request expects the next delegate-backed request to
behave normally unless they deliberately choose autonomy again.

**Why this priority**: The failure mode this guards against — an operator forgetting autonomy was
left on and a later, unrelated request running unsupervised — is worse than the inconvenience of
re-selecting it each time.

**Independent Test**: Use an autonomous mode for one delegate-backed request, then send another
delegate-backed request without selecting a mode, and confirm the second one uses the standard
gated behavior.

**Acceptance Scenarios**:

1. **Given** an autonomous mode was used for one request, **When** the user sends a later
   delegate-backed request without re-selecting a mode, **Then** that request uses the standard,
   unchanged gated approval behavior.

---

### User Story 5 - Stop an autonomous response mid-run (Priority: P3)

A user who started an autonomous delegate response decides they want it to stop immediately, the
same way they already can for the standard gated behavior.

**Why this priority**: Consistency with existing stop behavior, but an autonomous response can be
waited out if this isn't ready yet, so it doesn't block the core capability from shipping.

**Independent Test**: Start an autonomous delegate-backed request doing something observable, stop
it, and confirm the underlying process actually terminates.

**Acceptance Scenarios**:

1. **Given** a running autonomous delegate response, **When** the user stops it, **Then** the
   underlying process is terminated and no further output from it is added to the conversation.

---

### Edge Cases

- What happens if no deny rule is configured for the gated-but-permissive mode? Everything is
  permitted by default — an empty rule set is fully permissive, not treated as an error.
- What happens if the mechanism evaluating a tool call under the gated-but-permissive mode itself
  fails mid-call? The pending action is treated as denied (fail-safe), consistent with how the
  standard gated mode already handles this.
- What happens if a user tries to select an autonomous mode without a connected delegate backend?
  The mode selector is only meaningful once a delegate backend is chosen, consistent with how
  backend-specific options already behave elsewhere.
- What happens if the delegate CLI's own native full-autonomy mechanism isn't available (e.g. an
  older installed version)? The system reports that autonomous mode is unavailable for this backend
  with a specific reason; it does not silently fall back to the standard gated mode or a different
  backend.
- What happens on mobile? Autonomous modes are unavailable there, the same as every delegate
  backend capability today — spawning a delegate process at all is not possible on mobile.
- What happens if a deny rule the operator enabled cannot actually be checked for a particular
  approval callback payload, because that backend does not expose enough detail about the action to
  evaluate it (found during planning: one delegate's file-change approvals do not carry a file path
  at all, so a workspace-boundary or credential-path rule cannot be verified against them
  specifically)? The callback action is treated as denied, the same fail-safe default as an
  evaluation failure — an unverifiable action never passes silently just because the deny rule that
  would have caught it couldn't be checked.

## Requirements _(mandatory)_

### Functional Requirements

- **FR-001**: Users MUST be able to select an explicit autonomy mode (standard, ungated, or
  gated-but-permissive) for a delegate-backed request, separate from choosing the backend itself.
- **FR-002**: When no autonomy mode is explicitly selected, the system MUST use the existing live
  per-tool-call approval behavior, unchanged.
- **FR-003**: In the ungated mode, delegate tool calls MUST proceed without any per-call approval
  request to the user.
- **FR-004**: In the gated-but-permissive mode, the delegate's own existing default risk
  classification MUST continue to govern which actions would normally need approval, unchanged from
  the standard mode. Any such action MUST additionally be evaluated against the operator's
  configured deny rules — auto-allowed unless a deny rule matches — instead of pausing for a human
  decision.
- **FR-005**: In the gated-but-permissive mode, the system MUST persist a record of every tool call
  the delegate makes during that run, and MUST make clear which autonomy mode a given turn ran
  under.
- **FR-006**: In the ungated mode, the system MUST NOT require a per-tool-call record — only the
  delegate's final response needs to be persisted.
- **FR-007**: An action represented by a vendor approval callback processed by
  `approval_bridge::request_approval` whose documented signals match a configured deny rule MUST NOT
  be permitted to run; the delegate MUST be informed the action was not available, and the run MUST
  continue rather than end outright. This requirement does not claim interception of tool calls for
  which the vendor emits no approval callback.
- **FR-008**: The system MUST NOT let a previously selected autonomy mode silently apply to a later,
  unrelated request — each delegate-backed request MUST start from the standard behavior unless
  autonomy is explicitly selected for that specific request.
- **FR-009**: Users MUST be able to stop an in-progress autonomous delegate response; stopping MUST
  terminate the underlying process rather than merely hide it from the interface.
- **FR-010**: If the mechanism evaluating a tool call under the gated-but-permissive mode fails or
  becomes unavailable mid-call, the pending action MUST be treated as denied rather than left
  pending indefinitely or defaulted to allowed.
- **FR-011**: Neither autonomy mode MUST weaken the existing guarantee that a delegate backend sees
  only holzi's own provided instance content and never host-level configuration belonging to that
  provider's own CLI.
- **FR-012**: Autonomous delegate modes MUST remain unavailable on mobile, consistent with delegate
  backends being desktop-only.
- **FR-013**: If a delegate's native full-autonomy mechanism is unavailable for the connected
  backend, the system MUST report this clearly rather than silently substituting a different mode or
  backend.
- **FR-014**: Configured deny rules MUST persist as rights of the `delegate` group in an agent
  profile and MUST apply automatically to every gated-but-permissive run using a model assigned to
  that profile until the operator changes them. They MUST NOT affect runs using another profile.

> **Since spec 083 PR J3** the autonomy mode and deny rules are profile rights, not device- or vault-wide values. Existing values are migrated into "Standard" and into profiles assigned to existing Abo-Agent models, unless those profiles already have a `delegate` group.

- **FR-015**: If an enabled deny rule cannot be evaluated against a particular approval callback
  because the backend does not expose enough detail in that callback, the system MUST treat it as
  denied rather than allow it through unverified.

### Key Entities

- **Autonomy Mode**: A profile setting for Abo-Agent model runs — standard (every call asks),
  ungated, or gated-but-permissive. Resolved from the profile assigned to the model.
- **Deny Rule**: An operator-configured, optional additional restriction that stays blocked under
  the gated-but-permissive mode even when the delegate's own default risk classification would
  otherwise have allowed it through. Persisted in the profile's `delegate` rights group and applies
  only to Abo-Agent model runs assigned to that profile.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: A user can complete a multi-step delegate-backed task under an autonomous mode without
  being interrupted for approval on any individual action along the way.
- **SC-002**: A user reviewing a gated-but-permissive run afterward can see every action the delegate
  took during that run.
- **SC-003**: A vendor approval callback processed by `approval_bridge::request_approval` whose
  documented signals match a configured deny rule is denied 100% of the time during a
  gated-but-permissive run, with no exceptions; no claim is made for tool calls that emit no approval
  callback.
- **SC-004**: A user who does not explicitly choose an autonomous mode always experiences the
  existing, unchanged per-action approval behavior — with no exceptions and no drift over time.
- **SC-005**: A user who stops an autonomous response sees the underlying process actually end,
  within the same short window stopping already takes for the standard mode.
- **SC-006**: When an autonomous mode cannot be used, the user always receives a specific,
  actionable explanation rather than a silent fallback or a generic error.

## Assumptions

- Builds directly on the delegate backend, delegate credential, and delegate invocation concepts
  already shipped in 007-cli-delegate; this feature adds two new ways to handle a delegate's tool
  calls during a request, not a new backend or credential model.
- Host isolation (a delegate sees only holzi's own provided content, never the host's own
  configuration for that provider's CLI), vault-portable credentials, and the mobile exclusion from
  007-cli-delegate all carry over unchanged and were directly re-verified under both autonomous
  modes rather than assumed to still hold.
- Does not use the Agent Client Protocol for either mode — the same third-party Agent-SDK
  restriction that ruled it out for 007-cli-delegate applies equally here.
- Verified 2026-09-17 against installed CLIs (Claude Code v2.1.274, Codex v0.147.0): both vendors
  provide a native mechanism suitable for the ungated mode. Codex additionally offers an
  independent, OS-level confinement setting that can stay active in the ungated mode without
  reintroducing per-call approval; no equivalent was found for Claude Code, so the
  gated-but-permissive mode is expected to be the more commonly recommended choice there
  specifically — both modes remain available for either backend, this is a usage recommendation,
  not a restriction.
- Verified 2026-09-17: a tool call made by a subagent the delegate spawns internally is evaluated
  the same way as a top-level call under the gated-but-permissive mode — confirmed by directly
  comparing an identical action performed at the top level versus inside a spawned subagent.
- No wall-clock, cost, or tool-call-count ceiling is included in this feature. Considered and
  explicitly deferred until a concrete problem motivates a specific mechanism, consistent with
  007-cli-delegate itself shipping without one.
- Full technical exploration and CLI verification underlying this specification is recorded in
  `docs/plans/2026-09-17-autonomous-delegate-mode-design.md`.
