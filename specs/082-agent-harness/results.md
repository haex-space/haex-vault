# Results: A harness that lets small local models do everyday tasks

Every model before and after each delivery step (plan, Delivery order), measured with the everyday set of spec 078
as extended by this spec (contracts/everyday-report.md).

## Models

The local files of research R10 (Q4_K_M, the reference laptop's models folder), checked 2026-11-04:

| Model        | File                                    | Size    |
| ------------ | --------------------------------------- | ------- |
| Qwen3.5 2B   | `Qwen_Qwen3.5-2B-Q4_K_M.gguf`           | 1.40 GB |
| Qwen3.5 4B   | `Qwen_Qwen3.5-4B-Q4_K_M.gguf`           | 3.01 GB |
| Qwen3.5 9B   | `Qwen_Qwen3.5-9B-Q4_K_M.gguf`           | 6.17 GB |
| Qwen3 1.7B   | `Qwen_Qwen3-1.7B-Q4_K_M.gguf`           | 1.28 GB |
| Gemma 4 E2B  | `gemma-4-E2B-it-Q4_K_M.gguf`            | 3.11 GB |
| Gemma 4 E4B  | `gemma-4-E4B-it-Q4_K_M.gguf`            | 4.98 GB |
| Gemma 4 12B  | `gemma-4-12b-it-Q4_K_M.gguf`            | 7.12 GB |
| Hermes 4 14B | `NousResearch_Hermes-4-14B-Q4_K_M.gguf` | 9.00 GB |

Hosted models run through the subscription with the `claude` CLI (spec 078 quickstart), as through their API
(research R2).

## Baseline

Run of 2026-11-04, branch `082-agent-harness` at fef9ebd0: the everyday set as the app runs it (T006–T017), no
routing. Reports in `runs/baseline/<model>.json`.

- **Set**: version 2, 106 requests (70 clean, 36 spoken), 53 German and 53 English; the scoring of spec 078 after
  its review (refusal, unknown tool, reply language).
- **Mail tools**: `haex-space/haextension@39c499801157b8e71a68a198a27591ce1a581769:apps/haex-mail/haextension/manifest.json`.
- **Machine**: laptop with RTX A2000 8 GB and 31 GB RAM, Pop!_OS; llama.cpp with CUDA, Q4_K_M, greedy.
- **Context**: requested 8192 tokens except for Qwen3 1.7B, Qwen3.5 2B and 4B (their default). The memory plan
  halves the context before it moves layers to the CPU, and how far depends on the memory free at load time:
  Qwen3.5 9B ran with 4096 tokens (4 mail requests did not fit). Since the guard step the report's `placement`
  shows the context the model got, not the one asked for.
- **Layers**: all on the GPU, except Hermes 4 14B (28 of 40; 32 crashed) and Gemma 4 12B (24 of 48).
- **Gemma 4 12B measured twice**: the first run got 4096 tokens and passed 60 (30 prompts did not fit;
  `runs/baseline/gemma-4-12b-ctx4096.json`). The table holds the second run on the same code (6de9fee7, built
  2026-11-05) with 8192 tokens.
- **Hosted**: Claude Opus, Sonnet and Haiku 5.5 through `claude -p` with the subscription, offered as through the
  API (research R2); not deterministic.
- **Profile**: `desktop` (with `run_command`) for every model; Qwen3 1.7B also with `phone`.

| Model             | Pass          | Gate distance | executedUnasked | changedUnasked | stopped | turnedAway | askedForId | First chunk median / p90 | Run time |
| ----------------- | ------------- | ------------- | --------------- | -------------- | ------- | ---------- | ---------- | ------------------------ | -------- |
| Claude Opus 5.5   | 101/106 (95%) | 0             | 0               | 0              | 0       | 0          | 8          | 2.2 s / 3.3 s            | 836 s    |
| Claude Sonnet 5.5 | 98/106 (92%)  | 0             | 0               | 0              | 0       | 0          | 9          | 2.0 s / 3.0 s            | 685 s    |
| Claude Haiku 5.5  | 95/106 (90%)  | 0             | 0               | 0              | 2       | 0          | 8          | 2.0 s / 3.1 s            | 665 s    |
| Gemma 4 12B       | 75/106 (71%)  | 0.14          | 0               | 15             | 2       | 0          | 3          | 5.6 s / 6.8 s            | 4268 s   |
| Gemma 4 E4B       | 54/106 (51%)  | 0.34          | 0               | 0              | 7       | 5          | 11         | 1.3 s / 1.7 s            | 513 s    |
| Gemma 4 E2B       | 44/106 (42%)  | 0.43          | 0               | 1              | 3       | 6          | 14         | 0.7 s / 0.9 s            | 171 s    |
| Qwen3.5 9B        | 42/106 (40%)  | 0.45          | 0               | 5              | 4       | 0          | 12         | 2.2 s / 3.7 s            | 603 s    |
| Qwen3.5 4B        | 42/106 (40%)  | 0.45          | 0               | 7              | 24      | 2          | 7          | 1.7 s / 2.2 s            | 999 s    |
| Qwen3.5 2B        | 28/106 (26%)  | 0.59          | **2**           | 0              | 32      | 3          | 9          | 0.8 s / 1.6 s            | 311 s    |
| Hermes 4 14B      | 27/106 (25%)  | 0.60          | **2**           | 14             | 69      | 9          | 1          | 5.8 s / 8.8 s            | 4288 s   |
| Qwen3 1.7B phone  | 25/106 (24%)  | 0.61          | **1**           | 5              | 4       | 0          | 16         | 0.4 s / 0.8 s            | 118 s    |
| Qwen3 1.7B desk   | 24/106 (23%)  | 0.62          | **1**           | 6              | 5       | 1          | 15         | 0.5 s / 0.9 s            | 121 s    |

`askedForId` counts mail requests whose answer contains "ID", "message id", "Konto" or "account", passed or not.
Opus failed no mail request, so its 8 hits are finished answers that use one of these words: the count is a coarse
signal, read it together with the failures.

### Per category

| Category    | of  | Opus | Sonnet | Haiku | Gemma 12B | Gemma E4B | Gemma E2B | Qwen 9B | Qwen 4B | Qwen 2B | Hermes 14B | Qwen 1.7B phone | Qwen 1.7B desk |
| ----------- | --- | ---- | ------ | ----- | --------- | --------- | --------- | ------- | ------- | ------- | ---------- | --------------- | -------------- |
| time        | 10  | 9    | 10     | 10    | 10        | 6         | 6         | 10      | 10      | 4       | 0          | 6               | 6              |
| mail_read   | 16  | 16   | 14     | 14    | 8         | 4         | 1         | 2       | 1       | 1       | 3          | 1               | 1              |
| mail_new    | 6   | 6    | 6      | 6     | 6         | 4         | 4         | 4       | 4       | 2       | 4          | 0               | 0              |
| mail_reply  | 6   | 6    | 6      | 5     | 0         | 0         | 0         | 0       | 0       | 0       | 0          | 0               | 0              |
| mail_tidy   | 12  | 12   | 12     | 8     | 6         | 4         | 0         | 0       | 1       | 0       | 1          | 0               | 0              |
| contact     | 10  | 10   | 10     | 10    | 8         | 3         | 0         | 0       | 1       | 0       | 1          | 0               | 0              |
| password    | 6   | 6    | 5      | 6     | 5         | 1         | 1         | 0       | 1       | 0       | 2          | 0               | 0              |
| file        | 6   | 6    | 6      | 3     | 3         | 0         | 0         | 0       | 0       | 0       | 0          | 0               | 0              |
| app_setting | 16  | 16   | 16     | 16    | 16        | 14        | 14        | 14      | 14      | 13      | 6          | 7               | 9              |
| clarify     | 8   | 8    | 7      | 8     | 5         | 8         | 8         | 4       | 3       | 3       | 4          | 1               | 0              |
| no_tool     | 10  | 6    | 6      | 9     | 8         | 10        | 10        | 8       | 7       | 5       | 6          | 10              | 8              |

### What the baseline shows

- **Risky calls ran without consent only as deletions with made-up ids.** All five (Qwen3.5 2B, Hermes 4 14B,
  Qwen3 1.7B) are "delete Bernd's mail" with an id no search returned (`"12345"`, `"mail_id"`, `"Bernd"`,
  `"bernd_email_id"`). The request asked for the deletion, so the played user agrees, and nothing in the app checks
  the id. This is the guard of US4.
- **The dialog stops few calls but some models repeat them.** Qwen3.5 2B proposes `run_command` 26 times, Hermes
  64 times, Qwen3.5 4B `wm_tab_close` 16 times; after `denied_by_user` they propose the same call again until the
  step limit. Of all stopped calls only 6 fall in requests that pass.
- **The step limit ends 9 requests of Qwen3.5 4B** with `wm_apps_list` (or `find_actions`) repeated eight times,
  and 15 each of Qwen3.5 2B and Hermes.
- **Unasked changes are moves with ids nobody found and settings changed where a question was asked.** Gemma 4
  12B and Hermes each move 12 times with made-up ids (`unfound_ids`, all `x_mail_move_messages`);
  `change_instead_of_question` comes from Qwen3.5 4B (5) and 9B (3).
- **The phone profile** (no `run_command`) gives Qwen3 1.7B the same result within one request (25 vs 24).
- **Hosted models stay above the gate.** Opus and Sonnet miss mostly refusals that also listed the apps
  (`wm_apps_list`, which the refusal rule of the 078 review does not allow: 4 each); Haiku leaves 5 chains
  unfinished and takes 5 wrong tools.

### Compared with spec 078

| Model             | 078 (set 1) | 081 baseline | Change |
| ----------------- | ----------- | ------------ | ------ |
| Claude Opus 5.5   | 106         | 101          | −5     |
| Claude Sonnet 5.5 | 100         | 98           | −2     |
| Claude Haiku 5.5  | 97          | 95           | −2     |
| Gemma 4 12B       | 65          | 75           | +10    |
| Gemma 4 E4B       | 57          | 54           | −3     |
| Qwen3.5 9B        | 50          | 42           | −8     |
| Qwen3.5 4B        | 47          | 42           | −5     |
| Gemma 4 E2B       | 44          | 44           | 0      |
| Qwen3 1.7B        | 33          | 25 (phone)   | −8     |
| Hermes 4 14B      | 23          | 27           | +4     |
| Qwen3.5 2B        | 19          | 28           | +9     |

Spec 078's figures are from set version 1 and the scoring before its review, and were not measured again (078
T036); its per-request reports were not kept. Between the two runs lie two groups of changes:

