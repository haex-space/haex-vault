# Feature Specification: Unified, Cached Model Capabilities

**Feature Branch**: `012-unified-model-capabilities`

**Created**: 2026-09-20

**Status**: Draft

**Input**: User description: "I want to implement our plan with spec-kit: replace the several
independent, hand-maintained mechanisms that decide which reasoning-effort levels, adaptive-reasoning
behavior, and attachment types a model supports with one unified, provider-agnostic, cached model
capability record — fetched live from providers where they expose it (Anthropic API key and Claude
Code delegate today, Codex and Gemini later), determined locally for built-in models, persisted so it
is never recomputed from static tables, with the user's selected reasoning effort remembered
separately per model and per device."

## Clarifications

### Session 2026-09-20

- Q: How should the effort control look when there is nothing to choose (no reasoning, model-managed
  reasoning, or not yet known)? → A: Hidden when the model has no reasoning control; a disabled
  control with a state label when reasoning is model-managed or not yet known, the latter pointing
  to the provider refresh in Settings.
- Q: Should the user be told when a saved effort option is dropped after a provider refresh and the
  model falls back to Auto? → A: No — silent fallback; the control simply shows Auto.

## User Scenarios & Testing _(mandatory)_

### User Story 1 - The composer offers exactly what the selected model supports (Priority: P1)

Today holzi decides what to offer in the composer from several separate, hand-curated rules: a
table of effort levels per Claude model that goes stale whenever a new model ships, a separate rule
for whether a model reasons at all, another for how a request should be shaped, and another for which
attachments a provider accepts. The Claude Code delegate always advertises every effort level no
matter which model is selected. A user picking a model should instead see effort options and
attachment behavior that match what that specific model really allows, learned from the provider
itself and kept current whenever the model list is refreshed.

**Why this priority**: This is the core of the request and the source of every visible inaccuracy —
options that do not exist for the chosen model, missing options for a newly released model, and
attachment decisions made from guesses. Everything else (remembering choices, honesty about gaps)
builds on the composer being driven by real per-model data.

**Independent Test**: Connect a provider that reports per-model capabilities, select two models that
support different effort levels and different attachment types, and confirm the composer's effort
options and attachment acceptance change to match each model's reported capabilities. Refresh the
provider's model list and confirm a changed capability shows up without updating the app.

**Acceptance Scenarios**:

1. **Given** a directly connected Claude model that reports a provider-native set of reasoning
   options, **When** the user opens the effort control, **Then** only those options (plus Auto) are
   offered, in the order reported by the provider.
2. **Given** the Claude Code delegate is connected and the selected model reports a subset of the
   provider's reasoning options, **When** the user opens the effort control, **Then** only the
   reported options are offered, not a fixed global set.
3. **Given** a model the provider reports as having no controllable reasoning, **When** the user
   opens the composer, **Then** the effort control is hidden for that model.
4. **Given** a model the provider reports as managing its own reasoning with no user-selectable
   level, **When** the user opens the composer, **Then** the effort control is shown disabled with a
   label saying the model manages this itself, and no levels are offered.
5. **Given** a model the provider reports as accepting images, **When** the user attaches an image,
   **Then** it is accepted; **Given** a model the provider reports as not accepting images,
   **Then** it is declined with a reason.
6. **Given** a provider releases a new model or changes a model's supported levels, **When** the user
   refreshes that provider's model list, **Then** the composer reflects the new capabilities without
   any holzi update.

---

### User Story 2 - Each model remembers its own effort choice (Priority: P2)

A user who prefers one provider-native reasoning option on one model and the provider default on
another should not have to re-select each time they switch. The effort choice is a personal,
per-model preference: it is
remembered on this device for each model separately, restored when that model is selected again, and
never applied to a model that does not offer it.

**Why this priority**: Depends on Story 1's per-model option lists to know what is valid, but
delivers independent day-to-day value — no more re-picking effort after every model switch or
restart — and fixes the current behavior where one effort selection bleeds across models.

**Independent Test**: Choose different effort options for two models, switch back and forth and
restart the app, and confirm each model always comes back with its own choice; then remove an option
from what a model supports and confirm the model falls back to Auto and forgets the stale choice.

**Acceptance Scenarios**:

1. **Given** the user chose one provider-native option for model A and a different option for model B, **When** they
   switch between A and B (and restart the app), **Then** each model shows its own saved choice.
2. **Given** the user has never made a choice for a model, **When** they send a message, **Then** the
   provider's own default behavior (Auto) is used and nothing is stored for that model.
3. **Given** the user reselects Auto for a model with a saved choice, **When** they later return to
   it, **Then** it is on Auto.
