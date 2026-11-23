# Data Model: Abacus AI CLI delegate

**Spec**: [spec.md](spec.md) | **Research**: [research.md](research.md)

Spec 094's model (catalog, tool installation, vault home, `DelegateRuntime`, tool status) is unchanged. This file lists
only what Abacus adds or fixes to a value.

## 1. Vendor definition (a new entry of `src-tauri/src/cli_tools/catalog.json`)

```json
{
  "vendor": "abacusai",
  "display_name": "Abacus AI",
  "download_hosts": ["static.abacus.ai"],
  "executable": {
    "linux-x64": "abacusai",
    "linux-x64-baseline": "abacusai",
    "linux-arm64": "abacusai",
    "darwin-x64": "abacusai",
    "darwin-arm64": "abacusai",
    "win32-x64": "abacusai.exe",
    "win32-arm64": "abacusai.exe"
  },
  "env_set": [
    { "name": "ABACUS_AUTO_UPDATE", "value": "off" },
    { "name": "ABACUS_DISABLE_AUTO_MEMORY", "value": "1" }
  ],
  "config_args": [],
  "env_remove_prefixes": ["ABACUS_", "ABACUSAI_", "CODELLM_"],
  "config_env": "ABACUSAI_CODING_AGENT_DIR",
  "tmp_env": [],
  "versions": [{ "version": "2.10.23", "platforms": { "...": "see below" } }]
}
```

- `env_remove_prefixes` drops every host variable that could choose the account, endpoint, model or mode
  (`ABACUS_API_KEY`, `ABACUS_BASE_URL`, `ABACUS_MODEL`, `CODELLM_DEVICE_CODE`, ...). The credential is set afterwards by the
  adapter, as `CLAUDE_CODE_OAUTH_TOKEN` is for Claude Code.
- `config_env` is the agent directory variable (research §4). The measured unauthenticated runs did not create it; whether
  the active configuration generation uses it is checked in an authenticated run (quickstart §3).
- A platform entry per tested version and key, for example:

```json
"linux-x64": {
  "url": "https://static.abacus.ai/cli/releases/2.10.23/abacusai-linux-x64.tar.gz",
  "sha256": "9523013ea09211bb6dd4bdf60704ffa577e0c31e17e4879d0310fb3883fa7afc",
  "size": 78799835,
  "kind": "tar.gz",
  "unpacked_size": 157573467
}
```

(`unpacked_size` is the program plus the 10 411-byte Sigstore bundle, `157 563 056 + 10 411`.) Windows keys carry
`"kind": "zip"`. The entry for `2.10.23` is added only after the measurement of 094 FR-021 (quickstart §3); until then the
code runs against test vendor definitions.

- Validation additions: `kind` accepts `zip` (needs `unpacked_size`, like `tar.gz`); a platform key may be
  `linux-x64-baseline`; an artifact may set `requires_avx2: true` (default false; `linux-x64`, `darwin-x64` and `win32-x64` of
  Abacus do, so a host without AVX2 reads `unsupported` for them, except on Linux once a `linux-x64-baseline` entry is
  listed: that variant is tried first); `versions` may be empty (status `no_tested_version`, install
  refused) so the vendor definition can ship before its first measured version.

## 2. Platform key (`platform.rs`)

`PlatformKey` gains `LinuxX64Baseline` (`linux-x64-baseline`). `host()` returns it when the host is `linux` on `x86_64` and
`std::is_x86_feature_detected!("avx2")` is false, otherwise the base key as today. A lookup in a vendor's entries goes
through `PlatformKey::candidates()`, the variant first and then the base key: Claude Code and Codex have no variant entry and
keep using `linux-x64`. An installation records the key it was installed under; "fits this device" holds when the recorded key
is one of the host's candidates. Apply the same rule to `darwin-x64-baseline` only after the vendor archive and minimum CPU
requirements have been measured on a macOS x64 host without AVX2; until that artifact is verified, do not select or offer the
macOS x64 archive to such a host and report the platform as unsupported.

## 3. Abacus credential (a password-manager entry, per vault)

| Field                 | Value                                                                                                        |
| --------------------- | ------------------------------------------------------------------------------------------------------------ |
| owner                 | `delegate` (`Caller::Internal { feature: "delegate" }`; only that feature and the user see it)               |
| title                 | `Delegate: Abacus AI`                                                                                        |
| `password`            | the secret: the device code the vendor's sign-in returned                                                    |
| custom field `kind`   | `device_code` (decides the environment variable, see below)                                                  |
| custom field `device` | `vault` for a shared credential, or the installation ID for a device-bound credential (see "Device binding") |
| tags                  | `haex-delegate-abacusai`                                                                                     |

