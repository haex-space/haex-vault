# Test extension `mail-opener` (spec 063, T010)

The extension of the e2e scenes of spec 063 and of the install and list tests: it registers as an
opener for `mail.compose` and, like a mail extension, asks holzi for the request the user gave it when
its frame starts and whenever `opener-request` arrives. Its bundle `../mail-opener.xt` is built and
signed by `tests/extension_e2e_fixtures.rs` with the fixtures' test key.

| File            | Role                                                                         |
| --------------- | ---------------------------------------------------------------------------- |
| `manifest.json` | The manifest; no permissions, `"openers": ["mail.compose"]`                  |
| `index.html`    | Entry page with a minimal client of the SDK protocol, like `../probe.html`   |

There is no `app.js` and no build step: the page speaks the bridge directly (`contracts/bridge.md`), so
the scenes test holzi's side without the SDK, which gets `client.openers` in its own release. The
result of `extension_opener_take` is in `#request` as JSON, the number of calls in `#taken`.

After a change:

```sh
HOLZI_WRITE_E2E_FIXTURES=1 cargo test --manifest-path src-tauri/Cargo.toml --test extension_e2e_fixtures
```
