# Codex spike (T064): what the app-server reports of a web search and a command

- **Date**: 2026-11-24
- **Codex**: 0.169.0, the vault's own installation (`cli-tools/codex/0.169.0/bin/codex`), the version of
  `src-tauri/src/cli_tools/catalog.json`
- **Question** (research R7): do the notifications of `codex app-server` name a web search and a command, so the reader in
  `adapters/cli_delegate/codex.rs` can mark the conversation as having seen foreign content?

## How it ran

A Python driver (not committed) did what `spawn_codex_app_server` does:

- It started `codex -c check_for_update_on_startup=false --disable plugins --disable plugin_discovery_prefetch
--disable recommended_plugins --disable remote_plugin app-server --stdio`.
- `CODEX_HOME` pointed at an empty folder holding a copy of the ChatGPT `auth.json`, with no `config.toml` (as for an
  agent that is not registered). Variables starting with `OPENAI_` and `CODEX_` were removed from the environment.
- It sent `initialize`, then `thread/start`, then `turn/start` with:
  > Search the web for the latest stable release of the Rust programming language. Then run the shell command `uname -s`
  > in your working folder. Answer with both results in one short sentence.
- **Gated**: `approvalPolicy: "on-request"`, `approvalsReviewer: "user"`; every approval request was to be answered
  `accept` (none came).
- **Ungated**: `approvalPolicy: "never"`, `sandbox: "workspace-write"`.

The token file was unchanged after both runs and deleted.

## What came back

The notifications per run, by method and `params.item.type`. Both modes gave the same set:

| Notification                                         | Gated | Ungated |
| ---------------------------------------------------- | ----: | ------: |
| `item/started` / `item/completed` `webSearch`        | 1 / 1 |   1 / 1 |
| `item/started` / `item/completed` `commandExecution` | 1 / 1 |   1 / 1 |
| `item/started` / `item/completed` `agentMessage`     | 2 / 2 |   2 / 2 |
| `item/started` / `item/completed` `userMessage`      | 1 / 1 |   1 / 1 |
| `item/agentMessage/delta`                            |    68 |      72 |
| `mcpServer/startupStatus/updated` (`codex_apps`)     |     2 |       2 |
| approval requests                                    |     0 |       0 |
| `turn/completed`                                     |     1 |       1 |

The rest were `thread/started`, `thread/status/changed`, `turn/started`, `thread/tokenUsage/updated`,
`account/rateLimits/updated`, `account/updated` and `remoteControl/status/changed`.

The two items, redacted (ids shortened, working folder replaced):

```json
{"method": "item/completed", "params": {"item": {"type": "webSearch", "id": "exec-21dd…",
  "query": "site:blog.rust-lang.org Rust November 2026 stable release …",
  "action": {"type": "search", "queries": ["…", "…"]},
  "results": [{"type": "text_result", "domain": "blog.rust-lang.org", "snippet": "…"}]},
  "threadId": "01a2…", "turnId": "01a2…"}}

{"method": "item/started", "params": {"item": {"type": "commandExecution", "id": "exec-7bd8…",
  "command": "/bin/bash -lc 'uname -s'", "cwd": "<run folder>", "source": "unifiedExecStartup",
  "status": "inProgress", "commandActions": [{"type": "unknown", "command": "uname -s"}],
  "aggregatedOutput": null, "exitCode": null}, "threadId": "01a2…", "turnId": "01a2…"}}
```

The `item/completed` of the command carries `"aggregatedOutput": "Linux\n", "exitCode": 0`.

## Findings

1. **Both are named, in both modes.** A web search is an item of type `webSearch`, and a command is an item of type
   `commandExecution`. Each has an `item/started` before its result and an `item/completed` with it.
2. **Gated mode asks nothing for a command the sandbox allows.** `uname -s` ran without
   `item/commandExecution/requestApproval`. The approval requests cannot serve as the signal; the item events must.
3. **The web search needs no approval in either mode.** Its results reach the model inside Codex; only the item event
   shows it.
4. **Order.** Ungated, the two ran in parallel (`webSearch` started, then `commandExecution` started and completed, then
   `webSearch` completed). The reader must not expect one item to finish before the next starts.
5. **Codex starts an MCP server of its own, `codex_apps`, although plugins are disabled.** Its calls would arrive as
   `mcpToolCall` items with `server: "codex_apps"`, and they are foreign content too.
6. **The schema has 19 item types** (`codex app-server generate-json-schema`): `userMessage`, `hookPrompt`,
   `agentMessage`, `functionCallOutput`, `plan`, `reasoning`, `commandExecution`, `fileChange`, `mcpToolCall`,
   `dynamicToolCall`, `collabAgentToolCall`, `subAgentActivity`, `webSearch`, `imageView`, `sleep`, `imageGeneration`,
   `enteredReviewMode`, `exitedReviewMode`, `contextCompaction`.

## Decision (operator, 2026-11-24)

**Read the item events; do not switch off Codex's web search.** On `item/started` (the earliest line, before any output),
the reader marks the delegate thread foreign for:

- `webSearch` and `commandExecution`;
- `mcpToolCall` of any server other than haex-vault's own, whose calls `dispatch.rs` judges itself;
- `dynamicToolCall` and `functionCallOutput` (tools whose content haex-vault cannot see);
- any item type not on the own list below, so a new type asks once too often rather than once too rarely.

Own, with no mark:

- the conversation itself: `userMessage`, `hookPrompt`, `agentMessage`, `plan`, `reasoning`, `contextCompaction`;
- the run folder, which holds only the user's attachments: `fileChange`, `imageView`, `imageGeneration`;
- `sleep`, `enteredReviewMode`, `exitedReviewMode`;
- `collabAgentToolCall` and `subAgentActivity`: a sub-agent's own items arrive as items of their own.

Commands are marked whatever their exit code, since a failing command can also print foreign text. The Claude Code
reader marks `Bash` the same way.

## Left open

- Whether the items of a sub-agent arrive on the same stream (with its own `threadId`) was not tested; no sub-agent
  started in this turn. The reader therefore does not filter by `threadId`. T077 checks it with the real Codex.
- `codex_apps` stays on (operator, 2026-11-24): its calls count as foreign content, so it needs no switch.
