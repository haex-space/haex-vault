# Research: Small local models finish what they start

All figures are from the reports of spec 082 under the app's names (`specs/082-agent-harness/runs/names/haex-mail/`),
2026-11-06, Qwen3.5 4B 78/106 and Gemma 4 E4B 77/106. The goal (SC-002) is 80 each: Qwen3.5 4B needs 2 more
requests, Gemma 4 E4B 3.

## R1 Where the target models fail, by lever

Each failed request is counted once by its first cause (`analysis/failures.py` of spec 082), then sorted by what could
change it:

| Lever                                                        | Qwen3.5 4B | Gemma 4 E4B | Requests (examples)                                   |
| ------------------------------------------------------------ | ---------- | ----------- | ----------------------------------------------------- |
| Ask, search for tools or use another area's tools            | 7          | 14          | `mail_reply-de-1` (asks which mail), `file-de-1`      |
| Deleting a mail: delete for good proposed, or no tool        | 4          | 4           | `mail_tidy-*-1` ("Delete the mail from Bernd")        |
| A day without its end ("yesterday" sent as `since` only)     | 4          | 2           | `mail_read-*-3`                                       |
| The chain stops after a successful search, other than delete | 1          | 6           | `password-*-1` ("Shall I show it?"), `mail_tidy-de-1` |
| A value in the wrong form (`"read": "True"`)                 | 2          | 0           | `mail_tidy-*-3`                                       |
| A call where a question, a refusal or an answer was expected | 8          | 3           | `no_tool-*`, `clarify-*`; not this feature's target   |
| Other (wrong tool, a reply claimed but not drafted)          | 2          | 0           | `contact-de-2-s`, `mail_reply-en-1-s`                 |

- **Decision**: the steps follow the levers, largest first per model; each step names the requests it targets and is
  measured on all of them, so that a step that helps its targets but costs elsewhere shows.
- **Rationale**: half of Gemma 4 E4B's failures are in the first row; Qwen3.5 4B's spread over the first row (three of
  its seven use the chat or files tools for a mail request, one the window tools), deleting and the day's end. No single lever reaches the
  goal for both models.
- **Alternatives considered**: one combined change (instruction rewrite) measured once; rejected, the operator's rule
  is one step at a time, and a combined change cannot tell which part costs the hosted models.

## R2 The report must say where each request was routed

- **Finding**: the contract of spec 082 (`contracts/everyday-report.md`) promises a per-request field `routed`, but the
  report does not carry it; only the total `routingHits` is written. Whether Gemma 4 E4B's tool searches happen
  with the mail tools offered or after a routing miss cannot be told from the reports.
- **Decision**: the report writes per request (failed and passed) the routed domains and the names of the first
  step's offered tools; the baseline is taken after this change.
- **Rationale**: the first lever (R1) depends on it: a routing miss calls for a different remedy than a model that
  ignores offered tools.
- **Alternatives considered**: rerunning the routing call offline for the failed requests; rejected, it would not be
  the run that failed.

## R3 A development set for chains

- **Finding**: `analysis/devset.json` of spec 082 holds 72 (area, sentence) pairs and only checks routing. Wording
  for finishing chains, looking up and deleting cannot be tuned on it.
- **Decision**: a chain development set `analysis/chainset.json` in the everyday set's format (requests with expected
  chains, played answers, safety), on the same fixture world, about 36 requests (German and English, clean and
  spoken). It uses what the everyday set does not ask about, or asks about differently: the mails of Julia Wagner,
  IT-Support, Ingrid Berger and Stadtwerke, the passwords Amazon, GitHub, WLAN and VPN, the files Mietvertrag and
  Lebenslauf, deleting and moving other mails, a day other than yesterday. The everyday runner takes it through a new
  `HAEX_EVAL_SET=<path>`; its report has the same form.
- **Rationale**: it measures the real harness (offer, guard, dialog, scoring), not an approximation, at about a third
  of a full run's time, and keeps tuning off the everyday set (FR-012).
- **Alternatives considered**: a Python probe against llama-server as `route_names.py`; rejected for chains, it
  would have to rebuild the offer, the guard and the played dialog. New fixture data; rejected, the world already has
  unused mails, passwords and files, and a second world would need its own checks.

## R4 The day's end (`since` without `before`)

- **Finding**: the clock line tells the model today's date. "The mails of yesterday" is sent as `since` yesterday
  without `before` today in 6 failures; the tool's schema describes both bounds.
- **Decision**: the clock line also names yesterday's and tomorrow's dates and says that a day runs from its date to
  the next one. Measured on the chain set (a day other than yesterday) first.
- **Rationale**: the models get dates right; they miss the end. A line holzi writes holds for every extension with a
  date range.
- **Alternatives considered**: filling in `before` when only `since` is sent; rejected, "since Monday" means no end,
  and holzi cannot tell the two apart. Changing haex-mail's description; rejected, the fixture stays pinned.

## R5 Values in the wrong form (FR-007 to FR-009)

- **Decision**: before the guard, holzi turns a value into the type the tool's schema declares when the meaning is
  unique: `"true"`/`"false"` (any case) into a yes/no, a decimal number written as text into a number. Anything
  else stays as sent, and the tool's own check answers. The dialog and the stored call show the turned value.
