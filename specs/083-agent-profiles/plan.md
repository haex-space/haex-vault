# Implementation Plan: Agent profiles

**Branch**: `083-agent-profiles-plan` | **Date**: 2026-11-05 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/083-agent-profiles/spec.md`

## Summary

One agent profile is one set of rights over every kind of access, kept per vault. Today the rights of an external
agent or an Abo-Agent are rows per agent (`haex_mcp_agent_grants`, loaded once per connection), the local agent has
a rights model for the browser and the password fills only (device preferences), and nothing limits its other tools
except the approval mode. The plan:

1. Three new synced, vault-wide tables (profiles, the rights rows of a profile, the assignment of an agent to a
   profile). The rights rows have the shape and the code (`grant_rows`) of today's agent rows: one implementation.
   "Standard" is a fixed id whose rows may be absent; absent means the factory rights, so nothing has to be seeded.
2. One resolver, `agent_profiles::rights_of(subject)`, replaces `registry::load_rights` for external agents and
   Abo-Agents and feeds the local agent, whose subject is the model of its active chat session. `grants.rs` keeps
   deciding every tool list and call; only where `Rights` come from changes.
3. A membership row has a deterministic id derived from the subject and the profile (PR H; PR A to C derived it from the
   subject alone), so two devices that add the same membership write the same row and different memberships never
   conflict. Exactly one profile is active for the vault, one preference value; the resolver gives an agent the active
   profile when it is a member of it, else "Standard".
4. The per-connection rights cache of an MCP session is refreshed on every profile or assignment change, local or
   arriving by sync, from the existing table feed (`core.vault_changes`), and what a prompt remembers is written into
   the profile.
5. The local agent gets enforcement for the whole rights list (new). There is no migration (spec 083 FR-024): the old
   per-agent rights and the local agent's old settings stop counting, every agent and every model starts in "Standard",
   and the user sets profiles anew. A profile belongs to many agents; nothing creates one per agent.
6. UI: KI → Agentenprofile (list and editor reusing the rights list components of #817 to #827), one field
   "Agentenprofil" on the agent page and in the registration question, the local agent's rights leave
   KI → Browser, and "Abo-Agent" replaces "Delegate" in the user-facing texts.

Stage 1 only (spec Scope): files, contacts and cloud storage keep their per-agent grants.

## Technical Context

**Language/Version**: Rust (edition of `src-tauri`), TypeScript / Vue 3 / Nuxt, as the rest of the repository.

**Primary Dependencies**: none new. Existing: `grant_rows`/`grants` (rights shape and checks), the CRDT write path and
`haex_deleted_rows` markers, `vault_events` (`core.vault_changes`), the task-local `AgentTask`, `ts-rs` bindings,
holzi's own settings components in `src/components/settings/` (`SettingsGroup`, `SettingsRow`, `SettingsOptionRow`,
`SettingsSelect`, `SettingsMultiSelect`) and `AgentsRightsInset`.

**Storage**: three new CRDT tables in migration `0054_agent_profiles` (next free number after
`0053_sync_rules_source_target`), `HOLZI_TRIGGER_VERSION` 34 to 35. Existing tables `haex_mcp_agents`,
`haex_mcp_agent_grants` stay; the grant rows stop being read (R8).

**Testing**: `cargo test --lib -- agent_profiles mcp_server agent_browser grant_rows`, migration tests next to
`identity/migrations_mcp_agents_tests.rs`, `src-tauri/tests/mcp_server_*` integration tests, `scripts/check-mcp-actions.ts`,
`scripts/check-settings.ts`, the e2e scenario `mcp-agent.test.ts` plus a new `agent-profiles.test.ts`.

**Target Platform**: desktop (Linux, macOS, Windows) and Android; the rights logic is platform independent, the UI is
the shared settings UI.

**Project Type**: desktop and mobile app (Tauri), Rust backend with a Nuxt frontend.

**Performance Goals**: resolving the rights of an agent is two small indexed reads; the browser, password and file
executors already read rights on every call, so no cache is added there. The MCP session keeps its cache and is
refreshed by push (R5).

**Constraints**: a change reaches the agents' next call on every device (FR-011), so the refresh hangs off the table
feed, which also carries synced commits (R5); a rights row this build cannot read means no right (FR-012; PR A makes
`grant_rows::rights_of` strict for `commands` and `operate`, R9); the old rights are not converted (FR-024, R7), so a start after the update must not reset a profile the user made (FR-025).

**Scale/Scope**: a handful of profiles, a handful of agents; Stage 1 touches about 25 Rust and 20 frontend files
(research.md lists them).

## Constitution Check

_Checked against `.specify/memory/constitution.md` 1.4.0 and `.spaex/constitution.md`._

| Principle / rule                                                                                                      | Result                                                                                                                                                                                          |
| --------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| I No secrets in git                                                                                                   | Pass: profiles hold rights, never credentials; the agent credential stays in `haex_mcp_agents`.                                                                                                 |
| II No local absolute paths                                                                                            | Pass: nothing committed names a machine path.                                                                                                                                                   |
| III Project identity device independent                                                                               | Pass: profiles are vault data; assignments name an agent id or a model id, not a path.                                                                                                          |
| IV, V Pinned references, opt-in sources                                                                               | Pass: no external content is referenced.                                                                                                                                                        |
| VI Self-modifying instructions review-gated                                                                           | Pass: no skill, snippet or permission file of an agent is edited; the plan lands through pull requests.                                                                                         |
| VII Relay unavailability never blocks local work                                                                      | Pass: resolving rights is a local read; profiles reach other devices by the existing sync.                                                                                                      |
| VIII No concealment                                                                                                   | Pass.                                                                                                                                                                                           |
| Conventional Commits, rebase or merge commit, no squash                                                               | Followed by the delivery below.                                                                                                                                                                 |
| One implementation per rule (spaex/memory)                                                                            | Pass: the profile rights rows reuse `grant_rows` and `grants`; the old per-agent rows and actions are removed in the last PR, not kept next to the new ones.                                    |
| Guardrails locked for agents; no agent widens its own rights                                                          | Pass: every profile and assignment action is `scope: 'guardrails'`, `agentCallable: false`; the remember prompts write only what the user answered; extensions get no bridge call for profiles. |
| Every device and OS                                                                                                   | Pass: no OS specific logic; Android uses the same settings UI and the same Rust.                                                                                                                |
| UI: save on selection, errors as toast, no success notice, no "Layout" jargon, no bare "Relay", user name "HaexVault" | Followed (R11).                                                                                                                                                                                 |
| Specs in English, chat in German                                                                                      | Followed.                                                                                                                                                                                       |

No violation, so no entry in Complexity Tracking. One thing is not a violation but needs the operator's eye:
agents are device scoped (`haex_mcp_agents.vault_device_uuid`, ADR-0001) while profiles are vault wide; the
assignment therefore names an agent row of one device (R2), and the rows of every device are listed and counted. No
ADR is needed, the choice is recorded in research.md. The open questions below are not violations either; they are
spec-level choices the operator makes before the named PR lands.

**Re-check after design (Phase 1)**: unchanged, all rows still pass.

## Delivery (pull requests, in order)

Each PR leaves the app working. There is no migration of old rights: from PR B on every agent is in "Standard".

| PR  | Content                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    | Visible to the user                                                           |
| --- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------- |
| A   | Migration 0054, the three tables, the `agent_profiles` store and resolver, the strict `grant_rows::rights_of` (R9), the removal of an agent's assignment on revoke, the refresh of live MCP sessions from the table feed. Additive: nothing reads profiles yet                                                                                                                                                                                                                             | nothing changes                                                               |
| B   | The switch for external agents and Abo-Agents: `load_rights` resolves through profiles (every agent in "Standard"), commands and catalog actions, KI → Agentenprofile (list, editor with the widening confirmation, members), the field "Agentenprofil" on the agent page and in the registration question (also OAuth consent and Abo-Agent registration), remembered rights written into the profile and the prompts naming it, the old per-agent rights inputs removed                  | the new pages; agents start in "Standard"                                     |
| C   | The local agent: subject from the turn's `AgentTask`, the profile checked in the offer of each step and on each call, its remembered rights and the "switch the browser on" prompt written into its profile, its old settings deleted, the right "alle Erweiterungen" (Q3), the chat notice, KI → Browser without its rights, its models in the agent list and in the members of a profile                                                                                                 | the local agent's rights are profile rights                                   |
| G   | Fixes after the first manual check: the "+" button and the plainer texts on the profile list, every local model listed, the profile page keeps its draft while a save is queued                                                                                                                                                                                                                                                                                                            |
| H   | Several profiles per agent, one active (2026-11-06): member ids from subject and profile (migration 0057), the preference `agent_profile.active`, the resolver, `agent_membership_set` and `agent_profile_active_set` in place of `agent_assign`, the choice of the active profile in the list of profiles and on a profile's page, the agent page and registration question with several profiles, the member list of a profile, the live refresh from the preference, quickstart and e2e |
| I   | Models of the active profile in the chat (2026-11-07): `model_available`, a model can leave "Standard", the turn is refused otherwise, the select "Agentenprofil" in the composer, and no confirmation for any change of rights                                                                                                                                                                                                                                                            | the chat offers fewer models while a profile is active                        |
| J   | One kind for Abo-Agents and local models (2026-11-07, R14): J1 spec, J2 the pass names the model and the session carries its subject (rights by subject, no registration, Abo-Agent models listed and offered by profile), J3 autonomy and deny rules as rights of a profile (the group `delegate`, the old settings pages removed), J4 dead code and e2e                                                                                                                                  | an Abo-Agent's tools follow the profile of its model; "Rechte festlegen" goes |
| K   | Contacts and files in the profile (2026-11-08, FR-032, FR-033): K1 the group `contacts` (level for all contacts and per group, resolved for the model or agent, "Immer erlauben" written into the profile that was asked), K2 the group `files` (device, storages, encrypted folders, file shares, sync rules) and the drop of `agent_file_permissions`                                                                                                                                    | two profiles can give different contact and file access to different models   |
| M   | Providers as members (2026-11-08, FR-034): a provider is a subject (`provider:<id>`), its memberships add to those of each of its models (also future ones), a provider can leave "Standard", the pages group models by provider; M2 (FR-035) renames "Externe Agenten" to "Agenten und Modelle" and moves the MCP switch and snippets to "Externer Zugang"                                                                                                                                | rights per provider: a work LiteLLM and a private OpenRouter differ           |
| E   | Drop of the table `haex_mcp_agent_grants` (migration 0056), naming sweep "Abo-Agent", docs, quickstart, e2e                                                                                                                                                                                                                                                                                                                                                                                | nothing new                                                                   |

A is safe on its own. B and C each change what agents may do (they drop to "Standard"), so each comes with a test of that.
B holds the switch and the pages together because the old rights inputs have nowhere to write once `load_rights` reads
profiles.

## Decisions on the open questions (operator, 2026-11-05)

research.md R13 has the options and the reasoning.

- **Q1 and Q2: void.** They were about migrating the old per-device settings of the local agent and an edit of "Standard"
  that has not synced. There is no migration (spec 083, 2026-11-06), so neither case exists.
- **Q3** (PR C) Extensions installed later: **a new right "alle Erweiterungen"** (a row `extension` /
  `*`, skipped by older builds), so a profile can give a kind of agent every extension, also those installed later.
- **Q4** (PR A) Profile names: **no uniqueness**. A profile is its unique id; the same name may appear several times if
  the user wants that (FR-002 amended). No name check and no duplicate marking.

## Project Structure

### Documentation (this feature)

```text
specs/083-agent-profiles/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   └── commands-and-actions.md
└── tasks.md             # /speckit-tasks, not created here
```

### Source Code (repository root)

```text
src-tauri/src/
├── agent_profiles/                  # new
│   ├── mod.rs                       # Subject, STANDARD_PROFILE_ID, public API
│   ├── store.rs                     # the three tables: read, write, delete with markers
│   ├── resolve.rs                   # rights_of(subject): member row, profile, grant rows, defaults
│   ├── subjects.rs                  # the agents of this device and the models of the local agent
│   ├── commands.rs                  # Tauri commands (contracts/)
│   └── *_tests.rs
├── identity/
│   ├── migrations.rs                # register 0054, bump HOLZI_TRIGGER_VERSION to 35
│   ├── migrations_agent_profiles.rs # new: the three tables
│   └── migrations_legacy_names.rs   # only if another branch took 0054
├── mcp_server/
│   ├── registry.rs                  # load_rights/save_rights replaced by the resolver and an assignment write
│   ├── grant_rows.rs                # unchanged shape, reused for profile rows; rights_of strict (R9)
│   ├── live.rs                      # re-resolve every live session, under synchronize(agent)
│   ├── attach.rs                    # tell_catalog_changes also follows the three profile tables
│   ├── manage.rs, manage_commands.rs, connection.rs, delegates.rs, oauth_flow.rs,
│   │   oauth_rows.rs, password_env.rs, registration.rs, prompts.rs   # callers switched
├── agent_browser/
│   ├── env.rs, env_rights.rs        # built-in rights from the profile; remember writes the profile
│   └── commands.rs                  # builtin setters removed (PR E)
└── chat/
    ├── tools/agent_task.rs          # AgentTask carries the model id of the turn (the subject)
    ├── tools/offer.rs, availability.rs   # the offer of each step leaves out what the profile does not cover
    └── turn/                        # each call re-checked against the profile (PR C)

