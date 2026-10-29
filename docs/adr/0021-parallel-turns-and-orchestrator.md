# Parallel turns, and an orchestrator that cannot approve

Status: proposed
Date: 2026-10-28

## Context

Spec 068 puts an orchestrator model in front of the built-in agent. It answers simple questions
itself, hands each task to an executor whose model depends on the task (local, small, large, or a CLI
delegate), relays the executor's questions and approvals, and lets several tasks run at once.

Today the chat can run only one turn. `ChatState` holds one operation lock for the whole turn, one
loaded model session, one generation handle and one cancellation token, and `abort_turn` cancels every
pending approval and choice. The model of a turn is whatever is loaded.

## Decision

- **Turn slots.** Each running turn gets a `TurnSlot` (turn id, thread, optional task, cancellation,
  generation, model). The operation lock becomes a read/write lock: turns share the read side; model
  loading and vault transitions take the write side. Aborting a turn, and dropping its pending
  approvals and choices, works per turn id. With orchestration off, behaviour stays as today.
- **Model per turn.** A turn receives its model when it starts. Adapters come from the model-job
  source of spec 055, so jobs and tasks share one way to reach a model. At most one local model is
  loaded; orchestrator and local stage share it with up to two concurrent sequences.
- **The orchestrator holds no power to change data or approve.** Its tool set is fixed:
  `task_dispatch`, `task_cancel`, `task_status`, `task_answer` (only for information questions) and
  `ask_user`. No catalog action is offered to it. Approvals stay with `permission::decide` and the
  user's answer; the orchestrator only phrases and relays them, through the existing request ids.
- **Executors are the existing agents.** The built-in agent with the stage's model, in a hidden task
  conversation, or a CLI delegate in a project folder. Their tools, approval levels, guardrails and
  limits do not change.

## Consequences

- The chat core changes shape. The change lands first and invisibly (spec 068 PR B) and must keep all
  existing turn tests green.
- A wrong routing decision costs money or quality but never grants a permission: the worst a
  confused or manipulated orchestrator can do is start a task, which then meets the same approvals
  as a typed request.
- Webview actions still run one at a time through the action bridge; parallel tasks wait for each
  other there. Acceptable because actions are short.
- A learned router (RouteLLM-style, or a hosted service like Jev) can sit in front of
  `task_dispatch` later; it is not part of this decision.

## Alternatives

- **Executors as model jobs (spec 055)** — jobs have no tools and no approvals.
- **Orchestrator with the full action catalog** — would let one model both decide and act, and a
  small local model would be the one approving its own plan.
- **Strictly sequential tasks** — rejected by the operator (several tasks at once, 2026-10-28).
