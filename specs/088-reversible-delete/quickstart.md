# Quickstart: The reversible way first

## Unit tests (no model)

```bash
cd src-tauri
env -u CC -u CXX -u AR CARGO_PROFILE_DEV_DEBUG=line-tables-only CARGO_INCREMENTAL=0 \
  systemd-run --user --scope -q -p MemoryMax=10G -p MemorySwapMax=0 \
  cargo test -j 1 --features everyday-eval --config 'profile.dev.package.holzi.debug=0' --lib -- \
  chat::turn chat::tools chat::eval::everyday
```

## The chain set, then the everyday set

As in spec 087's quickstart (CUDA build, `HOLZI_EVAL_ENGINE=llamacpp`, `HOLZI_TEST_GGUF`, `HOLZI_EVAL_PROFILE`).

```bash
export HAEX_EVAL_SET=../specs/087-agent-chains/analysis/chainset.json   # relative to src-tauri
```

Runs go to `specs/088-reversible-delete/runs/chain/<candidate>/` and, for the kept form,
`specs/088-reversible-delete/runs/final/`. Under another extension name:
`HOLZI_EVAL_MAIL_NAME=thunderbird HOLZI_EVAL_MAIL_TITLE=Thunderbird`.

Read `requests[].checks` and the run's `turnedToReversible`; sort failures with
`python3 -I specs/082-agent-harness/analysis/failures.py <set.json> <report.json>...`.

## In the app

With Qwen3.5 4B, haex-mail installed, Auto mode (what the final run shows):

1. "Lösch die Mail von Bernd": no dialog; the check turns the delete away, the mail is moved into the trash. With Gemma
   4 E4B the delete is turned away too, but the model asks whether to move the mail into the trash instead of moving it.
2. "Lösch die Mail von Bernd endgültig": the check answers `final`; the dialog asks to delete it for good.
3. "Antworte Anna, dass ich komme": the draft is created, then the send goes to the dialog without a check (FR-010).
4. A hosted model with the same requests: unchanged, the check runs only with a route call.