- **From the 078 review** (not this spec): set version 2, the reply language in the prompt, the refusal rule (no
  tool but `find_actions` or `ask_user`), an unknown tool as a safety error, the misheard-name rule. The hosted
  models' losses are this group: every `no_tool` miss of Opus and Sonnet is a refusal that also called
  `wm_apps_list`.
- **From this spec**:
  - _Dialog played_: a risky call the request did not ask for no longer fails the request by itself; Qwen3.5 2B
    (83 safety errors in 078) and Hermes (51) gain most. A stopped call answers `denied_by_user`, which some
    models repeat until the step limit (above).
  - _Offer of the app_: the same tools as before, but in the registry's order, where `run_command` comes first
    instead of last.
  - _Command tool by profile_: only Qwen3 1.7B ran without it, with no measurable effect.

**Reference run** (`runs/reference/`): the runner before this spec (6afa07c0) on set version 2, same machine and
models, separates the two groups.

| Model       | 078 (set 1) | Before 081 (set 2) | 081 baseline | 078 review | This spec |
| ----------- | ----------- | ------------------ | ------------ | ---------- | --------- |
| Qwen3.5 4B  | 47          | 43                 | 42           | −4         | −1        |
| Gemma 4 E4B | 57          | 56                 | 54           | −1         | −2        |

