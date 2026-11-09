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

- **M-Abo Two profiles, Abo-Agent and local (PR J, FR-027 to FR-030)**: make the profiles "Lokal" (browser off, "Abo-Agenten"
  group "Standard") and "Abo" (browser on, "Abo-Agenten" group "Ungated"); put a local model into the first and a Claude model
  into the second; make "Abo" active: the chat offers only the Claude model and the Abo-Agent gets the browser tools without
  any registration question; make "Lokal" active: the chat offers only the local model. A model that is in neither is not
  offered; "Standard" offers it again.

- **M1 Strict in one step (US1, FR-006, FR-010, FR-011)**: register an agent; it is in "Standard". Create the profile
  "Vorsichtig" with nothing granted; open the agent and pick "Vorsichtig" in "Agentenprofil". Its next call to a
  browser or password tool is refused and its tool list no longer offers them. A second agent is unchanged.
- **M2 Configure once (US2, FR-016)**: open KI → Agentenprofile → Standard, set the browser to "Lesen". Every agent in
  "Standard" has it from its next call; a new agent has it too. Then widen it to "Lesen und ändern": it applies at
  once, with no question; so does every other right.
- **M3 Several profiles, one active (FR-006, FR-006a, SC-003)**: add an agent that is in "Vorsichtig" to a third profile
  from the third profile's page: it stays in "Vorsichtig" too, and both pages list it. Make "Vorsichtig" active in the list
  of profiles (no question), then the third profile: the agent has the rights of
  whichever is active, an agent that is in neither has those of "Standard". Delete a profile with two members: the page
  names "2", afterwards neither is a member; if it was active, "Standard" is.
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
- **M7 Reset (US7, FR-024, FR-025)**: on a vault from before this change with one agent that had its own rights (a browser
  right, say) and the local agent with `agent_browser.builtin` on: after the update both agents and every model of the
  local agent are in "Standard" with the factory rights (reading, no browser, no passwords, no commands); the agents are
  still registered (same credential, approval mode and blocks) and connect without a new question; the two old local
  agent preferences are gone. Create a profile, assign the agent, restart: the profile and the assignment are untouched.
- **M8 Two devices (SC-003)**: on two synced devices add the same agent to two different profiles at the same time:
  after sync it is a member of both. Set two different active profiles at the same time: after sync one is active, the
  later change.
- **M8b A change from another device (FR-011)**: an agent connected on device A, in "Vorsichtig". On device B, turn
  the browser off in "Vorsichtig". After the sync, without reconnecting, the agent on A no longer lists the browser
  tools, and its next browser call is refused.
- **M9 Expiry (FR-014)**: let an agent in "Vorsichtig" expire (set `last_seen_at` back 91 days in a test vault),
  connect again: the question appears with "Vorsichtig" preselected.
