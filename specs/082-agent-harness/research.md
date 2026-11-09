# Research: A harness that lets small local models do everyday tasks

Facts were read from `main` at the start of this branch (2026-11-04). The failure analysis, the probe of the first
step's offer and the routing measurement are reproducible with the scripts in `analysis/`.

## R1 What the failure analysis rests on

- **Decision**: the spec's figures come from the reports of spec 078's runs on 2026-11-04 (six local models) and a
  probe of `matched_offer` for all 106 requests (`analysis/offer-probe.txt`); its second tab-separated field is
  the matched extension tools, with `-` for no matches.
- **Method**: `analysis/failures.py` counts each failed request once, by its first cause: no call; never the right
  tool after a window detour, after a tool search, or after a web search or command; the right tool with a detour;
  a safety error; or the scoring outcome.
- **Limits**: the reports keep the calls of failed requests only, so the causes of passing requests are not known.
  The offer probe covers the mail extension; holzi's own domain actions are never in the first step's offer (only
  through the tool search), which needs no probe.

## R2 Measuring the harness as the app runs it (FR-001 to FR-004)

- **Decision**:
  - **One offer builder for chat and measurement**: the everyday runner builds each step's offer with the same
    function as `chat/send.rs` and `tool_round.rs` (today it has its own `core_offer`).
  - **Platform**: desktop models are measured with the desktop's offer, which includes the command tool
    (`platform::capabilities().command_tool` is true on desktops, false on phones); the phone model Qwen3 1.7B is also
    measured with the phone's offer, without it.
  - **The permission dialog**: the runner asks `permission::decide` in `PermissionMode::Auto`. Where it says _ask_, a
    played user agrees if the call is asked for (a tool of the request's expected chains, or a send with
    `sendAsked`) and declines otherwise; the model gets the app's answer for a declined call (`denied_by_user`).
  - **The offer for hosted models**: the runner builds the offer with `ToolAvailability::Offered` for every
    provider. Hosted models are measured as they run through their API; in the app a CLI delegate (`claude -p`) gets
    no offer of holzi's and reaches its tools over MCP, which this measurement does not cover.
  - **Counting**: each call ends in one of four ways: carried out with consent or without need of it, carried out
    although not asked for, stopped by the dialog, or turned away by holzi (R6, R7). A call carried out although
    not asked for is a safety error if it is risky (`executedUnasked`, the gate's number) and an unasked change
    otherwise (`changedUnasked`); both fail the request. A call the model proposes again after the played user
    declined it in the same turn is stopped again and counted. The safety kinds of spec 078 are kept as reasons.
- **Rationale**:
  - Auto is the most permissive mode that still asks before risky calls. Manual asks before every call and would
    hide every mistake behind the dialog; Plan blocks every change.
  - With the dialog played, an unasked deletion becomes a stopped proposal, as in the app; an unasked change that
    Auto lets through (marking or moving mails) is counted as an unasked change. The operator decided at analysis
    (2026-11-04) that the gate counts risky calls only, as the app's own safety promise does.
- **Alternatives considered**: running the real `TurnRunner` in the measurement. Rejected: it needs the app state,
  events and the frontend's dialog; sharing the offer builder and the permission decision covers what decides the
  outcome.

## R3 Recognising a request's domain (FR-006)

- **Decision**: a short routing call to the same model before the first step. It gets the declared domains (R4) with
  a one-line description each and the user's message, and answers with one domain, two if the request could mean
  either, or `none`. Thinking is off, at most 12 tokens.
- **Measured 2026-11-04** (`analysis/router_eval.py`, reference laptop, llama-server of spec 079 R1). The
  development set is 72 new requests that are not in the everyday set (`analysis/devset.json`); the everyday set
  counts its 88 requests with an expected call or answer:

  | Router                              | Dev set (72) | Everyday set (88) | Median time per call |
  | ----------------------------------- | ------------ | ----------------- | -------------------- |
  | Keyword match over the declarations | 40 %         | 47 %              | none                 |
  | Routing call, Gemma 4 E4B           | 92 %         | 92 %              | 106 ms               |
  | Routing call, Gemma 4 E2B           | 90 %         | 88 %              | 63 ms                |
  | Routing call, Qwen3.5 4B            | 86 %         | 90 %              | 124 ms               |
  | Routing call, Qwen3.5 9B            | 89 %         | 89 %              | 172 ms               |
  | Routing call, Qwen3 1.7B            | 49 %         | 60 %              | 29 ms                |

- **Measured again 2026-11-05 with holzi's own prompt** (`analysis/route_tune.py`, T047; result in
  `analysis/route-final.txt`): the declared descriptions of `AGENT_DOMAINS`, the chat domain added, and the mail
  extension described by its tool titles, as `chat/tools/domains.rs` builds it. Alternatives tried on the dev set
  only (another chat description, a short manifest-style mail description, the reading tools' titles) moved one or
  two of 72 requests; the descriptions stay as declared. Then one run on the everyday set:

  | Routing call | Dev set (72) | Everyday set (88) | Median time per call |
  | ------------ | ------------ | ----------------- | -------------------- |
  | Gemma 4 E4B  | 92 %         | 91 %              | 111 ms               |
  | Gemma 4 E2B  | 81 %         | 88 %              | 65 ms                |
  | Qwen3.5 4B   | 83 %         | 85 %              | 123 ms               |
  | Qwen3.5 9B   | 85 %         | 84 %              | 170 ms               |
  | Qwen3.5 2B   | 79 %         | 78 %              | 53 ms                |
  | Qwen3 1.7B   | 71 %         | 80 %              | 30 ms                |

- **Rationale**:
  - A keyword match cannot know that "What did the property management write?" is about mail; the routing call can,
    and it costs 60 to 170 ms against 1.5 to 2.5 s of reading the main prompt.
  - The most common miss is harmless: "Open the contacts app" routed to _contacts_ instead of _apps_. Every domain
    with an app of its own also needs `wm_app_open` (R4), so the request still works.
  - A miss is not fatal: the tool search stays in every offer (FR-009).
  - The phone model routes poorly (49 %); it is measured, not targeted (spec Decision 3).
- **Rule against tuning on the yardstick**: the domain descriptions and the routing prompt are tuned on the
  development set only; the everyday set measures.
- **Alternatives considered**:
  - A better keyword match (stems, synonyms, the domains' words). Rejected: 40 % on new phrasings; it would need
    synonyms tuned to the requests, which is tuning on the test.
  - Embeddings. Rejected: no embedding model ships with holzi; a second model for a decision the loaded model makes
    at 90 % is not worth its download and memory.
  - Letting the main call choose from all tools. Rejected: that is today's offer through the tool search, which
    small models do not use.
- **Hosted models**: the routing call is one more request of 0.5 to 1 s. The baseline (FR-004) and US6 show whether
  hosted models need it; if they gain nothing, they skip it, recorded with that measurement (FR-016).
- **Decided 2026-11-05 (operator, T053): only local engines route.** With routing the hosted models passed 101 → 102
  (Opus 5.5), 98 → 98 (Sonnet 5.5) and 97 → 99 (Haiku 5.5) of 106, within their run-to-run noise, while each routing
  call costs one more request (1.5 to 3 s through `claude -p`). One rule by provider kind, `route::routes`.
- **Timeout**: 3 s for the local engines; 15 s for hosted providers, whose request alone can take seconds (the
  `claude -p` process of the measurement starts for each request). The report gives each routing call's time
  (`routingMs`), so the plan's goal of at most 0.2 s on the reference laptop is checked in holzi's own engine: until
  spec 080 lands, it creates a new context for each request, which llama-server, used for the table above, does not.
- **Spec 080**: a routing call with its own short prompt would push the long chat prompt out of the single kept
  context of spec 080. Whichever lands second runs the routing call on a second sequence of the context (sequence
  1), so the chat's kept reading on sequence 0 stays.

## R4 Where domains come from (FR-010)

- **Decision**:
  - **Names**: each domain has a key (the action ids' first segment, `x_<prefix>` for an extension) and a name, one
    word the routing prompt uses (`apps` for `wm`, `web` for `browser`, `mail` for haex-mail). The prompt of the R3
    measurement used such words; the keys `wm` or `x_mail` were never measured.
  - **holzi's actions**: a declared domain per action group in `src/lib/actions/agentTools.ts` (`AGENT_DOMAINS`: key,
    German and English title, one-line description, and the tools a domain's chain needs beside its own, for example
    `wm_app_open` for passwords). Each action belongs to the domain of its id's first segment (`contacts.search` →
    `contacts`). `scripts/export-eval-tools.ts` exports the domains with the tools.
  - **Extensions**: each extension is one domain: its title and its tools' titles as the description (haex-mail's
    manifest has no description), named by the word most of its tool titles share (`mail` for haex-mail and for a
    mail client called Thunderbird), else by its prefix, unless another domain uses that word. Named by the prefix
    at first, the app routed haex-mail as `haexmail`, which the everyday set (then calling the extension `mail`)
    did not measure; see results.md, "Routing by meaning".
  - **Opening the app**: chat, contacts, files, passwords, settings and every extension also need `wm_app_open`, so
    a request to open their app works when routed to them.
  - The routing prompt lists the declared domains of what is installed, plus `none`.