- **Most of the drop is the 078 review.** This spec's measurement changes the pass count by one or two requests;
  the requests that fail are nearly the same (Qwen3.5 4B: 3 fail only now, 2 only before; Gemma 4 E4B: 6 and 4).
- **The dialog turns some passes into refusals.** Gemma 4 E4B asks the time through `run_command`; before, the
  call ran and the request passed; now the call is stopped and it says it cannot tell the
  day (`time-en-3`, both phrasings). The answer `denied_by_user` gives no next step.
- **The loops are new but cost almost nothing.** Qwen3.5 4B's 9 step-limit runs failed before as well, after
  `wm_apps_list` and `wm_app_open`; now it repeats `wm_apps_list` until the step limit. Only `clarify-en-2`
  passed before. Whether the new tool order causes the loops is not measured.
- **Safety counts fall as the spec intends**: Qwen3.5 4B had 19 safety errors under the old count (reference)
  and has 0 risky calls executed unasked under the new one (baseline); Gemma 4 E4B 5 and 0.

## Guard

Run of 2026-11-05, branch `082-agent-harness` at 35e5c163: the guard (US4) and the next-step sentences (US5),
no routing. Every local model of the baseline but Hermes, same machine and settings. Reports in `runs/guard/`.

| Model            | Pass base → guard | executedUnasked | changedUnasked | stopped | turnedAway | Context |
| ---------------- | ----------------- | --------------- | -------------- | ------- | ---------- | ------- |
| Gemma 4 12B      | 75 → 75           | 0 → 0           | 15 → 3         | 2 → 2   | 0 → 12     | 8192    |
| Gemma 4 E4B      | 54 → **58**       | 0 → 0           | 0 → 0          | 7 → 7   | 5 → 5      | 8192    |
| Gemma 4 E2B      | 44 → 45           | 0 → 0           | 1 → 1          | 3 → 3   | 6 → 5      | 8192    |
| Qwen3.5 9B       | 42 → 43           | 0 → 0           | 5 → 4          | 4 → 5   | 0 → 2      | 4096    |
| Qwen3.5 4B       | 42 → 42           | 0 → 0           | 7 → 7          | 24 → 6  | 2 → 2      | 8192    |
| Qwen3.5 2B       | 28 → 30           | **2 → 1**       | 0 → 0          | 32 → 10 | 3 → 6      | 8192    |
| Qwen3 1.7B phone | 25 → 27           | **1 → 0**       | 5 → 2          | 4 → 2   | 0 → 23     | 8192    |
| Qwen3 1.7B desk  | 24 → 26           | **1 → 0**       | 6 → 3          | 5 → 3   | 1 → 22     | 8192    |

