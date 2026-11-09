# Quickstart: Small local models finish what they start

## Unit tests (no model)

```bash
cd src-tauri
env -u CC -u CXX -u AR systemd-run --user --scope -q -p MemoryMax=10G -p MemorySwapMax=0 \
  cargo test -j 1 --features everyday-eval --config 'profile.dev.package.holzi.debug=0' --lib -- \
  chat::turn::arg_forms chat::tools chat::eval::everyday
```

## The everyday set and the chain set

As in spec 082's quickstart (CUDA build, `HOLZI_EVAL_ENGINE=llamacpp`, `HOLZI_TEST_GGUF`, `HOLZI_EVAL_PROFILE`).
For the chain set add:

```bash
# Cargo runs the test from src-tauri, so the path is relative to it.
export HAEX_EVAL_SET=../specs/087-agent-chains/analysis/chainset.json
```

Under another extension name: `HOLZI_EVAL_MAIL_NAME=thunderbird HOLZI_EVAL_MAIL_TITLE=Thunderbird`.

Sort the failures by cause with spec 082's script, run from the repository root:

```bash
python3 -I specs/082-agent-harness/analysis/failures.py <set.json> <report.json>...
```

Compare a step with the baseline request by request through `requests[]` (`pass`, `routed`, `firstOffer`).

## In the app

With Qwen3.5 4B and with Gemma 4 E4B, haex-mail installed:

1. "Lösch die Mail von Bernd": the mail moves to the trash, without a dialog.
2. "Zeig mir das Netflix-Passwort": the password is shown, without "Soll ich es anzeigen?".
3. "Antworte Anna, dass ich Samstag komme": a draft to Anna's mail is created, without asking which mail.
4. "Zeig mir die Mails von gestern": only yesterday's mails are listed.
