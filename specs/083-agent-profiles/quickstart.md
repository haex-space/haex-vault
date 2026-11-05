# Quickstart: validating agent profiles

**Feature**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md)

Prerequisites: a development build (`pnpm tauri:dev` inside `nix develop`), an external agent that can connect over
MCP (the e2e helper in `scripts/e2e/lib/mcp-agent.ts` works, or Claude Code), a local model and an API model for M5.

## Automated

```sh
nix develop --command scripts/with-nix-host-bridge.sh cargo test --manifest-path src-tauri/Cargo.toml -j 4 --lib -- agent_profiles mcp_server agent_browser grant_rows
nix develop --command pnpm check:mcp-actions
nix develop --command pnpm check:settings
nix develop --command pnpm check:agent-profiles
```

The e2e scenarios `mcp-agent.test.ts` and `agent-profiles.test.ts` run through the usual e2e rig (see the e2e notes of
spec 016); they are CI only.

## Manual scenarios

- **M1 Strict in one step (US1, FR-006, FR-010, FR-011)**: register an agent; it is in "Standard". Create the profile
  "Vorsichtig" with nothing granted; open the agent and pick "Vorsichtig" in "Agentenprofil". Its next call to a
  browser or password tool is refused and its tool list no longer offers them. A second agent is unchanged.
- **M2 Configure once (US2, FR-016)**: open KI → Agentenprofile → Standard, set the browser to "Lesen". Every agent in
  "Standard" has it from its next call; a new agent has it too. Then widen it to "Lesen und ändern": the page asks
  and names the number of agents first (agents of every device without a profile, plus models), also when that number
  is 0; "Abbrechen" restores the control. Adding a website to a profile, or allowing its requests, asks too.
- **M3 One profile at most (FR-006, SC-003)**: add an agent that is in "Vorsichtig" to a third profile from the third
  profile's page: it moves, the page of "Vorsichtig" no longer lists it. Delete a profile with two agents: the page
  names "2", afterwards both are in "Standard".
- **M4 Registration (US4, FR-013)**: connect an unregistered agent. The question shows the rights of "Standard" kind
  by kind (read only) and a field "Agentenprofil"; registering with "Vorsichtig" puts it there. An Abo-Agent registered
  in the chat shows the same field and the text says "Abo-Agent".
- **M5 Local agent per model (US5, SC-006)**: create "Lokal" with passwords on; assign the local model, not the
  API model. In the agent browser, ask the local agent with the local model to fill a login: it may; with the API
  model it may not, and the chat notice names the profile and says the password fill is missing. Open KI → Browser: no rights there any more.
- **M6 Remember writes the profile (US6, FR-015)**: two agents in one profile; answer a website question of one with
  "Für diese Website merken". The prompt said "gilt für alle Agenten im Agentenprofil …". The other agent opens the
  website without asking; an agent in another profile still asks. Remove the website on the profile page: gone for
  both.
- **M7 Migration (US7, FR-024, FR-025)**: on a vault from before this change with one factory-rights agent, one agent
  with a browser right and the local agent with `agent_browser.builtin` on: after the update the first is in
  "Standard", the second has a profile with its name and the same rights, the built-in models are in
  "Lokaler Agent" with the old browser rights and full topics. Restart: nothing changes. Compare `tools/list`
  before and after for each agent.
- **M8 Two devices (SC-003)**: on two synced devices assign the same agent to two different profiles at the same time;
  after sync it is in exactly one.
- **M8b A change from another device (FR-011)**: an agent connected on device A, in "Vorsichtig". On device B, turn
  the browser off in "Vorsichtig". After the sync, without reconnecting, the agent on A no longer lists the browser
  tools, and its next browser call is refused.
- **M9 Expiry (FR-014)**: let an agent in "Vorsichtig" expire (set `last_seen_at` back 91 days in a test vault),
  connect again: the question appears with "Vorsichtig" preselected.