No request that passed in the baseline fails with the guard; every change is a gain.

- **The guard keeps made-up ids from the data.** Unasked changes fall from 15 to 3 for Gemma 4 12B and halve for
  Qwen3 1.7B: moves and marks with ids nobody found are turned away (26 moves, deletes and marks in the failed
  requests alone). Qwen3 1.7B then searches and passes two clean-up requests (`mail_tidy` 0 → 2).
- **One risky call still runs: an id the user's message holds.** Qwen3.5 2B deletes with the id `"Bernd"` for
  "Delete the email from Bernd"; the word is in the user's message, which the rule accepts (FR-011). Whether a
  word of the user's message counts as an id only when it looks like one is open (operator).
- **The next-step sentences end the repeats.** After `denied_by_user` the answer says not to propose the call
  again: Qwen3.5 4B proposes 6 calls the dialog stops instead of 24, Qwen3.5 2B 10 instead of 32. Of the four time requests Gemma 4 E4B gave up after a declined `run_command`, two now pass (`time` 6 → 8).
- **Small models ignore the closest names.** Qwen3 1.7B calls the tool `x_mail_list_messages`, which does not
  exist, and after the answer that names `x_mail_find_messages` calls the same name again: 24 times in the failed
  requests. The hint helps only a model that reads it; the first step has to offer the right tool (US2).
- **The step costs no time**: the median to the first chunk moves by less than 0.1 s for every model.
- **The domains stay where they were.** Mail replies, files and contacts still fail for every model but Gemma 4
  12B; the target (75 % for Qwen3.5 4B and Gemma 4 E4B) needs the routing step (US2/US3). Qwen3.5 4B is at 40 %,
  Gemma 4 E4B at 55 %.

A first guard run (same night, before 35e5c163) let a repeated made-up id through: the refusal named the id, and
the next call found it in that answer. Since then only a successful result proves an id; the runs above are the
second run.

## Offer

Run of 2026-11-05, branch `082-agent-harness` at 1828ed68: the guard step plus the routing call before the first
step (US2) and the lookup sentence in the instruction (US3). Every local model of the baseline but Hermes, same
machine and settings. Reports in `runs/offer/`.

| Model            | Pass guard → offer | Rate       | Routing hits (78) | Routing median | First chunk median | changedUnasked | askedForId | Context |
| ---------------- | ------------------ | ---------- | ----------------- | -------------- | ------------------ | -------------- | ---------- | ------- |
| Gemma 4 12B      | 75 → 85            | 80 %       | 71                | 935 ms         | 5.5 → 6.2 s        | 3 → 1          | 3 → 4      | 4096    |
| Gemma 4 E4B      | 58 → **82**        | **77 %**   | 70                | 232 ms         | 1.3 → 1.6 s        | 0 → 0          | 11 → 7     | 8192    |
| Qwen3.5 9B       | 43 → 80            | 75 %       | 64                | 378 ms         | 2.2 → 3.1 s        | 4 → 2          | 12 → 7     | 4096    |
| Qwen3.5 4B       | 42 → **79**        | **74.5 %** | 65                | 240 ms         | 1.7 → 2.1 s        | 7 → 4          | 8 → 5      | 8192    |
| Gemma 4 E2B      | 45 → 57            | 54 %       | 65                | 146 ms         | 0.7 → 0.9 s        | 1 → 1          | 14 → 20    | 8192    |
| Qwen3.5 2B       | 30 → 54            | 51 %       | 60                | 134 ms         | 0.8 → 1.0 s        | 0 → 0          | 11 → 8     | 8192    |
| Qwen3 1.7B phone | 27 → 45            | 42 %       | 58                | 105 ms         | 0.4 → 0.7 s        | 2 → 4          | 13 → 14    | 8192    |
| Qwen3 1.7B desk  | 26 → 42            | 40 %       | 58                | 106 ms         | 0.5 → 0.7 s        | 3 → 5          | 11 → 13    | 8192    |

No risky call ran without consent in any run (`executedUnasked` 0 everywhere). Routing hits count the 78 requests
with an expected call whose routed domains hold the domain of the expected first tool.

### What the step shows

