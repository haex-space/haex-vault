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
