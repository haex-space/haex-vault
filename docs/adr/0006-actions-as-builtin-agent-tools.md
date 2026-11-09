# Actions are the built-in agent's tools, called in process, not over MCP

Status: accepted
Date: 2026-09-30

## Decision

Every agent-callable action of the window manager and the settings (specs 020
and 023) is offered to the models that holzi's own chat drives: local models
and providers reached with an API key. There is one definition per action and
two entrances to it.

| Entrance         | Caller                                   | Path                              | Permission                                     |
| ---------------- | ---------------------------------------- | --------------------------------- | ---------------------------------------------- |
| Chat (spec 032)  | local or cloud model, built-in agent     | in process, through the tool loop | approval mode (Manual / Auto / Plan), spec 003 |
| MCP server (021) | Claude Code, Codex, Hermes, other agents | MCP, after registration           | per-agent grants (spec 021)                    |

**The chat does not go through MCP.** The action runner and the tool loop live
in the same app. An MCP hop inside one process would add serialization and a
server without adding a boundary. The action definition (name, description,
JSON-schema input) already has the shape MCP `tools/list` expects, so spec 021
publishes the same definitions later without rework.

**The frontend owns the definitions.** The catalog stays in TypeScript
(`src/lib/actions/`). After the vault session starts, the frontend pushes the
definitions for the built-in agent to Rust (`set_agent_actions`). Rust keeps one
`ActionTool` per action in the existing `ToolRegistry` (source `action`). A call
goes out as an event (`action-call-request`) and comes back through a command
(`respond_action_call`), the same pattern as tool approvals. The action runs in
the frontend with caller `builtinAgent`, so input validation, target rules, the
guardrail lock and the opening of tab-bound apps stay in one place.

**Three risk stages.** The effect of an action maps to the approval gate:
`read` → `Safe`, `write` → `Change`, `destructive` → `Risky`. Auto lets `Safe`
and `Change` run and asks for `Risky`; Plan allows only `Safe`; Manual asks for
everything. Spec 003 had two stages (`Safe`, `Risky`); `run_command` and MCP
tools stay `Risky`, so their behavior does not change.

**One optional catalog field.** `builtinAgentCallable` (default `true`) keeps
actions that would re-enter the running chat turn (`chat.message.send`,
`chat.message.retry`, `chat.reply.cancel`) or need the operation lock the
running turn holds (`settings.models.activate`, `settings.models.deactivate`)
away from the built-in agent while
external agents (spec 021) can still call them. `agentCallable: false` would
lock them for spec 021 as well, which is not wanted. Guardrail actions stay
`agentCallable: false` and are never offered to any agent.

**Spec 021 takes over** registration, per-agent grants, the MCP server and the
delivery of the same definitions to CLI delegates. ADR-0005 stays reserved for
that spec.

## Consequences

- A model can only do what an action allows; there is no path around the action
  runner, and no second copy of the catalog in Rust.
- The approval dialog, the tool rows in the transcript and the persisted
  messages work unchanged, because actions are ordinary tools of the loop.
- Changing the tool loop's risk classes touches the delegate approval bridge
  and the frontend types; both are covered in the plan of spec 032.
- A call needs a running webview. If none answers, the call fails with
  `action_timeout`, which the model sees as a tool error.

A catalog action may run in Rust instead of the frontend; see
[ADR 0011](0011-native-catalog-actions.md).