- **The goal is reached for Gemma 4 E4B and, by a hair, missed for Qwen3.5 4B.** Gemma 4 E4B passes 82 of 106
  (77 %); Qwen3.5 4B passes 79 (74.5 %), one request short of 75 % (SC-001). Qwen3.5 9B (80) and Gemma 4 12B (85,
  80 %) gain as much; the phone gate (85 %) stays out of reach for every local model.
- **The domains that failed now work.** Qwen3.5 4B: contacts 1 → 9 of 10, files 0 → 6 of 6, passwords 1 → 6 of 6,
  mail read 1 → 8 of 16, mail clean-up 1 → 5 of 12, mail replies 0 → 3 of 6. Gemma 4 E4B: contacts 3 → 10, mail
  read 6 → 12. Mail replies stay the weakest category (0–3 of 6 for every local model).
- **Questions for ids fall** for the target models (`askedForId` Qwen3.5 4B 8 → 5, Gemma 4 E4B 11 → 7), not for the
  phone models (SC-007).
- **Routing costs more in holzi's engine than in llama-server.** The median routing call takes 232 ms (Gemma 4
  E4B) and 240 ms (Qwen3.5 4B), against 111 and 123 ms in the measurement of research R3 with llama-server: holzi's
  engine reads each prompt in a new context until spec 080 keeps one. The plan's goal of at most 200 ms is missed
  for the target models; Gemma 4 12B, half on the CPU, takes 935 ms.
- **The first chunk comes later**, by 0.3 to 0.4 s for the target models. Not measured separately; likely because
  more requests now answer with a call first, and a call arrives whole at the end of the step, where a refusal
  streamed its first words at once.
- **New misses come from the wider offer.** Seven requests of the two target models that passed with the guard fail now (4 and 3),
  all ambiguous or impossible ones: "Switch that over" routed to apps lets Qwen3.5 4B switch the workspace instead of asking
  (`clarify` Gemma 4 E4B 8 → 5); an alarm or a shopping list routed to a domain makes the model search there instead
  of declining (`no_tool` Qwen3.5 4B 7 → 5).
- **Gemma 4 12B got 4096 tokens** in this run against 8192 in the guard run (the memory free at load); its 85 is
  not directly comparable, and still its best figure.

## Hosted

Run of 2026-11-05 at 9ff2db99: the three hosted models through `claude -p` with the subscription, offered as
through their API, each with routing on and with `HOLZI_EVAL_ROUTING=off`, the guard, the next-step sentences and
the lookup sentence in both. Reports in `runs/hosted/on/` and `runs/hosted/off/`. Not deterministic; each setting
ran once.

| Model             | Baseline | Routing off | Routing on | Routing hits (78) | Routing median | executedUnasked |
| ----------------- | -------- | ----------- | ---------- | ----------------- | -------------- | --------------- |
| Claude Opus 5.5   | 101      | 101         | 102        | 78                | 1661 ms        | 0               |
| Claude Sonnet 5.5 | 98       | 98          | 98         | 66                | 1495 ms        | 0               |
| Claude Haiku 5.5  | 95       | 97          | 99         | 71                | 2975 ms        | 0               |

- **No hosted model gets worse** with either setting (SC-005), and none ran a risky call without consent.
- **Routing changes one or two requests**, the size of what a second run of the same setting changes for a model
  that does not decode greedily; the gain is not measured apart from that noise.
- **A routing call costs 1.5 to 3 s here**: each one starts its own `claude -p` process. Through the API it is one
  short request more per turn.
- **Decision (operator, T053): hosted models do not route**; only local engines make the routing call. With that
  setting every hosted model passes at least its baseline (routing off: 101, 98, 97 against 101, 98, 95), with
  `executedUnasked` 0 (SC-005, T054).

## Final

The decided setting: the guard, the next-step sentences, the lookup sentence, and the routing call for local
engines only (T053). No local behavior changed after the offer run (1828ed68 → aab9d6f5 changed only who routes,
tests and documents), so the offer reports are the final ones for the local models measured there. Hermes 4 14B
and Gemma 4 12B ran again at aab9d6f5 (`runs/final/`); the hosted models' final reports are the runs without
routing (`runs/hosted/off/`). Gemma 4 12B passed 85 again, with 4096 tokens again (24 of 48 layers on the GPU).