- One shared entry per vendor per vault when the credential is portable; otherwise one entry per installation. The provider
  table still has at most one Abacus row per vault; see the duplicate-row recovery rule in the command contract. `read`
  selects the exact scope and returns `Ready(kind, secret)` or `Missing` (no entry: deleted, or the vault's sync has not delivered
  it yet; the marketplace tells the two apart by the entry's id, which no row here can name). Saving on one installation never replaces another
  installation's entry. Duplicate resolution is newest `updated_at` within the same scope. Disconnecting the provider is
  vault-wide and removes every scope; deleting a duplicate provider row while another survives removes no credentials.
- `kind` selects what the adapter sets after containment: `device_code` is `CODELLM_DEVICE_CODE=<secret>`. An API key is
  never stored or set (spec FR-005); `ABACUS_API_KEY` is removed from the host's environment by the vendor definition.
- **Device binding** (open, quickstart §6): the sign-in sends a random UUID as `deviceId`. The vendor definition records
  whether the returned code is shared or device-bound, based on the operator measurement. A shared credential uses scope
  `vault` and serves every device. A device-bound credential uses the installation ID as scope; device B's connection creates
  its own entry and cannot replace device A's. Each adapter reads only its selected scope, and both records can coexist
  after sync. Disconnecting the provider is a vault-wide action that deletes every scope.
- The provider row of the vendor (`providers`, `kind = cli_delegate`, `adapter = "abacusai"`, `credentials = NULL`) exists once
  connected, so the model picker lists the backend; it holds no secret. Removing the tool or disconnecting deletes the entry and
  the row.

## 3a. The sign-in service (research §3; private, not documented by the vendor)

Base address `https://apps.abacus.ai` (fixed in code; a test double replaces it in tests). Both calls are `POST` with a JSON
body and a JSON answer.

| Call                                 | Body                                            | Answer on success                                                                 |
| ------------------------------------ | ----------------------------------------------- | --------------------------------------------------------------------------------- |
| `/api/_authenticateDeviceForCodeLlm` | `{"email": "...", "deviceId": "<random uuid>"}` | `{"success": true, "result": {"verificationUri", "userCode", "pollingInterval"}}` |
| `/api/_pollDeviceCode`               | `{"userCode": "..."}`                           | `{"success": true, "result": {"deviceCode", "chatWebUrl"}}` once confirmed        |

A `success: false` (or an answer without `result`) on the first call ends the flow with the vendor's `error` text; on the
second call it means "not confirmed yet" and polling goes on every `pollingInterval` seconds (default 5) for at most ten
minutes (the tool's own limits). Nothing of these bodies or answers is logged; the progress event carries only the address
and the code, which the user needs on screen.

## 4. The JSON-lines protocol subset the adapter uses (research §5)

Input to the program (one JSON object per line on stdin):

```json
{"type":"user","message":{"role":"user","content":"<text>"}}
{"type":"control_response","response":{"subtype":"success","request_id":"<id>","response":{"decision":"accept"}}}
{"type":"control_response","response":{"subtype":"success","request_id":"<id>","response":{"decision":{"type":"reject_with_message","message":"<text>"}}}}
{"type":"control_request","request_id":"<id>","request":{"subtype":"interrupt"}}
```

Output from the program (one JSON object per line on stdout; stderr carries diagnostics and, on exit 1, a last line
`{"type":"fatal","reason":...}`):

| `type`                                           | What the adapter does with it                                                                                                |
| ------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------- |
| `system` (`init`)                                | records `cli_version`, `protocol_version`, `permission_mode`; refuses a `permission_mode` other than `supervise`             |
| `event`                                          | turned into the stream deltas of the chat (text, tool use, tool result); unknown events are ignored                          |
| `control_request` with `subtype: "can_use_tool"` | calls `approval_bridge::request_approval`, answers with `accept` or `reject` (with the user's note as `reject_with_message`) |
| `result`                                         | ends the turn: `success` finishes, `is_error` ends with the fatal reason mapped to a chat error (below)                      |
| anything else                                    | ignored (the vendor adds message types between versions)                                                                     |

The adapter never sends `allowAlways`, `allowYolo`, `allow_always_with_rules`, `accept_with_message` for a tool the user
refused, or `question_answers` it did not collect from the user.

Fatal reasons (exit 1) mapped to the delegate errors that spec 007 and the existing adapters already show: `not_authenticated` and
`access_denied` lead back to connecting (FR-007); `network_unreachable` is "network"; `no_models`, `invalid_config`,
`startup_timeout`, `internal` are shown as "the tool failed" with the reason text.

## 5. Per-run folder

Unchanged from 094: `<vault>/home/run/<id>/` (config folder = `ABACUSAI_CODING_AGENT_DIR`, `tmp` = `TMPDIR`, working
folder), created at the start of a run and removed when it ends. The 13 MB native library the program extracts lands in that
`tmp` folder, so it is removed with the run; the cost is one 13 MB write per start. No credential file is created by
HaexVault; whether the program writes one by itself with `CODELLM_DEVICE_CODE` set (a keystore) is checked in the
authenticated measurement (quickstart §3) and, if it does, removed with the run folder.

## 6. State transitions

Tool states are spec 094's. Connection states of the Abacus backend: `not_installed` (no tool: install offered) to
`not_connected` (tool, no entry) to `connected` (tool and entry) and back through `disconnect`; `credential_refused` is a
result of a run (`not_authenticated`), not a stored state, and leads to `not_connected` once the user disconnects or
reconnects.
