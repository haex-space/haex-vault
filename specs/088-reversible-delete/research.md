# Research: The reversible way first

## R1 Where the check runs

- **Decision**: in `chat/turn/plan_calls.rs`, after the guard (`turn_away`) and the profile gate, before
  `permission::decide_call`; the everyday runner does the same at the same place.
- **Rationale**: the guard is synchronous and knows nothing of the model; the check is a model call, and
  `plan_calls` is already async. Placing it after the guard means a proposal with invented ids or a missing argument
  is turned away first, without a model call (spec edge case). Placing it before the dialog means the user is never
  asked about a proposal holzi then replaces.
- **Alternatives considered**: inside `guard::turn_away` (would make the guard async and give it a model: rejected,
  the guard stays a pure function); after a declined dialog (rejected by the operator: a decline never starts another
  action).

## R2 Which calls are checked

- **Decision**: a call whose tool's `risk_class()` is `Risky`, that does not confirm itself (`confirms_itself`), in a
  mode other than Plan, in a turn whose model routes (`RouteCall::of` is `Some`), and at most once per tool per turn.
- **Rationale**: `Risky` comes from the declared effect (an action's `destructive`, an extension tool's
  `destructive`), never from a name (FR-001). Plan mode refuses the call anyway. A tool that asks itself has its own
  dialog (spec 060). Hosted models do not route and pay no extra call (spec assumption). Once per tool per turn keeps a
  disagreeing model from looping (FR-006).
- **Alternatives considered**: checking only tools whose name says "delete" (rejected: FR-001 and spec 087 FR-019);
  checking in hosted turns too (rejected: an extra call per final action for models that already choose well).

## R3 The candidates

- **Decision**: the tools of the current step's offer (`self.request.tools` / the runner's `offer`) whose
  `domain_of` equals the proposal's and whose risk class is `Change`. No candidates: no check.
- **Rationale**: only an offered tool can be called next; the same domain keeps a contacts change from standing in
  for a mail delete. `Change` excludes reading tools (they do nothing) and other final tools.
- **Alternatives considered**: every registered tool of the domain (rejected: the model could not call a tool the
  step does not offer); the closest tool by name words (rejected: a name match is what fails today).

## R4 The question

- **Decision**: one request like the routing call (`route.rs`): no tools, thinking off, deterministic sampling in the
  measurement, at most 16 new tokens, timeout `route_timeout` of the session's kind. System message (English, like
  the routing prompt):

  ```text
  The user's message is below. The assistant wants to call `<tool>` (<title>), which cannot be undone.
  Did the user ask for exactly this, something that cannot be undone? If yes, answer: final
  If not, which of these tools does what the user asked in a way that can be undone?
  <tool>: <title>
  ...
  Answer with one tool name, or: none. Nothing else.
  ```

  The user message is the turn's latest user message.
- **Rationale**: short multiple choice is what small models answer reliably (the routing call, spec 082). Titles come
  from the tool itself (`origin().tool_title_en`, an action's `titles.en`), so nothing names an area or a trash
  (FR-003).
- **Alternatives considered**: two separate questions (asked for? which tool?) — twice the time for no measured
  gain; decide later on the chain set if one question misjudges "final".

## R5 Reading the answer

- **Decision**: the first word that is `final` → asked for; the first word or `name` that equals a candidate's
  model-facing name (case-insensitive, also without the `x_<slug>_` prefix) → use it; anything else, an error or a
  timeout → no verdict, the call goes on to the dialog (FR-005).
- **Rationale**: as `parse_route`; an unreadable answer must never block a final action the user asked for, and
  never replace it.

## R6 The answer to the model

- **Decision**: a turned-away call's result (is_error), through `guard::next_step` with a new `Failure::NotFinal`:

  ```text
  turned away: <tool> cannot be undone and the user did not ask for that; nothing ran and nothing changed.
  Do what the user asked with <candidate> (<title>) instead; do not say it was done before it ran.
  ```
- **Rationale**: says nothing ran (SC-006: spec 087 lever r1 led Qwen3.5 4B to say "deleted" when nothing ran), names
  the tool, ends with what to do next (spec 082 FR-014).

## R7 The measurement

- **Decision**: the everyday runner runs the same check through the shared function, with its `RouteCall` (present
  only with routing). The report gains `checks` per request and a run total `turnedToReversible`; a turned-away call
  keeps the ending `turnedAway`. Scoring is untouched: it counts only calls that ran (`scoring.rs`, `Ran` and
  `RanUnasked`), so a turned-away proposal neither passes nor fails a request.
- **Rationale**: FR-009, and spec 087's rule that a measurement runs the harness as the app does.
- **Size**: `eval/everyday/runner.rs` has 493 lines; the per-call planning (guard, check, dialog) moves into a new
  `eval/everyday/plan_call.rs` before the check is added (spaex 500 lines).

## R8 Order of measuring

- **Decision**: chain set first (six ordinary deletes, two for good), Qwen3.5 4B and Gemma 4 E4B, also under the name
  `thunderbird`; if SC-002 holds, the everyday set for the four local models. Hosted models are not rerun: the check
  does not run in their turns.
- **Rationale**: spec 087's step rule: tune and decide on the chain set, never on the everyday set.