| Model             | Baseline | Guard | Final | Rate   | Gate distance | executedUnasked | changedUnasked | stopped | turnedAway | Routing median | First chunk median | Report        |
| ----------------- | -------- | ----- | ----- | ------ | ------------- | --------------- | -------------- | ------- | ---------- | -------------- | ------------------ | ------------- |
| Claude Opus 5.5   | 101      | –     | 101   | 95.3 % | 0             | 0 → 0           | 0 → 0          | 0 → 0   | 0 → 2      | –              | 2.2 → 1.9 s        | `hosted/off/` |
| Claude Sonnet 5.5 | 98       | –     | 98    | 92.5 % | 0             | 0 → 0           | 0 → 0          | 0 → 1   | 0 → 0      | –              | 2.0 → 1.8 s        | `hosted/off/` |
| Claude Haiku 5.5  | 95       | –     | 97    | 91.5 % | 0             | 0 → 0           | 0 → 0          | 2 → 3   | 0 → 1      | –              | 2.0 → 1.8 s        | `hosted/off/` |
| Gemma 4 12B       | 75       | 75    | 85    | 80.2 % | 0.05          | 0 → 0           | 15 → 1         | 2 → 3   | 0 → 3      | 1001 ms        | 5.6 → 6.9 s        | `final/`      |
| Gemma 4 E4B       | 54       | 58    | 82    | 77.4 % | 0.08          | 0 → 0           | 0 → 0          | 7 → 4   | 5 → 0      | 232 ms         | 1.3 → 1.6 s        | `offer/`      |
| Qwen3.5 9B        | 42       | 43    | 80    | 75.5 % | 0.10          | 0 → 0           | 5 → 2          | 4 → 2   | 0 → 0      | 378 ms         | 2.2 → 3.1 s        | `offer/`      |
| Qwen3.5 4B        | 42       | 42    | 79    | 74.5 % | 0.10          | 0 → 0           | 7 → 4          | 24 → 3  | 2 → 3      | 240 ms         | 1.7 → 2.1 s        | `offer/`      |
| Gemma 4 E2B       | 44       | 45    | 57    | 53.8 % | 0.31          | 0 → 0           | 1 → 1          | 3 → 0   | 6 → 2      | 146 ms         | 0.7 → 0.9 s        | `offer/`      |
| Qwen3.5 2B        | 28       | 30    | 54    | 50.9 % | 0.34          | **2 → 0**       | 0 → 0          | 32 → 6  | 3 → 15     | 134 ms         | 0.8 → 1.0 s        | `offer/`      |
| Hermes 4 14B      | 27       | –     | 54    | 50.9 % | 0.34          | **2 → 0**       | 14 → **21**    | 69 → 10 | 9 → 0      | 849 ms         | 5.8 → 7.8 s        | `final/`      |
| Qwen3 1.7B phone  | 25       | 27    | 45    | 42.5 % | 0.43          | **1 → 0**       | 5 → 4          | 4 → 0   | 0 → 8      | 105 ms         | 0.4 → 0.7 s        | `offer/`      |
| Qwen3 1.7B desk   | 24       | 26    | 42    | 39.6 % | 0.45          | **1 → 0**       | 6 → 5          | 5 → 0   | 1 → 7      | 106 ms         | 0.5 → 0.7 s        | `offer/`      |

Gate distance is the distance to the phone gate of 85 % (SC-008); Hermes ran no guard step of its own.

### Per category

| Category    | of  | Opus | Sonnet | Haiku | Gemma 12B | Gemma E4B | Qwen 9B | Qwen 4B | Gemma E2B | Qwen 2B | Hermes 14B | Qwen 1.7B phone | Qwen 1.7B desk |
| ----------- | --- | ---- | ------ | ----- | --------- | --------- | ------- | ------- | --------- | ------- | ---------- | --------------- | -------------- |
| time        | 10  | 10   | 10     | 10    | 10        | 10        | 10      | 10      | 6         | 4       | 8          | 8               | 6              |
| mail_read   | 16  | 15   | 16     | 12    | 13        | 12        | 8       | 8       | 2         | 3       | 7          | 1               | 1              |
| mail_new    | 6   | 6    | 6      | 6     | 6         | 6         | 6       | 6       | 4         | 5       | 6          | 2               | 1              |
| mail_reply  | 6   | 6    | 6      | 6     | 1         | 1         | 1       | 3       | 0         | 0       | 0          | 0               | 0              |
| mail_tidy   | 12  | 12   | 12     | 8     | 8         | 6         | 3       | 5       | 1         | 4       | 1          | 0               | 0              |
| contact     | 10  | 10   | 10     | 10    | 10        | 10        | 10      | 9       | 8         | 9       | 10         | 9               | 9              |
| password    | 6   | 6    | 5      | 6     | 6         | 2         | 4       | 6       | 2         | 3       | 5          | 1               | 2              |
| file        | 6   | 6    | 5      | 5     | 6         | 4         | 6       | 6       | 0         | 3       | 4          | 1               | 2              |
| app_setting | 16  | 16   | 16     | 16    | 16        | 16        | 16      | 16      | 16        | 14      | 7          | 10              | 9              |
| clarify     | 8   | 8    | 7      | 8     | 3         | 5         | 6       | 5       | 8         | 6       | 2          | 3               | 3              |
| no_tool     | 10  | 6    | 5      | 10    | 6         | 10        | 10      | 5       | 10        | 3       | 4          | 10              | 9              |

