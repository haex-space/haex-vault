# Catalog actions may run in Rust

Status: accepted
Date: 2026-10-07

## Context

[ADR 0006](0006-actions-as-builtin-agent-tools.md) keeps one action catalog in TypeScript and runs
every action in the frontend: Rust registers one `ActionTool` per definition, and a call travels to the
webview and back. A call without a running webview fails with `action_timeout`.

The file actions of spec 044 (`files.list`, `files.search`, `files.read`, `files.copy`, …) work on the
file system and on S3. They need no UI, and FR-034 requires them to work while no haex-vault window is open.
They are also nine tools: as Rust tools with their own source they would land in every request
(`chat/tools/offer.rs`) and break the core budget of at most ten tools.

## Decision

**A catalog action may declare `runner: 'native'`.** Its definition stays in the TypeScript catalog
(name, description, input schema, `effect`, `scope`, `agentCallable`), so `find_actions`, the offer
check, the risk classes and `scripts/check-agent-actions.ts` treat it like any other action.

**Registration picks the executor.** `register_agent_actions` creates a `NativeActionTool` instead of an
`ActionTool` for native definitions. The tool keeps the source `action` and maps `effect` to the risk
class exactly as `ActionTool` does. Its executor is Rust code looked up by action id; ids without an
executor are rejected at registration.

**The executor fixes the caller.** A native executor calls a service with `Caller::BuiltinAgent` (later
`Caller::ExternalAgent { id }` from spec 021), never with a caller taken from the input (ADR 0007).

**Actions that need the UI stay in the frontend.** `files.show` opens a window and is an ordinary
frontend action; its handler asks Rust to check permissions before it opens anything.

## Consequences

- Native actions work without a window once the frontend has pushed the definitions for the vault
  session.
- There is still one catalog. A native action without a Rust executor, or a Rust executor without a
  catalog entry, is a registration error and a failing check.
- `scripts/check-agent-actions.ts` stubs native `read` actions in its harness instead of running them
  through `catalogRunner`.
- Spec 021 publishes native actions over MCP like any other action.
