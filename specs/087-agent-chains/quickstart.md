# Quickstart: Small local models finish what they start

## Unit tests (no model)

```bash
cd src-tauri
env -u CC -u CXX -u AR CARGO_PROFILE_DEV_DEBUG=line-tables-only CARGO_INCREMENTAL=0 \
  systemd-run --user --scope -q -p MemoryMax=10G -p MemorySwapMax=0 \
  cargo test -j 1 --features everyday-eval --config 'profile.dev.package.holzi.debug=0' --lib -- \
  chat::turn chat::tools chat::eval::everyday
```

## The everyday set and the chain set

As in spec 082's quickstart (CUDA build, `HOLZI_EVAL_ENGINE=llamacpp`, `HOLZI_TEST_GGUF`, `HOLZI_EVAL_PROFILE`).
For the chain set add:

```bash
# Cargo runs the test from src-tauri, so the path is relative to it.
export HAEX_EVAL_SET=../specs/087-agent-chains/analysis/chainset.json
```

`HAEX_EVAL_SET` takes any set file in the everyday set's format. To run a few requests again (a hosted model's
changed requests, say), copy `src-tauri/src/chat/eval/everyday/set.json` with only those requests and point the
variable at the copy.

Under another extension name: `HOLZI_EVAL_MAIL_NAME=thunderbird HOLZI_EVAL_MAIL_TITLE=Thunderbird`.

Sort the failures by cause with spec 082's script, run from the repository root:

```bash
python3 -I specs/082-agent-harness/analysis/failures.py <set.json> <report.json>...
```

Compare a step with the baseline request by request through `requests[]` (`pass`, `routed`, `firstOffer`).

How a step was measured (results.md):

- A lever is measured alone on the chain set first, each candidate in `runs/chain/<step>-<candidate>/`; only the
  kept one runs the everyday set, into `runs/steps/<step>/`.
- Build the eval binary once per candidate and never while a run is going: a run reads the binary the last build
  wrote.
- Local models run deterministically, so a local model's changed request is the step's effect. Hosted models do not;
  their changed requests are judged by whether the step touched them, and run again if in doubt.

## What a turn looks like now

- **Every turn**: the date line names yesterday and tomorrow and the bounds of a day (yesterday is its date to today's date);
  a word from the user's message counts as an id only when it looks like one (digits or `-_.:@/#`, FR-020).
- **A routed turn** (local models, `route::routes`): the first step offers the area's tools without `find_actions`,
  which comes with the second step; the instruction adds `FINISH` (do what was asked once found) and `NEXT_AREA`
  (search the other offered areas before saying nothing was found).

## In the app

With Qwen3.5 4B and with Gemma 4 E4B, haex-mail installed. What the final run (`runs/final/`) shows:

1. "Lösch die Mail von Bernd": both find Bernd's mail and propose deleting it for good; the permission dialog asks.
   Moving it to the trash without a dialog (SC-007) is not reached.
2. "Such mein Netflix-Passwort": Qwen3.5 4B finds the entry and opens it in the password app; Gemma 4 E4B finds it
   and asks whether to show it.
3. "Antworte auf Bernds Mail, dass ich die Rechnung bezahlt habe": both create a reply draft to Bernd's mail.
   "Antworte Anna, dass ich am Donnerstag komme" still fails for both.
4. "Zeig mir die Mails von gestern": both list only yesterday's mails.