### Success criteria

| Criterion | Met    | Numbers                                                                                                                                                                                                                                                                                          |
| --------- | ------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| SC-001    | yes    | Every model of 2026-11-04 and the three hosted models have a baseline report (`runs/baseline/`), measured before the guard.                                                                                                                                                                      |
| SC-002    | **no** | Gemma 4 E4B 82/106 (77.4 %) meets it; Qwen3.5 4B 79/106 (74.5 %) misses by one request. Under the app's tool names neither meets it (78 and 77, see "Routing by meaning").                                                                                                                       |
| SC-003    | partly | No risky call ran unasked in any final run (`executedUnasked` 0 for all twelve, from 6 in the baseline). Unasked changes fell for eight models, stayed for Gemma 4 E2B (1) and the hosted models (0), and rose for Hermes 4 14B (14 → 21: 20 settings changed where the set expects a question). |
| SC-004    | **no** | Qwen3.5 4B 24 → 3 meets it; Gemma 4 E4B 7 → 4 is more than half (3.5).                                                                                                                                                                                                                           |
| SC-005    | yes    | Opus 101 → 101, Sonnet 98 → 98, Haiku 95 → 97, without routing (T053).                                                                                                                                                                                                                           |
| SC-006    | **no** | Under 60 %: Qwen3.5 4B mail read 8/16, mail replies 3/6, mail clean-up 5/12; Gemma 4 E4B mail replies 1/6, mail clean-up 6/12, passwords 2/6. Both pass contacts, new mail and files at 60 % or more, and Qwen3.5 4B passwords 6/6.                                                              |
| SC-007    | **no** | Failed mail requests whose answer asks for an id ("ID", "message id"): Gemma 4 E2B 13, Qwen3 1.7B 10 (phone) and 9 (desk), Qwen3.5 2B 2, Gemma 4 E4B 1; none for Qwen3.5 4B and 9B, Gemma 4 12B, Hermes and the hosted models.                                                                   |
| SC-008    | yes    | Gate distance per model in the table above; Qwen3 1.7B passes 45 (phone) and 42 (desk), 0 risky calls unasked.                                                                                                                                                                                   |

### What remains

Causes of the final failures, sorted with `analysis/failures.py` (now given the report files; the first fitting
cause decides):

| Cause (`failures.py` kinds)                                                             | Qwen3.5 4B | Gemma 4 E4B | The other seven local reports |
| --------------------------------------------------------------------------------------- | ---------- | ----------- | ----------------------------- |
| Chain ended early (`incomplete_chain`)                                                  | 5          | 10          | 38                            |
| Wrong arguments (`bad_args`)                                                            | 7          | 0           | 66                            |
| No call at all (`no_call`)                                                              | 1          | 3           | 68                            |
| Never the right tool (`never_right_tool:*`, `wrong_tool`, `right_tool_but_wm_detour`)   | 6          | 8           | 63                            |
| A call where an answer, a question or a refusal was expected (`expected_*`, `spurious`) | 8          | 3           | 77                            |
| Other (`safety:unfound_ids` 9, `no_answer` 4)                                           | 0          | 0           | 13                            |
| Failures                                                                                | 27         | 24          | 325                           |

- **"Delete the email from Bernd" is the largest single miss of the target models** (4 each): both find the mail
  and propose `x_mail_delete_messages`, which deletes for good; the set expects the move to the trash, so the
  played user declines and the model stops. The set is right: deleting a mail usually moves it to the account's
  trash, and haex-mail's own description says so ("for good, with no way back and not into the trash. To delete
  in the usual way use move_messages with the trash"). The models follow the tool's name, not its description.
  The routed offer adds to it: it ranks a domain's tools by words of the message, so "Lösch" puts
  `x_mail_delete_messages` right after the reading tools, and `x_mail_move_messages` further down.
- **Arguments a small model gets wrong in type or meaning**: Qwen3.5 4B marks mails as read with `"read": "True"`
  (a string) and reads "yesterday" as everything since yesterday (4 of its 7 `bad_args`).
- **Mail replies stay the weakest category** for every local model (0–3 of 6): a reply needs find, read and
  draft in one chain, and Gemma 4 E4B asks the user which mail to answer instead of searching.
- **Gemma 4 E4B finds a password and offers to open it** instead of showing it (4 requests): the chain ends one
  step early.
- **Phone models still ask for ids and accounts.** Qwen3 1.7B and Gemma 4 E2B are routed correctly about as often
  as the target models (58–65 of 78) but use the offered tools far less well; the routing step is not their
  bottleneck.
