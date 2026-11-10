# Contract: the chain development set

## File

`specs/087-agent-chains/analysis/chainset.json`, in the everyday set's format (`version`, `mailTools`, `requests`)
and against the same fixture world (`chat/eval/everyday/world.json`). Its `mailTools` pin is the everyday set's.

## Running it

`HAEX_EVAL_SET=<path to chainset.json>` makes `measure_everyday_voice` load that file instead of the embedded set;
every other variable works as for the everyday set. The report has the same form, with `set` naming the file.

A set file that does not parse, or whose expected tools are neither holzi's tools nor the pinned mail tools, fails
the run before the first request (as `load_set` does for the embedded set).

## What it holds

- 69 requests (35 in the first version, 63 in the second), German and English, clean and spoken, in the everyday set's categories.
- At least 4 requests each for: looking up instead of asking (a sender, a subject or a day named), finishing a chain
  (show, answer, move after finding), deleting a mail (into the trash, and once "for good"), a day's bounds (a day
  other than yesterday).
- What the everyday set asks about is avoided where the world allows: other senders, passwords, files and days.
- Every area (R13, operator 2026-11-09): holzi's conversations (in `app_setting`, opened or renamed after
  `chat_conversations_list`), contacts, files (found, then shown), settings and passwords beside mail; at least a
  third of the requests are not mail.
- Requests that name only a person or a thing, not the area ("Was will die Sparkasse von mir?", "Antworte der IT …",
  "Wo ist die Rechnung für die Fahrradreparatur?"); where more than one area can hold the answer, each is an
  `anyOf` chain of its own.
- Requests for what holzi cannot do (a taxi, a table, the light, the heating, a transfer, a pizza), so that wording
  that leads to more searching shows whether it costs the refusals (added after the next-area step, 2026-11-10).

## Use

Only to tune and choose wording and order (spec FR-012). A step's success is judged on the everyday set.