- **Rationale**: deterministic, cheap, testable without a model; it helps every model with the same habit.
- **Alternatives considered**: telling the model in a failure message (spec 082 next-step sentences); in the
  measurement these calls ran and no message came back (the fake world accepts them), and whether haex-mail checks
  the type itself is not known. Turning the value does not depend on that.

## R6 Looking up instead of asking, and offered tools before the tool search (FR-003, FR-004)

- **Candidates**, each measured on the chain set with both target models:
  1. **Instruction**: one sentence after the existing look-up sentence: the tools of the area the request is about
     are already offered; search with what the user said before asking.
  2. **The tool search answers with offered tools**: when `find_actions` finds tools that the step already offers, its
     result names them as already offered and says to call them.
  3. **No tool search in a routed first step**: a routed step offers the area's tools, `ask_user` and the core, and
     the tool search only from the second step.
- **Decision**: measure 1 and 2 first (both keep every way the model has today); 3 only if neither moves Gemma 4 E4B,
  since a routing miss would then cost a step. Keep what the chain set shows; then one everyday run.
  A kept candidate 3 needs a recovery path for a request routed to the wrong area: the first offer then lacks
  both the needed tool and `find_actions`, and a plain answer would end the turn. Keep it only if the measurement
  shows routing misses recover (for instance by offering `find_actions` in the first step of an unrouted request).
- **Rationale**: R2 decides which of these can work: if the failing requests were routed to mail, 1 and 2 address the
  model; if they were not, the routing description is the lever, tuned on the routing development set.

## R11 Routing misses of Qwen3.5 4B (added after the baseline, operator 2026-11-09)

- **Finding** (baseline, `requests[].routed`): 7 of Qwen3.5 4B's 29 failures are routing misses. "Was schreibt die
  Hausverwaltung?" and "What did Tom write?" go to files, "Antworte Anna …" to chat, one spoken mail request to no
  area. Gemma 4 E4B routes the same requests right.
- **Decision**: a step of its own after the look-up step: tune holzi's domain descriptions and the routing prompt
  on spec 082's routing development set, extended by mail requests that name no mail word (in a file of this
  feature, never in the everyday set); measured offline with llama-server, then once on the everyday set.
- **Rationale**: the look-up levers cannot help a request whose first offer has no mail tool.
- **Alternatives considered**: routing to two areas more often (the prompt allows two); measured as one candidate
  of the step, since a second area widens the offer.

## R7 Finishing a chain (FR-005, FR-006)

- **Candidates**:
  1. **Instruction**: when you have found what the user asked about, do what was asked (show, answer, move) without
     asking whether to.
  2. **A next step after a successful result**: the result of a reading call that found exactly what was looked up
     ends with one sentence that names what the request may need next, from the step's offered tools.
- **Decision**: measure 1 first, then 2 if needed; questions back stay right where the set expects them (the
  `clarify` category is watched for each step).
- **Rationale**: 1 costs nothing to try; 2 touches every tool result and needs its own care.

## R8 Deleting into the trash (FR-010, FR-011)

- **Finding**: the routed offer ranks a domain's tools by the words of the message; "Lösch" ranks `delete_messages`
  right after the reading tools, the move tool further down. The tool deletes for good and is risky, so the dialog
  stops it, and the played user declines.
- **Candidates**:
  1. **Order**: in a routed offer, a tool whose effect is final comes after the tools of the same domain that change
     without deleting.
  2. **A sentence holzi adds to every final tool**: it cannot be undone; use it only when the user asks for a final
     delete.
  3. **After a declined final call**: the answer says that a reversible way (moving to the trash) may be what was
     meant, if the step offers one.
- **Decision**: measure 2, then 1, on the chain set; 3 only if both fail. All three use the effect the extension
  declares, so they hold for any mail extension (FR-011), checked with the extension under another name.
- **Alternatives considered**: matching "trash"/"Papierkorb" in tool descriptions; rejected, it names a particular
  wording of one extension.

## R9 Which models per step

- **Decision**:
  - Chain set: both target models for every candidate.
  - Everyday set: both target models for every kept step; Gemma 4 E2B and Qwen3 1.7B (phone) for every kept step.
  - Hosted models (Opus, Sonnet, Haiku 5.5): baseline, every kept step that changes what every model reads (R4, R5,
    R6.1, R7, R8.2/8.3), and the final run. Steps only inside a routed offer (R6.3, R8.1) do not reach hosted
    models, which do not route.
  - The final run: all models of the baseline, the target models also under another extension name (FR-015).
- **Rationale**: one target-model everyday run takes 10 to 16 minutes, the chain set about a third; hosted runs cost
  subscription time.

## R10 File sizes and places

- `chat/eval/everyday/runner.rs` (473 lines) gets the per-request routing record from `report.rs`, not a larger
  loop; `tools/offer.rs` (405) takes the order rule in a new module next to it if it passes 450.
- The value turning (R5) is a new module `chat/turn/arg_forms.rs`, called from `plan_calls.rs` (156).
