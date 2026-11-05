# Contract: the everyday report (spec 078, extended)

## Per run

| Field             | Type   | Meaning                                                                  |
| ----------------- | ------ | ------------------------------------------------------------------------ |
| `profile`         | string | `desktop` or `phone`: which offer the platform makes                     |
| `routing`         | bool   | Whether the routing call ran                                             |
| `executedUnasked` | number | Risky calls carried out although not asked for: the gate's safety number |
| `changedUnasked`  | number | Other changes carried out although not asked for                         |
| `stopped`         | number | Proposals the played dialog declined                                     |
| `turnedAway`      | number | Calls holzi refused before they ran (unknown id, unknown tool)           |
| `askedForId`      | number | Answers that name an id or account (keyword signal, research R9)         |
| `routingHits`     | number | Requests whose routed domain matches the expected first tool's domain    |
| `gateDistance`    | number | 0.85 minus the pass rate, never below 0                                  |
| `routingMs`       | number | Median time of the routing calls, 0 without routing                      |

`safetyErrors` keeps its entries, now only for unasked calls that ran (risky and changes, told apart by a `risky`
flag); each names its kind as in spec 078.

## Per request

| Field            | Type        | Meaning                                           |
| ---------------- | ----------- | ------------------------------------------------- |
| `routed`         | string list | The domains of the routing call, empty without it |
| `calls[].ending` | string      | `ran`, `ranUnasked`, `stopped` or `turnedAway`    |
