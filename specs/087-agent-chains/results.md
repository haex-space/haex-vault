# Results: Small local models finish what they start

Machine: RTX A2000 8 GB laptop. Local models through holzi's llama.cpp engine (CUDA build), the context the memory
plan picks (no `HOLZI_LLAMACPP_CONTEXT`); hosted models through the subscription, without routing. haex-mail under
the app's names (`x_haexmail_<tool>`).

## Baseline

Measured on 2026-11-09 at 0d47c23f (`main` with #853, #856, #857, #903, #904), before any change of this feature
(`runs/baseline/`, the chain set in `runs/chain/baseline/`).

| Model             | Everyday (106) | Rate   | executedUnasked | Routing hits (of 78) | Context | Chain set (35) |
| ----------------- | -------------- | ------ | --------------- | -------------------- | ------- | -------------- |
| Claude Opus 5.5   | 102            | 96.2 % | 0               | –                    | –       | –              |
| Claude Sonnet 5.5 | 100            | 94.3 % | 0               | –                    | –       | –              |
| Claude Haiku 5.5  | 98             | 92.5 % | 0               | –                    | –       | –              |
| Qwen3.5 4B        | 77             | 72.6 % | 0               | 65                   | 65536   | 18             |
| Gemma 4 E4B       | 77             | 72.6 % | 0               | 70                   | 65536   | 13             |
| Gemma 4 E2B       | 56             | 52.8 % | 0               | 65                   | 131072  | –              |
| Qwen3 1.7B phone  | 39             | 36.8 % | 0               | 58                   | 32768   | –              |

- **The goal (SC-002) is 80 for each target model**: both need 3 more requests.
- **Compared with spec 082** (`specs/082-agent-harness/runs/kept/haex-mail/`, the same app names at 8192 tokens):
  Qwen3.5 4B 78 → 77 (5 requests newly failed, 4 newly passed, mostly `clarify` and `no_tool`), Gemma 4 E4B 77 → 77
  (2 and 2), Gemma 4 E2B 56 → 56, Qwen3 1.7B phone 41 → 39 (5 and 3). The hosted models ran under `x_mail_` in spec
  082 (101, 98, 97); under the app's names they pass 102, 100 and 98. Which of the differences come from the larger
  context, the request fitting of #903 or the agent profiles of spec 083 cannot be told apart from these runs.
- **The context differs between runs of one model**: the memory plan gave Qwen3.5 4B and Gemma 4 E4B 131072 tokens in
  the chain-set runs and 65536 in the everyday runs, as it halves the context when less memory is free at load time.
  No request of the sets comes near either size.

### Where the target models fail

By first cause (`specs/082-agent-harness/analysis/failures.py`), with where each failed request was routed
(`requests[].routed`, new in this feature):

| Cause                                                                      | Qwen3.5 4B | Gemma 4 E4B |
| -------------------------------------------------------------------------- | ---------- | ----------- |
| Tool search first, then a question (`never_right_tool:after_find_actions`) | 0          | 13          |
| The chain stops early (`incomplete_chain`)                                 | 7          | 8           |
| Wrong arguments (`bad_args`)                                               | 6          | 2           |
| No call at all (`no_call`)                                                 | 1          | 4           |
| Another area's tools (`never_right_tool:other`, `wm_detour`, web)          | 7          | 0           |
| A call where a question or a refusal was expected                          | 8          | 2           |
| Failures                                                                   | 29         | 29          |

- **Gemma 4 E4B's tool searches are not a routing miss** (research R2): of its 17 failures that start with
  `find_actions`, 15 were routed to the right area, and the mail ones had `x_mail_find_messages` in the first offer.
  Its 4 failures without a call were routed to mail too. The model ignores tools it is offered; the levers are R6.1
  and R6.2, not the routing description.
- **Qwen3.5 4B's other-area failures are routing misses**: "Was schreibt die Hausverwaltung?" (both phrasings) and
  "What did Tom write?" went to files, "Antworte Anna …" (both) to chat, a spoken Dr. Huber request to the browser,
  one spoken mail request to no area. Seven of its failures need the routing of spec 082 (tuned on its development
  set) rather than this feature's levers.
- **Deleting**: Qwen3.5 4B proposes deleting Bernd's mail for good in all four phrasings, as in spec 082.
- **Values and days**: Qwen3.5 4B's six `bad_args` are "yesterday" without its end (4) and `"read": "True"` (2).

### The chain set

| Category (of)  | Qwen3.5 4B | Gemma 4 E4B |
| -------------- | ---------- | ----------- |
| mail_read (12) | 5          | 5           |
| mail_tidy (13) | 4          | 4           |
| mail_reply (4) | 3          | 0           |
| password (6)   | 6          | 4           |
| Total (35)     | 18         | 13          |

## Step: values as meant (US4)

Measured at 084921d6 (rebased since onto the review commit and the merge of `main`): holzi turns `"true"`/`"false"` into a yes/no and a number written as text into a number when the tool's
schema declares that type (`chat/turn/arg_forms.rs`), before the guard, the dialog, the tool and the stored call
(`runs/steps/value-forms/`).

| Model             | Baseline | Step | Newly passed                       | Newly failed                                            |
| ----------------- | -------- | ---- | ---------------------------------- | ------------------------------------------------------- |
| Qwen3.5 4B        | 77       | 79   | `mail_tidy-de-3`, `mail_tidy-en-3` | –                                                       |
| Gemma 4 E4B       | 77       | 77   | –                                  | –                                                       |
| Gemma 4 E2B       | 56       | 56   | –                                  | –                                                       |
| Qwen3 1.7B phone  | 39       | 39   | –                                  | –                                                       |
| Claude Opus 5.5   | 102      | 103  | `contact-en-3`                     | –                                                       |
| Claude Sonnet 5.5 | 100      | 101  | `mail_tidy-en-1`                   | –                                                       |
| Claude Haiku 5.5  | 98       | 97   | 3                                  | `file-de-1`, `file-de-1-s`, `file-en-2`, `no_tool-de-4` |

- **The step does what it should**: Qwen3.5 4B's two `"read": "True"` failures pass; the local models change
  nowhere else (they run deterministically).
- **The hosted changes are run-to-run variance**: the hosted models do not run deterministically, and none of the
  calls of their changed requests carries a yes/no or a number argument, so the step did not touch them. Haiku 5.5's
  newly failed requests are tool searches for a file and a shopping list.
- **Kept**, although Haiku 5.5 ends one below its baseline: the rule "no hosted model below its baseline" cannot tell
  a step's effect from a non-deterministic model's variance; for hosted models, the requests the step touched decide.
- **Seen on the way**: Haiku 5.5's `find_actions` search for "Steuererklärung" returned only mail tools, so it said
  holzi can only move mails. The tool search's ranking is a general lever outside the mail extension.

## Baseline on the truthful world (T052)

Measured at 57b52762 (`runs/baseline-world/`, `runs/chain/baseline-world/`), after the operator asked for general
levers (spec Decision 7): the world answers every reading tool of holzi's areas (research R12), opens only apps that
exist and finds files by every word, invented ids are scored by one rule for every area (`ids.rs`), and the chain set
covers every area (R13, 63 requests). Every later step is compared with this baseline.

| Model             | Value forms (old world) | Baseline | Newly passed                                                 | Newly failed   |
| ----------------- | ----------------------- | -------- | ------------------------------------------------------------ | -------------- |
| Qwen3.5 4B        | 79                      | 79       | –                                                            | –              |
| Gemma 4 E4B       | 77                      | 79       | `clarify-en-2`, `clarify-en-2-s`                             | –              |
| Gemma 4 E2B       | 56                      | 56       | –                                                            | –              |
| Qwen3 1.7B phone  | 39                      | 44       | `mail_read-de-5`, `mail_new-de-2`, `mail_new-en-1(-s)`, `-2` | –              |
| Claude Opus 5.5   | 103                     | 102      | –                                                            | `contact-en-3` |
| Claude Sonnet 5.5 | 101                     | 101      | –                                                            | –              |
| Claude Haiku 5.5  | 97                      | 99       | `no_tool-de-4`, `file-de-1-s`, `file-en-2`                   | `clarify-en-4` |

- **Gemma 4 E4B, from the world**: for "Switch that over", `wm_state_get` answered `{"ok": true}` before and Gemma
  ended without a word; told the open window, it asks back, as the set expects.
- **Qwen3 1.7B, from the scoring**: it sends `"account": ""`; the old mail list counted the empty string as an
  invented id, the guard's rule counts it as not given.
- **Qwen3.5 4B passes the same 79**, but has two more changes instead of a question: for "Stell das mal um" it renames
  both conversations, which it can see now. Neither is risky; no model ran a risky call without consent.
- **The hosted changes are run-to-run variance**, as in the value-forms step: none of the changed requests calls a
  tool whose answer changed.

### The chain set across areas

| Category (requests) | Qwen3.5 4B | Gemma 4 E4B |
| ------------------- | ---------- | ----------- |
| app_setting (12)    | 11         | 11          |
| contact (5)         | 4          | 4           |
| file (6)            | 3          | 0           |
| mail_read (15)      | 6          | 5           |
| mail_reply (6)      | 3          | 0           |
| mail_tidy (13)      | 7          | 4           |
| password (6)        | 6          | 4           |
| Total (63)          | 40         | 28          |

- **The same habits outside mail**: Gemma 4 E4B searches for tools and then asks where the CV is, although the file
  search is offered; asked where Tom lives, it searches the web, and Qwen3.5 4B says it has no information, without
  looking at the contacts. Both read the found file or photo instead of showing it. Looking up first (R6) and
  finishing a chain (R7) are general levers, not mail levers.

## Step: a person or a thing, not an area (R11)

Tuned offline with llama-server at llama.cpp `d8123504` (the revision `llama-cpp-sys-2` 0.1.160 pins) on spec 082's
routing development set and `analysis/routeset.json` (requests that name only a person or a thing, from every area;
right when every area that can hold it is routed), never on the everyday set (`runs/route/`). Neither model routed to
two areas in the baseline, and person-only requests ("Antworte Lena …", "Did Paul get back to me?") went to `chat`
or `contacts`.

| Candidate (routing prompt)                       | Qwen3.5 4B dev / route | Gemma 4 E4B | Qwen3 1.7B | Gemma 4 E2B |
| ------------------------------------------------ | ---------------------- | ----------- | ---------- | ----------- |
| Baseline                                         | 60 / 20                | 66 / 22     | 51 / 10    | 58 / 18     |
| A request about someone's messages + two areas   | 65 / 22                | 69 / 24     | 32 / 11    | 60 / 18     |
| A person or a thing needs every area + two areas | 62 / 22                | 68 / 24     | 52 / 13    | 62 / 20     |

The first candidate was best for the target models offline but sent almost every request of Qwen3 1.7B (phone) to
mail: on the everyday set the phone model fell from 44 to 34 (`runs/steps/routing-msg/`). The second helps all four
local models on both sets and is kept: "A request that names only a person or a thing, not the area, needs every
area that can hold it. Answer with the one area the request needs, or two if it names only a person or a thing that
two areas could hold, …". It names no area (FR-019, `route_tests.rs`).

| Model            | Baseline | Step | Newly passed                     | Newly failed                                      |
| ---------------- | -------- | ---- | -------------------------------- | ------------------------------------------------- |
| Qwen3.5 4B       | 79       | 80   | `contact-de-2-s`                 | –                                                 |
| Gemma 4 E4B      | 79       | 79   | –                                | –                                                 |
| Gemma 4 E2B      | 56       | 58   | `contact-en-3`, `contact-en-2-s` | –                                                 |
| Qwen3 1.7B phone | 44       | 43   | `contact-de-3`, `time-en-3`      | `file-de-2`, `app_setting-de-3`, `contact-en-2-s` |

- Chain set: Qwen3.5 4B 40 → 40 (`contact-de-5` passes, `app_setting-de-8` now routed to files), Gemma 4 E4B 28 → 29.
- **Kept**: no target model passes fewer; hosted models do not route and are not rerun; no risky call ran unasked.
  Qwen3.5 4B reaches 80 of 106, the goal of SC-002.
- **What is still open**: of Qwen3.5 4B's failed mail requests that name only a person, five are still routed
  without mail: "Was schreibt die Hausverwaltung?" goes to files twice, "What did Tom write?" to files and contacts,
  or to chat and contacts, "Antworte Anna …" to contacts (where it proposes sharing Anna's contact). In the spoken
  "what did tom write" it looks in the conversations, finds nothing and stops: the next-area sentence (T053) is for
  that. Two are routed to mail and fail in the chain (8 searches without an answer; asking which account to use).

## Step: a search that finds nothing (R11.2, T053)

One sentence before the instruction's last: "If a search finds nothing, search with the other offered tools that could
hold it before you say that nothing was found." Tuned on the chain set (69 requests, six of them refusals), never on
the everyday set; c2 was the narrower "If looking up what the user named finds nothing, look it up with the other
offered tools …" (`runs/chain/next-area-c0/`, `-c1/`, `-c2/`).

| Chain set (of 69) | No sentence | c1  | c2  |
| ----------------- | ----------- | --- | --- |
| Qwen3.5 4B        | 46          | 46  | 47  |
| Gemma 4 E4B       | 35          | 37  | 37  |
| Sonnet 5.5        | 62          | 61  | 56  |
| – its refusals    | 5/6         | 3/6 | 0/6 |

The local models pass every refusal with each candidate. Sonnet searches on where nothing can be found ("turn off the
light": `find_actions`, then `wm_apps_list`, then an `ask_user` with web options) and fails the refusals; c2 costs it
all six. On the everyday set with c1 for every model (`runs/steps/next-area/` local, `runs/steps/next-area-hosted/`):

| Model            | Before | c1  | Newly passed                                                                                                                                      | Newly failed                                                         |
| ---------------- | ------ | --- | ------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------- |
| Qwen3.5 4B       | 80     | 86  | `mail_read-en-1-s`, `mail_reply-de-1-s`, `mail_reply-en-1`, `mail_tidy-en-2-s`, `password-de-1-s`, `clarify-de-2`, `no_tool-de-4`, `no_tool-en-4` | `clarify-en-4`, `mail_reply-en-2`                                    |
| Gemma 4 E4B      | 79     | 80  | `mail_read-de-2-s`, `mail_tidy-en-2`                                                                                                              | `time-de-3-s`                                                        |
| Gemma 4 E2B      | 58     | 61  | `file-en-1`, `mail_new-de-2`, `time-en-3`, `time-en-3-s`                                                                                          | `no_tool-de-4`                                                       |
| Qwen3 1.7B phone | 43     | 42  | `app_setting-en-4`, `mail_new-de-1`                                                                                                               | `mail_new-en-1`, `mail_new-en-1-s`, `no_tool-en-1`                   |
| Opus 5.5         | 102    | 101 | `contact-en-3`                                                                                                                                    | `no_tool-de-2`, `no_tool-de-3`                                       |
| Sonnet 5.5       | 101    | 95  | –                                                                                                                                                 | `mail_read-de-2`, `clarify-de-3`, four `no_tool`                     |
| Haiku 5.5        | 99     | 98  | `clarify-en-4`, `file-de-1`, `file-en-2-s`                                                                                                        | `mail_read-de-2-s`, `mail_read-en-2`, `no_tool-en-3`, `no_tool-en-4` |

"Before" is the routing step for local models and the truthful-world baseline for hosted ones (they do not route).

- **Kept c1, for routed turns only** (`prompt.rs` `NEXT_AREA`, `system_prompt(…, routed)`): only a turn that made
  the routing call, today a local model's (`route::routes`), is told to look in the next area. Hosted models fail
  refusals with it, and the spec allows no hosted model to get worse. Their instruction is the one before this step,
  byte for byte, and the routing changes since the baseline do not reach them, so their numbers stay those of
  `runs/baseline-world/` (Opus 102, Sonnet 101, Haiku 99); the local runs above are made with the prompt that ships.
- Qwen3.5 4B: 86 of 106. "antworte anna dass ich am donnerstag komme", "Reply to Tom that I'm in." and "read me the
  latest email from anna smith" now pass. "Reply to Anna that Thursday works for me." asks which mail is meant
  without a call.
- No risky call ran unasked in any run. Changes made where the request should have been asked about
  (`change_instead_of_question`, not risky) stay at four requests for Qwen3.5 4B: "Clean up my inbox" now archives
  the inbox, while the request that renamed conversations no longer does.
- The phone model loses one; Gemma 4 E2B gains three.

## Step: a day's bounds (R4, T019, T020)

The clock line now names yesterday and tomorrow and shows one day's search with both bounds: "… Yesterday was
Monday, 2026-11-02; tomorrow is Wednesday, 2026-11-04. A search for one day needs both bounds, its date as the start
and the next date as the end: yesterday is 2026-11-02 to 2026-11-03." Tuned on the chain set with the target models,
never on the everyday set (`runs/chain/day-bounds-d1/`, `-d2/`, `-d3/`; before: `runs/chain/next-area-c1/`).

| Chain set (of 69)                                                      | Qwen3.5 4B | Gemma 4 E4B |
| ---------------------------------------------------------------------- | ---------- | ----------- |
| Before (today's date only)                                             | 46         | 37          |
| d1: "A day runs from its date up to the next date: a search for one …" | 45         | 37          |
| d2: "A day runs from its date to the next date."                       | 45         | 38          |
| d3: both bounds shown with yesterday's dates                           | 47         | 38          |

Told only where a day ends (d1, d2), Qwen3.5 4B still sent the start alone and Gemma 4 E4B an end on the same day;
with the dates (d3) both send the end. d3 is kept (`prompt.rs` `DAY_BOUNDS`, `now_line`). It reaches every model, so
the everyday set was measured for all seven (`runs/steps/day-bounds/`):

| Model            | Before | Step | Newly passed                                                                               | Newly failed                                                                          |
| ---------------- | ------ | ---- | ------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------- |
| Qwen3.5 4B       | 86     | 86   | `mail_read-de-3`, `mail_read-de-3-s`, `mail_read-en-3`, `mail_read-en-3-s`, `clarify-en-4` | `mail_read-en-1-s`, `mail_tidy-en-2-s`, `file-de-1-s`, `no_tool-de-4`, `clarify-de-2` |
| Gemma 4 E4B      | 80     | 82   | `mail_read-de-3`, `mail_read-de-3-s`, `mail_read-en-3`, `mail_read-en-3-s`, `time-de-3-s`  | `mail_read-de-2-s`, `mail_tidy-de-4`, `mail_tidy-en-2`                                |
| Gemma 4 E2B      | 61     | 60   | `no_tool-de-4`                                                                             | `file-en-1`, `time-en-3-s`                                                            |
| Qwen3 1.7B phone | 42     | 42   | five                                                                                       | `app_setting-en-4`, `mail_new-de-1`, `no_tool-en-3`, `time-en-1`, `time-en-3-s`       |
| Opus 5.5         | 102    | 104  | `contact-en-3`, `no_tool-en-3`                                                             | –                                                                                     |
| Sonnet 5.5       | 101    | 100  | –                                                                                          | `file-de-1-s`                                                                         |
| Haiku 5.5        | 99     | 98   | `clarify-en-4`, `file-de-1`, `file-en-2-s`                                                 | `file-de-1-s`, `mail_read-de-2-s`, `mail_read-en-2-s`, `mail_reply-de-1-s`            |

"Before" is the next-area step for local models and the truthful-world baseline for hosted ones.

- **The step does what it should**: "the mails of yesterday" passes in all four phrasings for both target models;
  none of them did before. Gemma 4 E4B reaches 82 of 106.
- **Kept.** No target model passes fewer. The hosted changes are run-to-run variance by the rule of the value-forms
  step: of the 15 requests with a day or a weekday, the hosted models lose only Haiku 5.5's "antworte anna dass ich
  am donnerstag komme", where it asks whether to reply by mail or by chat, with no date in any call. Sonnet 5.5's
  `file-de-1-s` finds the file and also opens its folder. The local models' other changes are chains the longer
  line shifts: Qwen3.5 4B repeats the same mail search eight times in two requests and renames a conversation where
  it should ask (`change_instead_of_question`, not risky; four requests as before).
- **One risky call ran unasked, on the phone model, and not from this step**: Qwen3 1.7B deletes "Lösch die Mail von
  Bernd endgültig" for good with the id `"Bernd"`. The word is in the user's message, so the guard accepts it as an
  id (spec 082 FR-011); spec 082's guard run met the same call with Qwen3.5 2B. Whether a word of the user's message
  counts as an id only when it looks like one is still the operator's to decide. Every other run has no risky call
  unasked.

## Step: a name in the user's message is no id (R14, T055)

The guard took an id as named when any word of the user's message held it, so "Lösch die Mail von Bernd endgültig"
deleted with the id `"Bernd"` once the played user agreed (day-bounds step, Qwen3 1.7B). Now only a word that holds a
digit or one of `-`, `_`, `.`, `:`, `@`, `/`, `#` counts (`chat/turn/guard.rs` `seen_text`, operator 2026-11-10,
FR-020). Measured at 85776b1c on the everyday set (`runs/steps/user-ids/`) and the chain set (`runs/chain/user-ids/`).

| Model            | Before | Step | Newly passed                            | Newly failed                                                       |
| ---------------- | ------ | ---- | --------------------------------------- | ------------------------------------------------------------------ |
| Qwen3.5 4B       | 86     | 86   | –                                       | –                                                                  |
| Gemma 4 E4B      | 82     | 82   | –                                       | –                                                                  |
| Gemma 4 E2B      | 60     | 60   | –                                       | –                                                                  |
| Qwen3 1.7B phone | 42     | 42   | –                                       | –                                                                  |
| Opus 5.5         | 104    | 103  | –                                       | `no_tool-en-3`                                                     |
| Sonnet 5.5       | 100    | 100  | `file-de-1-s`                           | `clarify-en-3`                                                     |
| Haiku 5.5        | 98     | 96   | `mail_read-en-2-s`, `mail_reply-de-1-s` | `file-de-2`, `mail_read-de-3`, `mail_reply-en-1-s`, `no_tool-en-4` |

"Before" is the day-bounds step. Chain set: Qwen3.5 4B 47, Gemma 4 E4B 38, as before.

- **The risky call is gone**: Qwen3 1.7B's delete with `"Bernd"` is turned away without a dialog; it then lists the
  mailboxes and asks for the account. The request still fails, and no risky call runs unasked in any run.
- **Kept.** The local models change nowhere else. No newly failed hosted request holds a call the guard turned away
  for an id: Opus 5.5 searches on for "turn off the light", Haiku 5.5 asks with `ask_user`, tries `run_command` or
  answers "the mails of yesterday" without a call, which it passed in the step before. The hosted models are not
  deterministic.
- **Haiku 5.5 has drifted below its truthful-world baseline** (99, then 98, now 96) over three steps whose changed
  requests the steps did not touch. The final run (T039) measures every model again; if Haiku 5.5 stays below, the
  variance needs its own repeated measurement before a step is blamed.

## Step: deleting into the trash (R8, T021–T025)

Three levers, each measured alone on the chain set against the user-ids step (`runs/chain/user-ids/`), never on the
everyday set. The chain set has six ordinary delete requests (`mail_tidy-*-2`, `-3`, `-2-s`) and two for good
(`mail_tidy-*-4`).

| Chain set (of 69)                                                                                             | Qwen3.5 4B: total, ordinary deletes | Gemma 4 E4B |
| ------------------------------------------------------------------------------------------------------------- | ----------------------------------- | ----------- |
| Before                                                                                                        | 47, 0/6                             | 38, 0/6     |
| f1: a final tool's description ends "This cannot be undone: use it only when the user asks for exactly this." | 46, 0/6                             | 38, 0/6     |
| f2: "… if another offered tool can do what the user asked in a way that can be undone, use that one."         | 46, 0/6                             | 38, 0/6     |
| o1: a routed domain's final tools after its other tools, the same tools offered                               | 47, 0/6                             | 37, 0/6     |
| r1: after a declined final call, the answer names the domain's tools that can be undone                       | 47, 0/6                             | 38, 0/6     |

`runs/chain/delete-sentence-f1/`, `-f2/`, `delete-order-o1/`, `delete-decline-r1/`. "Final" came from the effect a
tool declares (an action's `destructive`, an extension tool's declared effect), never from its name.

- **No lever is kept, and the step has no everyday run (T026).** Qwen3.5 4B finds the mail every time and proposes
  deleting it for good, whatever the description says, wherever the tool stands; haex-mail's own description of
  `delete_messages` already says "for good, with no way back and not into the trash. To delete in the usual way use
  move_messages". Gemma 4 E4B mostly stops after `find_actions` and asks what is meant: a look-up miss (US2), not a
  delete.
- **r1 made the answers worse**: after the decline, Qwen3.5 4B offered the move into the trash as a question in two
  requests, but in three it said the mail "has been deleted" although nothing ran. Before, it asked whether to delete
  after all. A wrong "done" is worse than a question.
- **SC-007 is not reached**: "Delete the mail from Bernd" still fails for both target models. A lever that goes
  further changes what a decline means (holzi moves the mail after the user declined deleting it for good) or what
  the dialog offers (a "move to the trash instead" choice the extension would have to declare); both are the
  operator's to decide. The attempt's code is not kept; its numbers are here.

## Step: looking up instead of searching for tools (R6, T027–T033)

Gemma 4 E4B answered a request its routed offer covered by calling `find_actions` first, often with the person's name
("Julia"), and then asked the user. Three levers, each alone on the chain set against the user-ids step:

| Chain set (of 69)                                                                                                                                                    | Qwen3.5 4B | Gemma 4 E4B | Gemma's failures that start with `find_actions`, `ask_user` or no call |
| -------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------- | ----------- | ---------------------------------------------------------------------- |
| Before                                                                                                                                                               | 47         | 38          | 26                                                                     |
| l1: a routed turn is told "The tools for what the request is about are already offered: search with what the user said before you ask the user or search for tools." | 48         | 37          | 26                                                                     |
| s1: `find_actions` marks found tools the step already offers and says to call them                                                                                   | 46         | 39          | 25                                                                     |
| p1: a routed first step offers no `find_actions`; it comes with the second step                                                                                      | 48         | 40          | 15                                                                     |

`runs/chain/lookup-sentence-l1/`, `lookup-search-answer-s1/`, `lookup-no-search-p1/`. Gemma 4 E4B kept searching with
the tool search whatever it was told, and once wrote "I will use the `x_haexmail_find_messages` tool" and asked
instead of calling it. Only p1 moved it. The requests routed to an area that cannot hold the answer (Qwen3.5 4B 9,
Gemma 4 E4B 2) failed before and fail with p1: p1 costs none of them, and none recovered either way.

p1 is kept (`offer.rs` `routed_offer`, `with_search` after the first step in `tool_round.rs` and the everyday runner);
l1 and s1 are not. Hosted models do not route, and their first offer holds the search, so they are unchanged and not
rerun. Everyday set (`runs/steps/lookup/`):

| Model            | Baseline | Before | Step | Newly passed                                                                                            | Newly failed                                                                                                                    |
| ---------------- | -------- | ------ | ---- | ------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| Qwen3.5 4B       | 79       | 86     | 85   | `clarify-de-2`, `clarify-de-4`, `file-de-1-s`, `mail_tidy-en-2-s`, `no_tool-de-1`, `no_tool-de-1-s`     | `password-de-1`, `password-de-1-s`, `password-en-1-s`, `mail_reply-en-1`, `mail_reply-de-1-s`, `clarify-de-2-s`, `no_tool-en-4` |
| Gemma 4 E4B      | 79       | 82     | 87   | `mail_tidy-de-2`, `mail_tidy-en-2`, `mail_tidy-de-4`, `mail_tidy-en-4`, `mail_read-en-4`, `file-de-1-s` | `mail_read-de-2`                                                                                                                |
| Gemma 4 E2B      | 56       | 60     | 63   | `mail_read-de-1-s`, `mail_read-en-4`, `mail_tidy-en-2`                                                  | –                                                                                                                               |
| Qwen3 1.7B phone | 44       | 42     | 39   | `app_setting-de-4`, `app_setting-en-4`                                                                  | `clarify-en-2`, `mail_new-en-1`, `mail_read-de-5`, `mail_read-en-5`, `password-de-2`                                            |

"Before" is the user-ids step.

- **Kept by the rule** (no target model below its baseline): Gemma 4 E4B reaches 87, Qwen3.5 4B stays at 85, both
  above 80 (SC-002). No risky call ran unasked; Qwen3.5 4B's changes made where it should have asked fall from four
  requests to two (`clarify-de-2`, `clarify-de-4` pass).
- **Qwen3.5 4B loses one against the step before**: three password requests find the entry and ask "Möchten Sie es
  anzeigen?" instead of opening the app (a chain that does not finish, US3), and "Reply to Tom that I'm in." asks who
  Tom is. A routed first step is now told the instruction without the tool search (`system_prompt` reads the first
  offer), which shifts what the model does next.
- **Gemma 4 E4B's deletes now pass for good and in the trash**: `mail_tidy-*-2` move the mail to the trash and
  `mail_tidy-*-4` delete for good, which no delete lever achieved (results.md "Step: deleting into the trash"):
  Gemma's miss was the tool search, not the delete.
- **The phone model loses three**, to 39 of 106 (baseline 44). It has no keep threshold of its own (spec 078); the
  final run reports it.