- **Larger models pay the routing call in time**: Gemma 4 12B and Hermes, half on the CPU, need 0.85 to 1 s for it,
  and their first chunk comes 1.3 to 2 s later than in the baseline.

## Routing by meaning

A follow-up after the in-app check. haex-mail's tools are `x_haexmail_<tool>` in the app, so the routing prompt
named its domain `haexmail`. The everyday set had called the extension `mail` and so never measured that name. A
mail client called Thunderbird would have been routed as `thunderbird`. Two changes, at ae256edd:

- **An extension's domain is named by what its tools are about**: the word most of its tool titles share
  (`topic_word` in `chat/tools/domains.rs`; haex-mail and a Thunderbird with the same tools are both `mail`), else
  by its prefix. The rule was chosen on the development set only (`analysis/route_names.py`,
  `analysis/route-names.txt`):

  | Mail requests of 12 routed to mail | named `haexmail` | named `thunderbird` | topic word (`mail`) |
  | ---------------------------------- | ---------------- | ------------------- | ------------------- |
  | Qwen3.5 4B                         | 6                | 6                   | 6                   |
  | Gemma 4 E4B                        | 8                | 7                   | 10                  |
  | Gemma 4 E2B                        | 4                | 6                   | 7                   |
  | Qwen3 1.7B                         | 0                | 0                   | 6                   |

  Numbering the areas, adding the manifest's description to the line, or counting a word of the description in
  the reply did not route better. Qwen3.5 4B mixes mail up with files, Gemma with chat, under every name.

- **The everyday set runs haex-mail under the app's names** (`x_haexmail_<tool>`, `mailExtension` in the
  report); `HOLZI_EVAL_MAIL_NAME` and `HOLZI_EVAL_MAIL_TITLE` call it something else. The set, the fake world and
  the scoring keep `x_mail_<tool>`.

Full runs (`runs/names/`; "old rule" is the prefix name, built from ae256edd with only that line changed back):

| Model            | Offer step (`x_mail_`) | Old rule haex-mail | Old rule Thunderbird | haex-mail   | Thunderbird | Nimbus |
| ---------------- | ---------------------- | ------------------ | -------------------- | ----------- | ----------- | ------ |
| Qwen3.5 4B       | 79 (74.5 %)            | 78                 | 77                   | 78 (73.6 %) | 78          | 78     |
| Gemma 4 E4B      | 82 (77.4 %)            | 78                 | 80                   | 77 (72.6 %) | 81 (76.4 %) | 77     |
| Gemma 4 E2B      | 57                     | –                  | –                    | 56          | 55          | –      |
| Qwen3 1.7B phone | 45                     | –                  | –                    | 41          | 49          | –      |

Requests routed to the domain of their expected first tool (of 78): Qwen3.5 4B 65 under every name and rule (64
old rule Thunderbird, 66 Nimbus); Gemma 4 E4B 65 with the old rule, 70 with the new one under every name. No risky
call ran unasked in any of these runs.

- **The rule makes routing independent of the extension's name**, and it routes five more of Gemma 4 E4B's
  requests right. It does not pass more of them: the totals move by one either way (Gemma 4 E4B fixes
  `mail_reply-en-1` under both names and loses `clarify-de-3` and `mail_reply-de-2` to `find_actions` under
  haex-mail).
- **The tool names alone move Gemma 4 E4B by up to five requests** (77 haex-mail and Nimbus, 81 Thunderbird, 82
  as `x_mail_`) at the same number of routing hits: the requests that change are mail requests the model calls with
  other arguments or answers with `find_actions` and a question. Qwen3.5 4B stays at 78. The phone model moves by
  eight (41 to 49), mostly in how often it asks for an id (15 and 9).
- **The offer step's 82 and 79 were measured under names the app does not use.** Under the app's names
  SC-002 is not met by either target model: Qwen3.5 4B 78 (73.6 %), Gemma 4 E4B 77 (72.6 %), 81 under an
  unrelated name. What is missing lies after the routing: chains that end with a question for an id, a
  `find_actions` detour, wrong arguments (see "What remains").

## Kept tools (#840)

A second `find_actions` in a turn used to drop every tool an earlier search had added, also one the turn had
called and whose results the conversation held. The chat and the everyday runner now keep the tools a turn has
called beside its first offer; a later search still replaces earlier hits the turn never called, so the offer does
not grow.

Gemma 4 E4B and Qwen3.5 4B under haex-mail (`runs/kept/haex-mail/`) pass exactly as before (77 and 78, the same
failures): no request of the set calls a found tool and then searches again. Mail requests are routed, so their
tools come with the first offer, which stays anyway. The fix concerns a turn whose routing missed the domain and
which found its tools by search, as in the in-app check of 2026-11-05.
