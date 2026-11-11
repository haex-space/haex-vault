# Contract: what outside callers see of a pass

Callers: the browser autofill and `entry.fill` (spec 065), the external bridge (spec 049), the built-in agent, external agents
over MCP (spec 021), extensions, and holzi's own functions that read credentials. All reach entries through
`PasswordsService`; the rules below are enforced there (research R11), not in each caller.

## The rule

| Caller                                                                         | Sees a pass?              | How                                                                                                                                                                                            |
| ------------------------------------------------------------------------------ | ------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Autofill, `entry.fill`, `browser.credentials.fill`                             | Never                     | `headers_for_page`, `list_autofill_shapes`, `read_secret_item`, `totp_code` default to logins; a pass is refused even by id.                                                                   |
| External bridge (`get_items`, `get_totp`, passkey `entries`, `passkey_create`) | Never                     | Same filters; a passkey is never offered, created or linked on a pass.                                                                                                                         |
| Built-in agent, `passwords_items_search`                                       | Yes, as a narrowed header | id, title, tags, folder name, `kind`, `passKind`, `startAt`/`startZone`/`startDate`, `expiresAt`; never a code. Unscoped by tag, as for logins (Z2).                                           |
| Built-in agent, `passwords_secret_get`                                         | Never                     | The built-in agent reads no secret (ADR 0007); the action refuses a pass for any caller.                                                                                                       |
| External agent over MCP                                                        | Yes, by grant             | Search carries the type and a type filter; tag-scoped grants cover passes by tag; no code. `passwords_secret_get` refuses a pass.                                                              |
| Extensions (SDK `passwords`)                                                   | Only when asked           | The list takes a `type` filter, default `login`; a summary carries `type`; reading, updating or deleting a pass from an extension is refused (no writer but the user's own interface, FR-027). |
| `remote_storage/credentials.rs`, `files/local/drives/unlock.rs`                | Never                     | Skip passes when looking for credentials.                                                                                                                                                      |
| Import duplicate detection of logins (`service/import.rs` `existing_keys`)     | Never                     | Counts logins only.                                                                                                                                                                            |

## `passwords_items_search` (agent tool)

Input adds an optional `type`: `"login"` (default) | `"pass"` | `"any"`. The description says logins are the default and a
pass is a card or ticket without a secret to the agent. Result item (today `id`, `title`, `tags`, `folder`, `hasTotp`) gains:

```text
kind: "login" | "pass"
passKind?: string            // pass only
start?: { date } | { at, zone }
expiresAt?: string           // date
```

`hasTotp` is false for a pass. No `codes`, no `value`. The folder is the entry's folder name (the search matches the whole
path, research R10). The schema is in `src-tauri/src/chat/eval/tools.json` and `src/lib/actions/passwordsActions.ts`; the
native executor is `passwords/agent_actions.rs` (`header_json`).

## Grants and tag scope

- External agents: `haex_agent_profile_grants` group `passwords`, `PasswordRights` and `covers_scope` (`mcp_server/grants.rs`).
  A pass is covered through `headers_in_scope` and `authorize_*` with its tags; no new setting.
- Extensions: the `Passwords` permission (`Target::Tag | Any`); denied tags and a denied `*` apply to passes.
- Bridge: `haex_browser_clients` (`access`, `scope_tags`, `default_tag`).
- Built-in agent: browser fill follows the profile's `passwords.tags`; its search has no tag scope for logins or passes
  (spec 021 Z2). Limiting it is a separate change.

## Acceptance checks (SC-004, SC-005)

- With a seeded pass whose code value is a unique marker string, a test runs every read path (list, headers, agent search,
  agent header JSON, MCP search and get, extension list and read, bridge `get_items`, history list, tab history, logs of a
  failing call) and finds the marker in none of them.
- With a pass whose title, address and tag match a login's, a test runs every credential lookup (autofill by site,
  `entry.fill`, bridge, credential tools, remote-storage credentials, drive unlock) and gets 0 passes.
- A passkey create or link onto a pass is refused.
