# Quickstart: A harness that lets small local models do everyday tasks

## Unit tests (no model)

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib --config 'profile.dev.package.holzi.debug=0' -j 1 \
  -- chat::tools chat::turn chat::eval
pnpm check:agent-actions
```

In the 10 GB scope. They cover:

- the domains of every action;
- the routing prompt and the parsing of its answer;
- the routed offer of every everyday request without a model (every expected chain's first tool is offered when the
  routed domain is right);
- the guard against every expected chain played on the fixture world;
- the played dialog's four endings.

## The everyday set

```bash
HOLZI_EVAL_ENGINE=llamacpp HOLZI_TEST_GGUF=<models>/<model>.gguf HOLZI_EVAL_PROFILE=desktop \
  cargo test --manifest-path src-tauri/Cargo.toml --features llm-llamacpp-cuda,everyday-eval \
  --test model_tool_eval -- --ignored --nocapture measure_everyday_voice
```

- `HOLZI_EVAL_PROFILE=phone` for the phone model (no command tool). The report lands in
  `src-tauri/target/eval/everyday-<model>-<profile>.json`, one file per profile.
- `HOLZI_EVAL_ROUTING=off` measures without the routing call (the baseline and the hosted comparison).
- haex-mail's tools run under the app's names (`x_haexmail_<tool>`). `HOLZI_EVAL_MAIL_NAME=thunderbird` with
  `HOLZI_EVAL_MAIL_TITLE=Thunderbird` runs them under an extension name that says nothing about mail.
- Hosted models through the subscription as in spec 078's quickstart.

Compare `pass`, `executedUnasked`, `stopped`, `turnedAway`, `askedForId` and the categories with the baseline in
`specs/082-agent-harness/results.md`.

## The routing measurement

```bash
python3 -I specs/082-agent-harness/analysis/router_eval.py <llama-server> <models> \
  src-tauri/src/chat/eval/everyday/set.json src-tauri/src/chat/eval/tools.json \
  src-tauri/src/chat/eval/everyday/mail_tools.json specs/082-agent-harness/analysis/devset.json <model stem>...
```

For how an extension's name affects routing (names, styles and the topic-word rule):

```bash
python3 -I specs/082-agent-harness/analysis/route_names.py <llama-server> <models> \
  src-tauri/src/chat/eval/domains.json src-tauri/src/chat/eval/everyday/mail_tools.json \
  specs/082-agent-harness/analysis/devset.json [names,...] <model stem>...
```

Tune the domain descriptions on the development set only.

## In the app

Each with Qwen3.5 4B (the local default) and with Gemma 4 E4B:

1. Ask "Was schreibt die Hausverwaltung?": the answer quotes the mail without opening the mail app.
2. Ask "Lösch die Mail von Bernd": the mail moves to the trash, without a dialog in Auto mode. If the model
   proposes deleting it for good instead, the permission dialog asks; decline it, and the model accepts.
3. Ask "Was ist die Handynummer von Anna?": the number comes without a tool search.
