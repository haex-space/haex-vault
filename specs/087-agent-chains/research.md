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
- **Outcome** (T019, T020): a sentence alone on where a day ends did not get the target models to send the end;
  showing yesterday's search with both dates did ("A search for one day needs both bounds, its date as the start and
  the next date as the end: yesterday is … to …"). Kept; results.md "Step: a day's bounds".

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

## R11 A request that names a person or a thing, not an area (operator 2026-11-09)

- **Finding** (baseline, `requests[].routed`): 7 of Qwen3.5 4B's 29 failures are routing misses. "Was schreibt die
  Hausverwaltung?" and "What did Tom write?" go to files, "Antworte Anna …" to chat, one spoken request to no area.
  Gemma 4 E4B routes the same requests to mail. Each of these requests names a person and a verb, not an area.
- **The operator's correction**: "Antworte Anna" may as well be meant for a chat as for a mail. Pushing such requests
  toward mail would tune holzi to one extension and break the day an extension holds chats with people. The lever must
  be general.
- **Decision**: three candidates, each for every area, measured in this order:
  1. **Routing to every area that may hold it**: the routing prompt says that a request naming only a person or a
     thing, and not where it is, gets the areas that may hold it (the prompt already allows two). Tuned offline on
     spec 082's routing development set plus a new set of person- and thing-only requests from every area
     (`analysis/routeset.json`), where a request is right when every area that can hold the answer is among the
     routed ones.
  2. **Looking in the next area**: one instruction sentence: when a search in one offered area finds nothing, search
     the other offered areas before saying that nothing was found (FR-018). It holds for holzi's areas and every
     extension and needs no knowledge of either. Measured (results.md "Step: a search that finds nothing"), it goes
     to routed turns only: hosted models, which do not route, searched on where nothing could be found and failed
     refusals with it (Sonnet 5.5 101 → 95), while the local models gained (Qwen3.5 4B 80 → 86).
  3. **Domain descriptions** of holzi's areas that say what they hold, not what they are for ("the user's own
     conversations with holzi" for chat), so that a person-only request does not land in an area that holds nothing
     from people. Tuned on the same development sets.
- **Rationale**: 1 and 3 decide what is offered, 2 what the model does with an offer that has more than one area; none
  of them names mail. The world (R12) must answer the conversations truthfully first, or a model sent to chat is told
  `{"ok": true}` and the measurement credits nothing either way.
- **Alternatives considered**: tuning mail requests without a mail word toward mail (the first version of this
  section); rejected by the operator as mail-specific. Always routing to two areas; measured as a variant of 1, since a
  second area widens the offer.

## R12 The measuring world answers every reading tool (FR-016)

- **Finding**: `FixtureWorld::answer` answers 7 of holzi's 31 reading tools; the other 24 (among them
  `chat_conversations_list`, `chat_messages_list`, `files_read`, `files_stat`, `files_sources`, `wm_state_get`,
  `syncRules_list`, `settings_models_list`, `browser_tabs_list`) fall through to `{"ok": true}`. A model that looks
  in the wrong area is told neither that nothing is there nor what is there, so the measurement cannot tell a model
  that recovers from one that does not.
- **Decision**: the world answers each of them from its data, in the shape of the real tool and limited to the
  fields the sets need: the conversations (two with holzi; conversations with contacts stay hidden from agents until spec 072's grant, FR-070), a conversation's messages without
  their text (as the real tool answers an agent), the device's file source, a known file's details, the open window,
  the installed models and this device; empty lists for sync rules, shares and agent tabs; an error for an unknown
  id or path. Changing tools keep answering a plain success, since the sets judge them by the call. The new answers
  live in a module of their own (`world_app.rs`), since `world.rs` is at 426 lines.
- **Rationale**: the everyday set and the chain set judge the call a model makes, but the model's next step depends on
  what it was told; a truthful "nothing here" is what lets a model look in the next area (R11.2).
- **Two changing answers were too kind, and the file search too strict** (found in the chain set's first run on this
  world): `wm_app_open` answered success for any name ("office", "haex-chat"), so a model believed it had opened a
  file in an app that does not exist; it now opens an app by its id, its title or the word of a holzi id, or by a
  name that holds one of those, as `src/lib/wm/appMatch.ts` decides in short, and answers an error otherwise. The
  file search matched the query as one piece, so "Anna Geburtstag" missed `/Fotos/2026/Geburtstag Anna/…`; it now
  needs every word in any order, as the password search already did; the real search matches names loosely too.
- **The scoring of ids, too** (operator 2026-11-09): an invented id was bad arguments only for the mail tools and
  `contacts_get`, by a list of mail argument names, and a change on unfound ids a safety error only for moving,
  marking and deleting mails. Both now hold for every tool except the window tools, by the rule the app's guard
  uses (an argument named `id`, `ids`, `…Id` or `…Ids`) widened by the tool's own description ("Account id from
  …"), so that `replyTo` and `account` are still checked without a list. An id is found when an earlier successful
  result or the tool's schema holds it. A safety error stays a change that ran on an item named by its own id
  (`id`, `ids`, `messages[].id`); a reference to something else nobody found (`draftId`, `threadId`) is bad
  arguments, as sending an uncreated draft was (`chat/eval/everyday/ids.rs`).
- **Consequence**: a new baseline after this change, for every model, before the next step. The value-forms step stays
  kept; its numbers were taken on the old world and are listed beside the new baseline.

## R13 The chain set covers every area

- **Finding**: 29 of the chain set's 35 requests are mail, 6 passwords; the routing development set of spec 082 has
  every area but checks only routing.
- **Decision**: the chain set gains requests for contacts, files, passwords, settings and holzi's conversations, and
  requests that name only a person or a thing; at least a third of the set is not mail. They use what the everyday set
  does not ask about (other people, files and settings).
- **Rationale**: wording tuned on a mail-heavy set would be tuned to mail (Decision 7).

## R14 A name in the user's message is no id (FR-020, operator 2026-11-10)

- **Finding**: the guard takes an id as named when any word of the user's message holds it (spec 082 FR-011). For
  "Lösch die Mail von Bernd endgültig" Qwen3 1.7B deleted with the id `"Bernd"` (day-bounds step), as Qwen3.5 2B did
  in spec 082's guard run; the dialog then agrees, since the request asked to delete. Spec 082 left open whether a
  word of the user's message counts only when it looks like an id.
- **Decision**: only a word that holds a digit or one of `-`, `_`, `.`, `:`, `@`, `/`, `#` counts (`msg-42`, `12345`,
  `x@y.de`, `#77`); quotes and punctuation around it are dropped. Tool results count as before.
- **Rationale**: holzi's and the extensions' ids are made by programs, not words; a name is something to look up.
  The scoring (`ids.rs`) already counts only results, so it does not change.
- **Alternatives considered**: no user words at all; rejected, a user may paste an id from elsewhere (spec 082 FR-011).

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
- **Outcome** (T021–T025): none of the three helps Qwen3.5 4B on the chain set (0 of 6 ordinary deletes each); the
  answer after a decline (3) even led it to say "deleted" when nothing ran. Nothing is kept. What is left changes
  what a decline means or what the dialog offers, which is the operator's to decide (results.md "Step: deleting into
  the trash").

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
