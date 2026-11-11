# Contract: checks in the everyday report

Extends `specs/082-agent-harness/contracts/everyday-report.md` and
`specs/087-agent-chains/contracts/report-routing.md`.

## Per run

| Field                | Type   | Meaning                                                    |
| -------------------- | ------ | ---------------------------------------------------------- |
| `turnedToReversible` | number | Final proposals the check turned away (also in `turnedAway`) |

## Per request

| Field              | Type        | Meaning                                                       |
| ------------------ | ----------- | ------------------------------------------------------------- |
| `checks`           | object list | One entry per check in the request, in order; empty without  |
| `checks[].tool`    | string      | The proposed final tool, in the set's names (`x_mail_<tool>`) |
| `checks[].verdict` | string      | `final`, `use` or `noVerdict`                                 |
| `checks[].use`     | string      | The named candidate, in the set's names; only with `use`      |
| `checks[].ms`      | number      | How long the question took                                    |

`calls[].ending` is unchanged: a proposal the check turned away ends `turnedAway`. Scoring is unchanged.
