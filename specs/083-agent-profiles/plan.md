# Implementation Plan: Agent profiles

**Branch**: `083-agent-profiles-plan` | **Date**: 2026-11-05 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/083-agent-profiles/spec.md`

## Summary

One agent profile is one set of rights over every kind of access, kept per vault. Today the rights of an external
agent or an Abo-Agent are rows per agent (`haex_mcp_agent_grants`, loaded once per connection), the built-in agent has
a rights model for the browser and the password fills only (device preferences), and nothing limits its other tools
except the approval mode. The plan:

1. Three new synced, vault-wide tables (profiles, the rights rows of a profile, the assignment of an agent to a
   profile). The rights rows have the shape and the code (`grant_rows`) of today's agent rows: one implementation.
   "Standard" is a fixed id whose rows may be absent; absent means the factory rights, so nothing has to be seeded.
2. One resolver, `agent_profiles::rights_of(subject)`, replaces `registry::load_rights` for external agents and
   Abo-Agents and feeds the built-in agent, whose subject is the model of its active chat session. `grants.rs` keeps
   deciding every tool list and call; only where `Rights` come from changes.
3. The assignment row has a deterministic id derived from the subject, so "at most one profile" holds by primary key,
   also across two devices.
4. The per-connection rights cache of an MCP session is refreshed on every profile or assignment change, and what a
   prompt remembers is written into the profile.
5. The built-in agent gets enforcement for the whole rights list (new), with a migrated full-rights profile so no
   existing model loses anything.
6. UI: KI → Agentenprofile (list and editor reusing the rights list components of #817 to #827), one field
   "Agentenprofil" on the agent page and in the registration question, the built-in agent's rights leave
   KI → Browser, and "Abo-Agent" replaces "Delegate" in the user-facing texts.

Stage 1 only (spec Scope): files, contacts and cloud storage keep their per-agent grants.

## Technical Context

**Language/Version**: Rust (edition of `src-tauri`), TypeScript / Vue 3 / Nuxt, as the rest of the repository.

**Primary Dependencies**: none new. Existing: `grant_rows`/`grants` (rights shape and checks), the CRDT write path and
`haex_deleted_rows` markers, `ts-rs` bindings, haex-ui settings components (`SettingsGroup`, `SettingsRow`,
`SettingsSelect`, `SettingsMultiSelect`, `AgentsRightsInset`).

**Storage**: three new CRDT tables in migration `0054_agent_profiles` (next free number after
`0053_sync_rules_source_target`), `HOLZI_TRIGGER_VERSION` 34 to 35. Existing tables `haex_mcp_agents`,
`haex_mcp_agent_grants` stay; the grant rows stop being read (R8).

**Testing**: `cargo test --lib agent_profiles mcp_server agent_browser`, migration tests next to
`identity/migrations_mcp_agents_tests.rs`, `src-tauri/tests/mcp_server_*` integration tests, `scripts/check-mcp-actions.ts`,
`scripts/check-settings.ts`, the e2e scenario `mcp-agent.test.ts` plus a new `agent-profiles.test.ts`.

**Target Platform**: desktop (Linux, macOS, Windows) and Android; the rights logic is platform independent, the UI is
the shared settings UI.

**Project Type**: desktop and mobile app (Tauri), Rust backend with a Nuxt frontend.

**Performance Goals**: resolving the rights of an agent is two small indexed reads; the browser, password and file
executors already read rights on every call, so no cache is added there. The MCP session keeps its cache and is
refreshed by push (R5).

**Constraints**: a change reaches the agents' next call (FR-011); a rights row this build cannot read means no right
(FR-012, as `grant_rows` does); migration without widening (FR-025); a data migration cannot run as SQL inside a
migration (a CRDT write needs the HLC), so it runs after open (R7).

**Scale/Scope**: a handful of profiles, a handful of agents; Stage 1 touches about 25 Rust and 20 frontend files
(research.md lists them).

## Constitution Check

_Checked against `.specify/memory/constitution.md` 1.4.0 and `.spaex/constitution.md`._

| Principle / rule                                                                                                      | Result                                                                                                                                                       |
| --------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| I No secrets in git                                                                                                   | Pass: profiles hold rights, never credentials; the agent credential stays in `haex_mcp_agents`.                                                              |
| II No local absolute paths                                                                                            | Pass: nothing committed names a machine path.                                                                                                                |
| III Project identity device independent                                                                               | Pass: profiles are vault data; assignments name an agent id or a model id, not a path.                                                                       |
| IV, V Pinned references, opt-in sources                                                                               | Pass: no external content is referenced.                                                                                                                     |
| VI Self-modifying instructions review-gated                                                                           | Pass: no skill, snippet or permission file of an agent is edited; the plan lands through pull requests.                                                      |
| VII Relay unavailability never blocks local work                                                                      | Pass: resolving rights is a local read; profiles reach other devices by the existing sync.                                                                   |
| VIII No concealment                                                                                                   | Pass.                                                                                                                                                        |
| Conventional Commits, rebase or merge commit, no squash                                                               | Followed by the delivery below.                                                                                                                              |
| One implementation per rule (spaex/memory)                                                                            | Pass: the profile rights rows reuse `grant_rows` and `grants`; the old per-agent rows and actions are removed in the last PR, not kept next to the new ones. |
| Every device and OS                                                                                                   | Pass: no OS specific logic; Android uses the same settings UI and the same Rust.                                                                             |
| UI: save on selection, errors as toast, no success notice, no "Layout" jargon, no bare "Relay", user name "HaexVault" | Followed (R11).                                                                                                                                              |
| Specs in English, chat in German                                                                                      | Followed.                                                                                                                                                    |

No violation, so no entry in Complexity Tracking. One thing is not a violation but needs the operator's eye:
agents are device scoped (`haex_mcp_agents.vault_device_uuid`, ADR-0001) while profiles are vault wide; the
assignment therefore names an agent row of one device (R2). No ADR is needed, the choice is recorded in research.md.

**Re-check after design (Phase 1)**: unchanged, all rows still pass.

## Delivery (pull requests, in order)

Each PR leaves the app working and every agent with exactly the rights it had.

| PR  | Content                                                                                                                                                                                                                     | Visible to the user                                                 |
| --- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------- |
| A   | Migration 0054, the three tables, `agent_profiles` store and resolver, rights of external agents and Abo-Agents resolved through it, the post-open migration of existing agents (FR-024, FR-025), tests                     | nothing changes                                                     |
| B   | Live refresh of MCP sessions on any profile or assignment change; remembered rights (`env_rights`) written into the profile; the prompts name the profile (FR-011, FR-015)                                                  | the remember prompts say "gilt für alle Agenten im Agentenprofil X" |
| C   | The built-in agent: subject from the active model, enforcement of the whole rights list on its tools, migration of its device settings into a full-rights profile, chat notice for a model that is in a restrictive profile | the built-in agent's rights are profile rights                      |
| D   | Commands and catalog actions, KI → Agentenprofile (list and editor), the field "Agentenprofil" on the agent page and in the registration question, the widening confirmation, KI → Browser without rights                   | the new pages                                                       |
| E   | Removal of the old per-agent rights actions, commands and the built-in settings; naming sweep "Abo-Agent"; docs, quickstart, e2e                                                                                            | nothing new                                                         |

A and B are safe on their own. C changes what the built-in agent is gated by, so it comes with the migrated profile
and its own tests; D is the first PR that shows anything.

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
│   ├── subjects.rs                  # the agents of this device and the models of the built-in agent
│   ├── migrate.rs                   # post-open, idempotent: agents and built-in settings into profiles
│   ├── commands.rs                  # Tauri commands (contracts/)
│   └── *_tests.rs
├── identity/
│   ├── migrations.rs                # register 0054, bump HOLZI_TRIGGER_VERSION to 35
│   ├── migrations_agent_profiles.rs # new: the three tables
│   └── migrations_legacy_names.rs   # only if another branch took 0054
├── mcp_server/
│   ├── registry.rs                  # load_rights/save_rights replaced by the resolver and an assignment write
│   ├── grant_rows.rs                # unchanged shape, reused for profile rows
│   ├── live.rs                      # refresh every live session on a profile or assignment change
│   ├── manage.rs, manage_commands.rs, connection.rs, attach.rs, delegates.rs,
│   │   oauth_rows.rs, password_env.rs, registration.rs, prompts.rs   # callers switched
├── agent_browser/
│   ├── env.rs, env_rights.rs        # built-in rights from the profile; remember writes the profile
│   └── commands.rs                  # builtin setters removed (PR E)
└── chat/
    ├── session.rs                   # the model of the active session as the subject
    ├── action_commands.rs           # built-in tools filtered by the profile (PR C)
    └── tools/action_tool.rs

src/
├── components/settings/agents/      # AgentsView (kind labels), AgentDetailView (field + read-only rights)
├── components/settings/profiles/    # new: ProfilesView.vue, ProfileView.vue, ProfileMembers.vue, WideningConfirm.vue
├── components/settings/agentBrowser/AgentBrowserView.vue   # rights section removed
├── components/agents/               # RegistrationDialog.vue (profile field), the rights components reused
├── lib/agents/rights.ts, lib/agents/profiles.ts (new: widens(), labels)
├── lib/actions/settingsMcpAgentActions.ts, settingsAgentBrowserActions.ts, new settingsAgentProfileActions.ts
├── lib/settings/registry.ts, components/wm/appRoutes.ts
├── stores/mcpAgents.ts, stores/settingsActionHandlers.ts, stores/agentProfiles.ts (new)
└── i18n/locales/de.json, en.json

scripts/
├── check-mcp-actions.ts, check-settings.ts, check-agent-profiles.ts (new)
└── e2e/scenarios/mcp-agent.test.ts, agent-profiles.test.ts (new)
```

**Structure Decision**: a new top-level Rust module `agent_profiles`, because the built-in agent, the MCP server and
the agent browser all resolve rights through it and none of them should own it. The rights shape stays in
`mcp_server/grants.rs`.

## Complexity Tracking

No constitution violation to justify.