src/
├── components/settings/agents/      # AgentsView (kind labels), AgentDetailView (field + read-only rights)
├── components/settings/agentProfiles/   # new: ProfilesView.vue, ProfileView.vue, ProfileMembers.vue, Memberships.vue
├── components/settings/agentBrowser/AgentBrowserView.vue   # rights section removed
├── components/agents/               # RegistrationDialog.vue (profile field), the rights components reused
├── lib/agents/rights.ts, lib/agents/profiles.ts (new: labels, error codes)
├── lib/actions/settingsMcpAgentActions.ts, settingsAgentBrowserActions.ts, new settingsAgentProfileActions.ts
├── lib/settings/registry.ts, components/wm/appRoutes.ts
├── stores/mcpAgents.ts, stores/settingsActionHandlers.ts, stores/agentProfiles.ts (new)
└── i18n/locales/de.json, en.json

scripts/
├── check-mcp-actions.ts, check-settings.ts, check-agent-profiles.ts (new)
└── e2e/scenarios/mcp-agent.test.ts, agent-profiles.test.ts (new)
```

**Structure Decision**: a new top-level Rust module `agent_profiles`, because the local agent, the MCP server and
the agent browser all resolve rights through it and none of them should own it. The rights shape stays in
`mcp_server/grants.rs`.

## Complexity Tracking

No constitution violation to justify.