4. **Given** a saved choice that the model's refreshed capabilities no longer offer, **When** the
   capabilities are refreshed or the model is next selected, **Then** the model falls back to Auto,
   the stale saved choice is cleared, the control simply shows Auto (no separate notice), and the
   unsupported option is never sent.
5. **Given** the user switches models quickly, **When** the saved choice for the previous model
   finishes loading after the switch, **Then** it does not overwrite the newly selected model's value.
6. **Given** saving a new choice fails, **When** the user picks an option, **Then** the control
   reverts to the previous effective value and the failure is shown.

---

### User Story 3 - The composer is honest about what is not yet known (Priority: P3)

Some models have no capability information yet: a Codex delegate model (Codex does not currently
expose it), a model from a provider connected before this feature shipped and not yet refreshed, or a
provider whose response omitted a field. Holzi must never present "we have not found out yet" as "the
model cannot do this". Built-in local models, which have no provider to ask, keep their existing
known behavior, and everything already learned stays available without re-fetching.

**Why this priority**: A correctness and trust refinement on top of Stories 1–2. Without it the
system would either fabricate capabilities or wrongly tell users their model is incapable.

**Independent Test**: Select a Codex model and a not-yet-refreshed Claude model and confirm the
composer treats their capabilities as undetermined (not as unsupported); refresh the Claude provider
and confirm real options appear. Select a local model and confirm its reasoning behavior is
unchanged and attachments are reported as unsupported.

**Acceptance Scenarios**:

1. **Given** a model whose capabilities have not been determined, **When** the user opens the
   composer, **Then** no effort options are invented and the effort control is shown disabled with a
   label saying this is not yet known and pointing to the provider refresh in Settings, never
   describing the model as unsupported.
2. **Given** an undetermined model, **When** the user tries to attach a file, **Then** it is not
   accepted and the reason states that support is not yet known rather than that the model cannot
   take attachments.
3. **Given** a provider configured before this feature shipped, **When** the user first opens the
   composer, **Then** its models are undetermined until the user refreshes the provider in Settings,
   after which real capabilities appear.
4. **Given** a built-in local model, **When** it is selected, **Then** its previously existing
   reasoning behavior is unchanged and attachments are reported as authoritatively unsupported.
5. **Given** capabilities were previously learned for a model, **When** the app is restarted or the
   network is unavailable, **Then** the last-known capabilities are still used.

---

### Edge Cases

- A provider's capability data is missing, partial, or contains fields holzi does not recognize:
  affected capabilities become undetermined; the model list refresh still succeeds.
- A model-list refresh fails partway: previously stored capabilities are kept rather than cleared.
- A refresh returns a model whose capabilities differ from what is stored: the new answer replaces
  the old one entirely (no stale merging).
- Providers use different vocabularies and orderings for effort (some have "max", some "minimal",
  some numeric budgets): the composer shows each provider's own options and never forces them onto a
  single shared scale.
- The selected model changes while a send is being prepared: the effort used must be the one valid for
  the model actually being sent to.
- A saved effort belongs to a model that is later removed or renamed: it is simply never restored; it
  is not applied to any other model.
- A stored capability record cannot be read (corrupt data): the model is treated as undetermined and
  the problem is reported with context, rather than crashing the model list.
- The same account is used on several devices: capability facts follow the model everywhere, while
  each device keeps its own effort preferences.
- The model lists are still loading or no model is selected: the effort control is hidden, not shown as "not yet known".

## Requirements _(mandatory)_

### Functional Requirements

- **FR-001**: The system MUST keep, for every model, one capability record stating what that specific
  provider/model pair supports (reasoning control and accepted attachment types), and every part of
  the product that decides what to offer, accept, or send MUST consult that record instead of
  independent rules.
- **FR-002**: The capability record MUST distinguish "not yet determined" from "determined to be
  unsupported". Missing, partial, or unrecognized provider data MUST be treated as not yet
  determined, never as unsupported and never as an error that blocks the model list.
- **FR-003**: For Claude models reached through an API key and through the Claude Code delegate, the
  record MUST be populated from the provider's own per-model capability information each time the
  model list is refreshed.
- **FR-004**: A model's reasoning control MUST be exactly one of: not determined, unavailable,
  model-managed (the model decides; the user cannot choose), or a list of selectable options, each
  with an opaque, stable provider-native identifier and a display label in the provider's own
  vocabulary. The system MUST preserve the provider's option order and MUST NOT require a shared
  enum or canonical set of level names. A list with no options MUST be treated as unavailable.
