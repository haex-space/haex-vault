# Data model: The reversible way first

Nothing is stored. The types live for one turn (app) or one request (measurement).

## Candidate

| Field   | Meaning                                               |
| ------- | ----------------------------------------------------- |
| `name`  | The model-facing tool name, as offered                |
| `title` | The tool's English title (extension tool or action)   |

Rule: offered in the current step, same `domain_of` as the proposal, risk class `Change`.

## Verdict

One of:

- `Final` — the user asked for exactly this final action: go on to the dialog.
- `Use(name)` — the user did not; `name` is one of the candidates: turn the proposal away.
- `NoVerdict` — `none`, unreadable, error or timeout: go on to the dialog.

## Check

| Field       | Meaning                                    |
| ----------- | ------------------------------------------ |
| `tool`      | The proposed final tool                    |
| `verdict`   | The `Verdict`                              |
| `ms`        | How long the question took                 |

State per turn: the set of tools already checked (FR-006); a tool in it skips the check.

## Flow of one proposed call

```text
unknown tool? ──► Unknown
profile gate  ──► TurnedAway
guard         ──► TurnedAway (ids, required argument)
risky ∧ ¬confirms_itself ∧ mode≠Plan ∧ routes ∧ not checked this turn ∧ candidates≠∅
   └─ check ── Use(name) ──► TurnedAway (answer names `name`)
           └─ Final / NoVerdict ──► permission dialog as today
else ──► permission dialog as today
```