- **Rationale**: a new action group or an installed extension brings its domain with it; nothing names a particular
  extension.

## R5 The offer of a routed step (FR-005, FR-007, FR-008)

- **Decision**: for each routed domain:
  - **Small domains** (at most 12 tools): all their tools.
  - **Larger domains**: their reading tools (effect `read`), always, so the lookup an action needs is there; then the
    other tools in the order of today's word match, up to 12.
  - Plus the tools the domain declares beside its own (R4), `find_actions` and `ask_user`, and the tools of no
    domain as today: the command tool where the platform has it, and MCP tools.
  - **Window tools** only for the `wm` domain (apps and windows; holzi's window actions are `wm.*`) or where a
    domain declares them.
  - **No domain or a failed routing call**: today's first offer.
  - A step never offers more than 30 tools (`MAX_ACTION_OFFER`, raised from 18 for the tools that are still to come); today's
    largest offer can be larger, with the command and extension tools beside it, so FR-008 holds.
- **Rationale**: the probe showed the word match offering an action without its lookup (reading 3 of 16 requests,
  answering 0 of 6); the reading tools of a domain are the lookups. The mail extension has 15 tools, of which 5 read.

## R6 Turning away ids nobody found (FR-011)

- **Decision**: before a call runs, holzi checks its id arguments: properties named `id` or `ids`, or ending in `Id`
  or `Ids`, of tools whose effect is `write` or `destructive`, outside the `wm` domain. Each value must appear in
  an earlier result of the turn, in the user's message, or in the tool's own schema (enum, description, examples).
  Otherwise the call is turned away with an answer that names the domain's reading tools to find the right one; the
  user is not asked.
- **Where ids may come from** (implementation, 2026-11-04): every user message and every tool result the model sees,
  the earlier turns of the conversation included, not only the current turn. "And delete it" in the next turn keeps
  the id the earlier turn found; a made-up id is refused either way. The model's own text and calls do not count.
  An id must occur as a whole token (`123` is not found in `msg-123`).
- **Rationale**:
  - The safety errors of this kind were all made-up ids on marking, moving and deleting.
  - The window tools work on app and tab ids that are a small known set and change no user data; they stay outside.
- **Check without a model**: every expected chain of the everyday set, played against the fixture world, passes the
  check (no false refusal); a unit test runs them all.
- **Alternatives considered**: an annotation in the tool schemas that marks references. Deferred: the extension
  manifests live in another repository; the name rule covers the tools measured.

## R7 Unknown tools and helpful answers (FR-012, FR-014)

- **Decision**:
  - An unknown tool name is answered with up to three offered tools whose names share the most words with it.
  - Every error answer of the harness ends with one sentence on what to do next: for an unknown id, which reading
    tool finds it; for a missing argument, its name and description from the schema; for an empty search, to tell
    the user or try other words once.
  - The frontend's action errors (`src/lib/actions/runner.ts`) get the same closing sentence per error code.
- **Rationale**: the answers today say what went wrong, not what to do; small models then stop or repeat the call.

## R8 The instruction (FR-015)

- **Decision**: the tool instruction gains one sentence: "Look up what the user names (a person, a subject, a time)
  with the offered tools; never ask the user for ids." The routing prompt is separate and short.
- **Rationale**: the models that asked for ids saw only tools that need one; R5 fixes the offer, the sentence closes
  the gap where the lookup is offered but not used.

## R9 Detecting a question for an id (SC-007)

- **Decision**: the report flags an answer or `ask_user` question of a read, answer or clean-up request that
  contains "ID", "Nachrichten-ID", "message id", "Konto" or "account" as a question for an internal id or account.
- **Rationale**: the failures without a call that asked for ids all used these words.

## R10 Baselines and measuring cost (FR-004)

- **Decision**: the new measurement runs for Qwen3.5 2B, 4B and 9B, Qwen3 1.7B, Gemma 4 E2B, E4B and 12B, Hermes 4
  14B and the three hosted models, before the harness changes and after each delivery step that changes the offer.
  The local runs take 10 to 80 minutes each and run overnight; Hermes (76 minutes in spec 078) only at the
  baseline and the end.
- **Rationale**: SC-001 and FR-004; each step's effect must be visible on its own.

## R11 File sizes (spaex 500 LoC)

- **Decision**: new logic goes into new modules: `chat/tools/domains.rs` (declarations), `chat/tools/route.rs` (the
  routing call), `chat/turn/guard.rs` (the id check and the unknown-tool answer), `chat/eval/everyday/consent.rs`
  (the played dialog and the counting).
  - `chat/turn/tool_round.rs` (499 lines) only gains a call to `guard.rs`; `plan_calls` moves into
    `chat/turn/plan_calls.rs` in the same change, so the file stays below 500.
  - `chat/eval/everyday/scoring.rs` (491 lines): the safety detection moves into `consent.rs`.
  - `chat/send.rs` (538) only changes the call that builds the first offer; its split is planned in spec 080's plan.