- **FR-005**: The composer MUST offer only the reasoning options present in the selected model's
  record (plus Auto, meaning "leave it to the model's default"), and MUST NOT present an active
  control whose choice would have no effect. The control MUST be hidden when reasoning is
  unavailable, and shown disabled with a state label when reasoning is model-managed or not yet
  determined; the not-yet-determined label MUST point to the provider refresh in Settings. The control MUST also be hidden while no model is resolved (nothing selected, or the model lists are still loading), so "not yet known" is never shown for a model that has simply not been looked up yet.
- **FR-006**: Built-in local models MUST have their reasoning control determined locally when they are
  registered (by download or import), since no provider exists to ask; their accepted attachment types
  are authoritatively none until local multimodal support exists.
- **FR-007**: The Codex delegate's models MUST be recorded explicitly as not determined, until Codex
  exposes capability information, and MUST NOT be presented as unsupported.
- **FR-008**: File attachments MUST be accepted only when their type is in the selected model's
  accepted attachment types; for a model whose attachment support is not determined, the attachment
  MUST NOT be accepted and the stated reason MUST say support is not yet known.
- **FR-009**: Capability records MUST be stored so they survive restarts and are available whenever
  the model list is shown, without extra lookups when the user switches models or opens the composer.
- **FR-010**: Refreshing a provider's models MUST replace each model's stored capabilities with the
  newest answer; a failed refresh MUST leave the previously stored capabilities intact.
- **FR-011**: Capability records MUST travel with the model record to the user's other devices in the
  same way other model facts do.
- **FR-012**: Models that already exist when this feature ships MUST start as not determined and MUST
  become determined through the per-provider refresh action of FR-022; the app MUST NOT add automatic
  network refreshes at launch.
- **FR-013**: The user's selected reasoning option MUST be validated against the selected model's
  current options immediately before a message is sent; a selection that is not currently offered MUST
  NOT be sent, and the model's own default behavior applies instead.
- **FR-014**: When a valid option is selected, the request to the provider MUST use that option in the
  provider's own request form; when Auto is in effect, the request MUST leave the choice to the model's
  own default.
- **FR-015**: The selected reasoning option MUST be remembered separately per model on the current
  device — where a model is one model as reached through one provider connection, so the same model
  reached through two connections remembers its choice separately — MUST be absent when the user is
  on Auto (Auto stores nothing), and MUST NOT be stored as a model capability or shared between
  models.
- **FR-016**: When the active model changes, the system MUST restore that model's saved option if it
  is still offered, otherwise use Auto; a slow restore for a previously selected model MUST NOT
  overwrite the value for the currently selected model.
- **FR-017**: When a capability refresh removes the option a model had selected or saved, the system
  MUST fall back to Auto and clear the stale saved option, silently — the control shows Auto and no
  separate notice is raised.
- **FR-018**: Changing the selected option MUST take effect immediately; if remembering it fails, the
  selection MUST revert to the previous effective value and the failure MUST be shown.
- **FR-019**: The previously separate mechanisms — the curated per-model effort table, the always-full
  effort set for the Claude Code delegate, the model-name-based reasoning and adaptive-reasoning
  rules, and the provider-based attachment rule — MUST be removed rather than kept as fallbacks, so an
  out-of-date table can never reappear as an answer. The single remaining local rule (FR-006) is
  scoped to local models only.
- **FR-020**: The composer and the send flow MUST NOT contain provider-specific rules about
  capabilities; supporting an additional provider MUST require only supplying that provider's
  capability record.
- **FR-021**: A stored capability record that cannot be read MUST be treated as not determined and
  reported with enough context to diagnose it, without preventing the model list from loading.
- **FR-022**: Settings MUST offer, for every connected provider, a lightweight action that re-fetches
  that provider's models and capabilities on demand (no new sign-in). While it runs it MUST show
  progress; on success the composer reflects the new capabilities the next time it is shown; on
  failure the previously stored capabilities stay intact and the failure is shown.
- **FR-023**: Whether a model's reasoning is requested and shown while it answers MUST follow the
  same capability record (reasoning control available or model-managed), not a separate rule; a model
  whose reasoning control is not yet determined neither requests nor shows reasoning until determined.
