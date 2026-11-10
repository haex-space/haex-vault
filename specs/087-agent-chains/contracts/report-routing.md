# Contract: the routing record per request (everyday report, extended)

Spec 082's contract (`specs/082-agent-harness/contracts/everyday-report.md`) promises a per-request `routed` field
that the report does not write. This feature writes it, for every request.

## Per run

| Field      | Type         | Meaning                                                                      |
| ---------- | ------------ | ---------------------------------------------------------------------------- |
| `set`      | string       | `everyday` for the embedded set, else the file name given in `HAEX_EVAL_SET` |
| `requests` | request list | One entry per request of the set, passed or failed, in the set's order       |

`failures` stays as it is (the failed requests with their calls and text).

## Per request (`requests[]`)

| Field        | Type        | Meaning                                                                        |
| ------------ | ----------- | ------------------------------------------------------------------------------ |
| `id`         | string      | The request's id                                                               |
| `pass`       | bool        | Whether it passed                                                              |
| `routed`     | string list | The domain keys of the routing call; empty without routing or when it failed   |
| `routingHit` | bool / null | Whether `routed` holds the domain of the expected first tool; `null` if none   |
| `firstOffer` | string list | The tools the first step offered, in the set's names (`x_mail_<tool>`), sorted |

The names in `firstOffer` are the set's, so reports under different extension names compare request by request.
