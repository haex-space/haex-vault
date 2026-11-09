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