- **FR-024**: For a model of an OpenAI-compatible server (spec 055 R7), whatever the server's model
  list reports MUST be read into the capability record instead of being left "not determined":
  accepted input (images), tool support, whether the model reasons, and what it produces (text,
  image, audio, other; the model's own type wins over its output modalities). A field that is absent,
  empty or of an unexpected type states nothing and never fails the list. Effort levels are never
  derived this way; the user states them (spec 055 FR-019). A model that does not produce text MUST
  NOT be offered in the chat's model list, and MUST stay listed, by kind, on its server's settings
  page. A statement of the user (spec 055 FR-019) replaces a reported value while it stands;
  withdrawing it MUST bring the reported value back. The settings page shows a reported value as it
  is and asks for a choice only where the server is silent.

### Key Entities

- **Model Capabilities**: What one provider/model pair supports. Holds the reasoning control and the
  accepted attachment types; each part can independently be "not determined". Owned by the model
  record, refreshed from the provider, shared with the user's other devices.
- **Reasoning Control**: The model's answer to "can the user influence reasoning?" — not determined,
  unavailable, model-managed, or a set of selectable options. Distinct from the user-side state
  "Auto", which only means no option is selected.
- **Reasoning Option**: One selectable choice: a stable identifier plus a label in the provider's own
  vocabulary. Valid only while it appears in the selected model's current options.
- **Effort Preference**: The user's remembered option for one model (per provider connection) on one
  device. Absent means Auto.
  Separate from capabilities: refreshing provider facts never overwrites a user's choice, other than
  clearing a choice that is no longer offered.
- **Model**: Existing entity for a provider/model pair; gains the capability record alongside facts it
  already carries, such as its context size.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: For every supported provider/model combination covered by verification, the provider-native
  reasoning options and attachment acceptance shown in the composer match the provider's reported
  capabilities for that model in 100% of cases. The verification matrix is: Claude models with
  different provider-reported option sets through an API key and the Claude Code delegate, a
  built-in reasoning model, a built-in non-reasoning model, and a Codex model.
- **SC-002**: After one manual refresh, a newly released or changed Claude model shows correct options
  with no application update.
- **SC-003**: In verification, a model switch never applies one model's effort choice to another, and
  every model revisited (including after restart) restores its own choice in 100% of cases.
- **SC-004**: Zero verified cases where the composer presents a control whose choice is silently
  ignored, or sends an effort option the selected model does not currently offer.
- **SC-005**: Switching models or opening the composer causes no additional capability requests to any
  provider; capability display is available as fast as the model list itself.
- **SC-006**: Models with undetermined capabilities are never described as unsupported in the
  composer, in 100% of verified cases (Codex model, not-yet-refreshed provider, incomplete provider
  data).
- **SC-007**: Adding capability support for a new provider requires no changes to composer or send
  logic, demonstrated by the Codex "not determined" case flowing through the same path as Claude.

## Assumptions

- The Anthropic model listing already reports per-model capabilities (effort levels, thinking style,
  image input); this feature depends on that and treats missing fields as not determined. This is confirmed for API-key requests; that the same capability tree is returned for the OAuth-authenticated requests the Claude Code delegate uses must be verified before merge (quickstart §3).
- Codex does not yet expose capability metadata, and Gemini has no adapter yet; both are outside this
  feature's scope. The design only needs to make adding them a provider-side change.
- Local multimodal support does not exist; local models' attachment types are recorded as none.
- Existing providers showing no effort control, no reasoning output, and no attachment support until
  refreshed is an accepted trade-off that avoids new network calls at launch. Today the only way to
  re-fetch a provider's models is the full "Reconnect" sign-in flow — no lightweight refresh action
  exists in the UI — so this feature adds one (FR-022); without it Story 1 scenario 6 and SC-002
  could not be met.
- Disclosed user-visible change: built-in local models that reason (e.g. Qwen3-family) previously
  showed no effort control and now show the disabled "managed by the model" state (FR-005), because
  their reasoning is model-managed rather than absent.
- Effort preferences follow the project's existing device-scoped data convention (ADR 0001): they
  belong to this device and are not shared across devices, while capability facts follow the model.
  **Since spec 023** (FR-024) effort preferences apply to the whole vault; a device's earlier
  value becomes the vault value on open when the vault has none, see [`023-settings-app`](../023-settings-app/spec.md).
- Sending to the Claude Code delegate keeps its current behavior of handing the chosen provider-native
  option id to the delegate as-is; only what the composer offers changes. This is a disclosed
  user-visible change: the delegate's model may now show fewer options than the previous always-full
  set.
- Attachment failure wording and ordering in the composer is otherwise unchanged; only the
  undetermined case gains its own reason.
- Splitting the oversized model-selection code into focused parts is a required, behavior-preserving
  prerequisite of this work, invisible to users.
- Out of scope: live capability data for Codex, a Gemini adapter, local multimodal attachments,
  refreshing all providers on launch, and any change to how the delegate is invoked at send time.
- Terminology: "not yet known" (user-facing wording) and "not determined" (requirement wording) name the same state — no capability data has been obtained for that field. It is distinct from "unavailable" or "unsupported", which are determined negative answers.
