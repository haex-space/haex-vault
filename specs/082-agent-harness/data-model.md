# Data model: A harness that lets small local models do everyday tasks

No database changes. Everything is declared in code or lives for one turn.

## AgentDomain (new, declared)

See `contracts/domains.md`. Holds `key`, `titles`, `description`, `alsoNeeds`. Extensions contribute one domain
each.

## Routing (per turn)

| Field     | Type          | Rule                                                    |
| --------- | ------------- | ------------------------------------------------------- |
| `domains` | key list, ≤ 2 | Parsed from the routing call; empty means today's offer |

## Offer (per step, changed)

| Part            | Rule                                                                                        |
| --------------- | ------------------------------------------------------------------------------------------- |
| Domain tools    | All, if the domain has at most 12; else its reading tools, then the word-matched rest, ≤ 12 |
| Declared extras | `alsoNeeds` of each routed domain                                                           |
| Always          | `find_actions`, `ask_user`; the command tool where the platform has it                      |
| Window tools    | Only for the `wm` domain or as an extra                                                     |
| Limit           | At most 30 tools (`MAX_ACTION_OFFER`)                                                       |

## Seen ids (per turn, new)

The strings of every earlier tool result of the turn and the user's message. A call's id arguments must each be
found there or in the tool's schema (research R6).

## Call ending (measurement, new)

```text
proposed ─► guard refuses ───────────────► turnedAway
        └─► permission: allow ─► asked? ─► ran / ranUnasked (risky: executedUnasked; else changedUnasked)
        └─► permission: ask ──► played user agrees (asked) ─► ran
                             └► played user declines ──────► stopped
```
