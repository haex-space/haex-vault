# Phase 0 Research: Abacus AI CLI delegate

**Date**: 2026-11-22 | **Spec**: [spec.md](spec.md)

Everything below was read from the vendor's own pages and release files, or measured on 2026-11-22 with the `abacusai`
2.10.23 program for `linux-x64`, started with an empty environment, a throwaway home, read-only access to the rest of the
disk (`bwrap`) and every file and network call recorded (`strace`). The tool was started without an Abacus account, so
nothing below says what an authenticated run does; those points are marked **operator** and listed in
[quickstart.md](quickstart.md). Statements taken from the binary's own embedded text (the program carries its help text and
user documentation as strings) are marked **binary**; they describe 2.10.23 and need re-checking per version.

## 1. Source and integrity

**Decision**: install from `https://static.abacus.ai/cli/releases/<version>/abacusai-<platform>.<ext>`; keep, per tested
version and platform, the SHA-256 of that archive in the catalog (as for the other two tools); the maintainer script that
creates an entry first verifies the vendor's Sigstore signature, so the app needs no signature library.

**Evidence**:

- The vendor's installer (`https://static.abacus.ai/cli/install.sh`, read in full) builds
  `BASE_URL/<version>/abacusai-<platform>.tar.gz` from `https://static.abacus.ai/cli/releases`; it picks the version from an
  argument, `ABACUSAI_VERSION`, or the vendor's API
  (`https://apps.abacus.ai/api/v0/_getCodellmCliVersion?channel=latest` answers `2.10.23`; channel `canary` answered the same
  version on 2026-11-22). **The installer verifies nothing**: no checksum, and the signature file is only moved
  ("Sigstore signature (best-effort, silent)").
- The Windows installer (`install.ps1`) downloads `abacusai-<platform>.zip` and expects `abacusai.exe` in it.
- Every archive holds the program and one Sigstore bundle next to it. `abacusai-linux-x64.tar.gz` (78 799 835 bytes,
  SHA-256 `9523013ea09211bb6dd4bdf60704ffa577e0c31e17e4879d0310fb3883fa7afc`) holds `abacusai` (157 563 056 bytes, SHA-256
  `108c4ba6e4076fc911959169bb7b94e64106af89ff83c3a8e32a050f91591d56`) and `abacusai-linux-x64.sigstore` (10 411 bytes, a
  `application/vnd.dev.sigstore.bundle.v0.3+json` message signature over the program's SHA-256, with a Rekor entry).
- The signing certificate names the vendor's release workflow: issuer `https://token.actions.githubusercontent.com`,
  identity `https://github.com/abacusai/codellm/.github/workflows/release-cli-native.yml@refs/tags/cli-v2.10.23`
  (repository `abacusai/codellm`, tag `cli-v2.10.23`, a `workflow_dispatch` run). The repository is private (the public
  `abacusai/abacusai-desktop` repository holds issues only), so the identity cannot be cross-checked against a source tree.
- **Verified**: `sigstore verify identity` (sigstore-python 4.5.1, in a throwaway virtual environment) with that identity and
  issuer reports `OK` for the `linux-x64` program; the same call with another identity fails. Not verified for the other
  platforms yet (the procedure is the same; a task).
- No separate checksum or signature file exists next to the archives (`.sigstore`, `.sha256`, `SHA256SUMS`, `checksums.txt`,
  `manifest.json` answer 403).

**Consequences**: spec 094 FR-006 is satisfiable for Abacus: the catalog holds archive SHA-256 values, and the maintainer
script pins the signing identity by pattern
(`^https://github\.com/abacusai/codellm/\.github/workflows/release-cli-native\.yml@refs/tags/cli-v<version>$`, issuer
GitHub Actions) and refuses an entry whose bundle does not verify against the unpacked program. The vendor's own installer
and the npm package are not used (they verify nothing and the npm route needs Node). Open for the operator: a signature
by a private repository's workflow shows that the vendor released the file, not what it contains.

## 2. Platforms, variants and sizes (version 2.10.23)

From the installers and `HEAD` requests (sizes are the archive's `content-length`):

| Platform key              | Archive   | Bytes      | Note                     |
| ------------------------- | --------- | ---------- | ------------------------ |
| `linux-x64`               | `.tar.gz` | 78 799 835 | glibc, AVX2              |
| `linux-x64-baseline`      | `.tar.gz` | 78 799 840 | x64 without AVX2         |
| `linux-x64-musl`          | `.tar.gz` | 75 228 054 | musl systems             |
| `linux-x64-baseline-musl` | -         | 403        | does not exist           |
| `linux-arm64`             | `.tar.gz` | 77 798 493 |                          |
| `linux-arm64-musl`        | `.tar.gz` | 74 086 821 |                          |
| `darwin-arm64`            | `.tar.gz` | 65 733 250 |                          |
| `darwin-x64`              | `.tar.gz` | 70 119 907 |                          |
| `darwin-x64-baseline`     | `.tar.gz` | 70 119 909 |                          |
| `win32-x64`               | `.zip`    | 81 850 321 |                          |
| `win32-x64-baseline`      | `.zip`    | 81 850 321 | same size as `win32-x64` |
| `win32-arm64`             | `.zip`    | 75 591 249 |                          |

The installer picks the variant at install time: `-musl` on a musl system, `-baseline` where the x64 CPU lacks AVX2.
**Consequence for the plan**: the catalog's platform key is not only (os, arch); `platform.rs` must also decide the musl and
baseline variants on the device (two small probes: the C library, the CPU feature). Unpacked, the Linux x64 program is
157 MB, about twice the archive, plus 13 MB of native library extracted per start (section 4).

## 3. Which credential the tool accepts

**Evidence**:

- The vendor's documentation (`abacus.ai/help/abacusai-desktop/headless`, `.../settings`): `ABACUS_API_KEY` in the environment
  "signs you in without a stored login"; the page names no other credential.
- **Binary**: the embedded documentation adds `CODELLM_DEVICE_CODE` ("set by the desktop app"; it wins over the API key) and
  states that while either is set, `/login` and `/logout` cannot change the account. The login itself is "your email (a
  code confirmed in the browser) or an API key".
- Measured: `abacusai auth login` without a terminal exits 1 with "abacusai auth login needs an interactive terminal. In
  scripts and CI, set the ABACUS_API_KEY environment variable instead." `abacusai auth status` without a login prints
  "Not logged in" and exits 0. `abacusai -p ... --output-format stream-json` without a credential prints a `result` with
  `is_error: true` and, on stderr, `{"type":"fatal","reason":"not_authenticated",...}`, exit 1.
- **Binary**: a stored login lives in an encrypted file `keystore.enc` under the tool's home folder (mode 0600,
  AES-GCM); its key is derived from `<machine key>:<user name>` (the host name and the operating-system user name). A file
  that cannot be decrypted is renamed `keystore.enc.unreadable` and "stored credentials must be re-entered". **A keystore
  cannot travel between devices**, which is why the tool's own login cannot satisfy spec 007 FR-002 or this spec's FR-006.
- **Binary** (the program's own login code, read in 2.10.23): the login is a device-code exchange with two calls to the
  vendor's host (`https://apps.abacus.ai`, overridable with `ABACUS_BASE_URL`), both private and undocumented, the ones the
  CLI and the desktop app use:
  1. `POST /api/_authenticateDeviceForCodeLlm` with `{"email": <address>, "deviceId": <machine id>}` answers
     `{"success": true, "result": {"verificationUri", "userCode", "pollingInterval"}}`. The tool shows "Open the URL and enter
     the code to log in", opens the browser, and waits.
  2. `POST /api/_pollDeviceCode` with `{"userCode": <code>}` (optionally `keyType`), polled every `pollingInterval`
     seconds (default 5) for up to ten minutes, answers `{"success": true, "result": {"deviceCode", "chatWebUrl"}}` once the
     user has confirmed the code in the browser.
  3. The tool stores `deviceCode` in `keystore.enc` as `codellm.devicecode`. `CODELLM_DEVICE_CODE` carries exactly this
     value, so a device code in the environment is the same credential the stored login holds. (`deviceId` is the
     `node-machine-id` of the device, or a hash of the host name when that fails.)
     Not measured (**operator**): the answers of the service to a client that is not the vendor's own, whether a device code is
     accepted on a device other than the one named by `deviceId`, how long it lasts, and what a run does with it.

**Decision** (operator, 2026-11-22): the credential is the account's own sign-in, never an API key (an API key is billed per
request, the sign-in unlocks free models); it is connected from the app's UI and lives in the password manager (spec
Clarifications). What is stored is one secret, the device code, and the kind says so.

- **Connect**: HaexVault runs the two calls itself with `reqwest`, like Codex's connect flow runs its device authentication:
  the UI asks for the e-mail address, `connect_cli_delegate` calls the first endpoint, emits `delegate-connect-progress` with
  `awaiting_browser`, the address and the code (the event the Codex flow already has, and the UI already renders), polls the
  second, stores the device code as the vault's entry and returns `Connected`. `deviceId` is a random UUID made for this
  connect, not the host's machine id and not a stable identifier of the device or the vault (so nothing that identifies the
  host or the vault reaches the vendor; the cost is one new device registration per connect on the vendor's side). The base address is the vendor's, fixed;
  a test double of the service stands in for it in tests.
- **Run**: the adapter sets `CODELLM_DEVICE_CODE=<secret>` for the child after containment; `ABACUS_API_KEY` is never set
  (and removed from the host's environment by the vendor definition).
- **Risk** (stated in the spec): the two endpoints are not documented, and a change on the vendor's side breaks connecting
  without a new program version, so the tested-version list cannot protect it; the failure is then a specific connect error,
  not a hang. If the service refuses other clients, the operator decides what replaces it.
- Rejected: an API key (billing, above); running `auth login` in a terminal view or a hidden pseudo-terminal (the operator wants
  no terminal; the login needs a real terminal and ends in a device-bound keystore whose key would have to be recomputed to
  read the code back); carrying `keystore.enc` in the vault (device-bound, and a credential file in the vault's folder).

## 4. What the tool writes and what moves it (measured)

Unauthenticated runs of `--version`, `--help`, `auth status`, `auth login` (no terminal), `-p` and `-p --input-format
stream-json`, in a throwaway `HOME`; the first series had no other variable set, the second set `TMPDIR` and
`XDG_CONFIG_HOME`:

| What                                                                                   | Where (no variables)                     | Moved by                       |
| -------------------------------------------------------------------------------------- | ---------------------------------------- | ------------------------------ |
| Log file per process (`<time>-<id>.log`)                                               | `~/.abacusai/logs/`                      | `HOME` only                    |
| Process marker while a print run lives (`<n>.pid`)                                     | `~/.abacusai/cli/` (empty after exit)    | `HOME` only                    |
| Config store (an empty folder `abacusai-cli-nodejs`)                                   | `$XDG_CONFIG_HOME` or `~/.config/`       | `XDG_CONFIG_HOME`, else `HOME` |
| Native library extracted from the program (13 174 248 bytes, `.bun-<uid>-<hash>.node`) | `$TMPDIR` or `/tmp`, stays after the run | `TMPDIR`                       |

- With `TMPDIR` and `XDG_CONFIG_HOME` set, the library and the config store landed in those folders and nothing was written
  to `/tmp` or the real home. **No network connection was made** in any unauthenticated run, with the vendor's own update
  setting at its default (`--version`, `auth status`) and with it off.
- The program runs `/usr/bin/ldd` once (to detect the C library); it does not write anywhere else.
- **Binary** (not measured): the agent directory (`settings.json`, `auth.json`, `mcp.json`, sessions) is
  `~/.abacusai/agent/` unless `ABACUSAI_CODING_AGENT_DIR` is set; sessions move with `ABACUSAI_CODING_AGENT_SESSION_DIR`;
  per-repository agent memory is `~/.abacusai/projects/<repo>/memory` (off with `ABACUS_DISABLE_AUTO_MEMORY=1`);
  checkpoints are `~/.abacusai/checkpoints`; user instructions are read from `~/.abacusai/AGENTS.md`. The vendor's
  documentation describes `~/.abacusai/config.json` instead; the program carries two generations of configuration
  (`.abacusai/` and `.abacusai-next/`) and which one 2.10.23 uses in an authenticated run is **operator**.
- Existing base layer (`cli_tools/containment.rs`, spec 094): it already sets `HOME` (and the Windows account folders),
  `TMPDIR`, the XDG variables where the host defines them, and the vendor's own config variable; so Abacus needs only a
  vendor definition whose config variable is `ABACUSAI_CODING_AGENT_DIR`. The two relocations that matter (`TMPDIR`, XDG)
  are the ones the existing layer performs for every vendor.

**Residual host traces after the base layer**: none found in unauthenticated runs. An authenticated run is unmeasured
(**operator**): the keystore question (a credential given by environment should not create `keystore.enc`; to confirm),
sessions, memory and caches.

## 5. Headless use and approvals

**Evidence** (documentation `.../headless`, `.../permissions`, and the binary's own help and protocol code):

- Print mode: `abacusai -p "<prompt>"`; `--output-format text|json|stream-json`; `--input-format text|stream-json`
  (needs `stream-json` output; "the process runs until stdin closes"); `--include-partial-messages`;
  `--replay-user-messages`; `--model <id>`; `--permission-mode supervise|accept-edits|plan|auto|unsupervised`;
  `--allowed-tools`, `--disallowed-tools`, `--add-dir`, `--settings <file>`, `--mcp-config <file>`, `--no-agents-md`;
  `-c`/`--resume <id>` for conversations.
- Exit codes: 0 finished, 1 fatal (last stderr line `{"type":"fatal","reason":...}` with reasons `not_authenticated`,
  `access_denied`, `no_models`, `invalid_config`, `network_unreachable`, `startup_timeout`, `internal`), 2 a
  `UserPromptSubmit` hook blocked, 143 `SIGTERM`.
- The documented approval route is the JSON-lines protocol: "the agent can then ask for permission mid-turn and your host
  answers". The documentation does not describe the messages. **Binary** (the program's own validation code):
  - Input lines: `{"type":"user","message":{"role":"user","content":<string|blocks>},...}`,
    `{"type":"control_request","request_id":...,"request":{"subtype":...}}` and
    `{"type":"control_response","response":{"subtype":"success"|"error","request_id":...,"response":{...}}}`.
  - Output lines: a `system` message with `subtype: "init"` (carries `protocol_version`, `cli_version`, `capabilities`,
    `permission_mode`, the model), `event` messages, `result` (subtypes `success`, `error_during_execution`,
    `error_max_turns`, `error_structured_output`, `prompt_blocked`), and
    `{"type":"control_request","request_id":...,"request":{"subtype":"can_use_tool","tool_call_id":...,"request":{...},"always_allow_rules":[...]}}`.
  - The answer to a `can_use_tool` request is a `control_response` whose `response` is `{"decision": D}` with D one of
    `"accept"`, `"reject"`, `"allowAlways"`, `"allowYolo"`, `{"type":"accept_with_message","message":...}`,
    `{"type":"reject_with_message","message":...}`, `{"type":"question_answers","answers":{...}}`,
    `{"type":"allow_always_with_rules","rules":[...]}`. An `error` response, a malformed answer or a closed stdin is turned
    into a rejection ("The host could not answer the permission request for this call, so it was not run. Do not retry
    it."). The program's reasons for an approval request include `needs_approval`, `ask_rule`, `plan_approval`,
    `question`, `auto_blocked`, `dont_ask`, `denied`, `hook`.
  - Update events exist in the stream (`update_available`, `update_downloading`, `update_complete`, `update_failed`) and a
    control request `check_for_update`.
- Documented caveat: without a host answering (plain print mode), "print mode resolves permissions up front"; with a named
  mode, a call that would have asked "is declined rather than left hanging".

**Decision**: the Abacus adapter runs the program in the JSON-lines mode (`-p --input-format stream-json --output-format
stream-json --permission-mode supervise`), answers each `can_use_tool` through the existing approval bridge with `accept`
or `reject` (with a message when the user typed one) and never sends `allowAlways`, `allowYolo` or the rule variants (they
would persist a permission outside HaexVault's posture). `--no-agents-md` and `ABACUS_DISABLE_AUTO_MEMORY=1` keep host
instructions and memory out (spec 007 FR-009). The set of tools Abacus uses beyond its own is given with `--mcp-config`
(the HaexVault MCP server, as for Claude Code and Codex).

**Read from the program's own code** (the bun binary carries its JavaScript as text; no run needed): the output line
`{"type":"event","event":{...}}` carries the conversation events of the turn, among them `text_delta` (field `content`,
`subtaskId` for a sub-task), `turn_complete`, `error` (`error.message`), `credits`, `token_usage` (`inputTokens`,
`outputTokens`, ...); without `--include-partial-messages` the program merges consecutive text deltas, so the adapter passes
it; the `result` line has `subtype`, `is_error`, `result` (the whole text), `stop_reason`, `errors[]`, `token_usage`,
`credits_used`; a `can_use_tool` request carries `tool_call_id` and `request` (the call as the permission system holds it,
whose fields are not typed there); a host that closes stdin ends the program.

**Not verified (operator)**: a real approval round trip (the fields of `request` in `can_use_tool`, how a rejection with a
message reads in the transcript), that `supervise` asks for every changing tool, and the user-message block format for
attachments. Until verified, FR-010 is "satisfiable by the protocol", not "measured".

## 6. The tool's own update

**Evidence**: documentation (`.../settings`): `autoUpdate` default `auto` ("installs and relaunches"), `prompt`, `off`;
environment variable `ABACUS_AUTO_UPDATE` (`auto`, `prompt`, `off`) overrides every file; the **binary** itself sets
`ABACUS_AUTO_UPDATE=off` when it starts another copy to ask its version, which shows the variable is read at start. `abacusai
upgrade` (also `update`) installs the latest version on request.

**Decision**: the vendor definition sets `ABACUS_AUTO_UPDATE=off` for every process (the same mechanism as Claude Code's
`DISABLE_UPDATES`); HaexVault never calls `upgrade`. The `--version` output is `Abacus.AI CLI version <semver>` (measured
`Abacus.AI CLI version 2.10.23`), which is what the start check of spec 094 FR-022 compares with the installed version.
No update request was seen on the network in unauthenticated runs; an authenticated run is **operator**.

## 7. Terms

`abacus.ai/terms` (the customer terms, read 2026-11-22) does not mention automated access, scripts, wrappers, third-party
software around the CLI, API keys or the CLI itself (the CLI appears once, as a label in the pricing box). The clauses
nearest to the question forbid letting third parties use the services and circumventing "any aspect of the Services". It is
silent, neither allowing nor forbidding. Recorded in the spec's Assumptions; **decision for the operator**, as spec 007 did
for Claude Code. Abacus's own documentation describes the print and JSON-lines modes as "what you use from a script or a CI
job" and as how "the Desktop app and the SDK drive the agent", which is the use this spec makes. The sign-in endpoints of
section 3 are not documented at all, so whether a third-party client may call them is a separate question for the operator
(spec Open risks).

## 8. The tool manager: what a vendor is today (code survey)

Read from `src-tauri/src/cli_tools` (spec 094, merged):

- A vendor is data: `catalog.json` is embedded (`include_str!`) and every struct denies unknown fields.
  `VendorDefinition`: `vendor` (lowercase letters and digits), `display_name`, `download_hosts`, `executable` (platform key to
  relative path), `env_set`, `config_args`, `env_remove_prefixes`, `config_env` (required), `tmp_env`, `versions`. A version
  has `platforms`, each an `Artifact { url (https, host in download_hosts), sha256, size, kind, unpacked_size }`; versions
  are strictly descending semver. `ArtifactKind` is `file` or `tar.gz`.
- The environment is applied generically by `containment.rs::apply`: remove the host variables that start with a vendor's
  `env_remove_prefixes`, set `HOME` (and the Windows account folders), `TMPDIR`/`TEMP`/`TMP`, the vendor's `config_env`
  (the run folder), `tmp_env`, `env_set`; point an XDG variable below the vault home only where the host defines it. The
  credential is not data: an adapter injects it after `contain` (Claude: `CLAUDE_CODE_OAUTH_TOKEN` at `claude.rs:422`;
  Codex: an `auth.json` in the run folder).
- The start check (`install.rs`, around line 486) runs the program with the vendor's `config_args` and `--version` under the
  same containment; `abacusai --version` prints `Abacus.AI CLI version 2.10.23`.
- `platform.rs`: `PlatformKey` has six values, from `(os, arch)` only: `linux-x64`, `linux-arm64`, `darwin-x64`,
  `darwin-arm64`, `win32-x64`, `win32-arm64`.
- `Cargo.toml` already has `tar`, `flate2` and `zip 8` (deflate) as direct dependencies.

**What Abacus needs beyond data** (so spec 094 SC-007 / this spec's SC-007 is not literally true and is corrected there):

1. A platform key for the x64 build without AVX2 (`linux-x64-baseline`): `platform.rs` must decide it on the device and
   every lookup must fall back to the base key where a vendor has no variant entry (Claude Code and Codex have none).
   The musl variants are not offered (a desktop app on a musl host is rare; the start check reports it as "not runnable").
2. `ArtifactKind::Zip` for the Windows archives (the `zip` crate is already a dependency), with the same escape refusal
   and size accounting as `tar.gz`.
3. A per-program requirement `requires_avx2` on an artifact (default false): the vendor's `darwin-x64` and `win32-x64`
   programs have only a `-baseline` sibling that is not offered (spec 096 plan D3), so a host without AVX2 must get
   `unsupported` for them instead of a program that cannot run. Claude Code and Codex entries do not set it.
4. A vendor that has no tested version yet (`versions: []`). The catalog rejects it today ("no version listed"); the vendor
   definition must ship before its first measured version (plan D8), and the status then reads `no_tested_version`
   and the install is refused with that reason.

Everything else (install, update, revert, remove, status, progress, paths) is untouched.

## 9. The delegate layer: what a third vendor touches (code survey)

Unlike the tool manager, the delegate layer knows its vendors by a closed enum and by string matches. A third vendor adds a
variant and extends every match; a vendor trait for two-to-three vendors would be an abstraction nobody asked for.

- `DelegateVendor { Claude, Codex }` in `adapters/cli_delegate/mod.rs:150-173` (`as_str`, `parse`); the provider row's
  `adapter` column holds the string (`providers` is a synced table; no schema change is needed, `credentials` stays NULL for
  Abacus).
- Rust matches on the vendor: `adapters/cli_delegate/mod.rs` (`list_models`, `stream_chat`), `autonomy.rs`
  (`classify_autonomy_spawn_error`), `claude.rs`/`codex.rs`/`codex_models.rs`/`connect_*.rs`/`haex_mcp.rs`/`process.rs`
  (take a `DelegateVendor`), `providers/connect.rs` (`connect_cli_delegate` and string literals), `providers/add.rs`,
  `providers/mod.rs:336` (`build_adapter`), `chat/model_loading.rs:582`, `agent_profiles/delegates.rs`,
  `mcp_server/delegates.rs`, `mcp_server/registry.rs` (`AgentKind`).
- Frontend matches: `src/lib/cli-tools.ts` (`CliToolVendor`), `composables/useProviders.ts`,
  `composables/useModelInventory.ts` (`DELEGATE_VENDORS`), `components/settings/ConnectDelegateProvider.vue` (per-vendor
  state maps), `components/chat/MessageList.vue` and the i18n key `chat.model.delegate.<vendor>`,
  `lib/actions/settingsActions.ts`, `composables/useToolNotice.ts`.
- Request path: `chat/model_loading.rs` calls `build_adapter`, which builds `CliDelegateAdapter::new(vendor, credentials,
runtime, ctx)`; `stream_chat` matches the vendor and calls `spawn_claude_invocation` or `spawn_codex_app_server`.
  Approvals reach the user through `approval_bridge::request_approval(...)` (Claude via its MCP permission server over a
  socket, Codex through its app-server callback); Abacus's `can_use_tool` request is the Codex shape (a request on the
  output stream), so it calls `request_approval` directly.

## 10. The credential in the password manager (code survey)

Precedent: the marketplace login (`marketplace/credentials.rs`), an entry of the password manager reached with
`Caller::Internal { feature }` and a fixed grant `ReadWrite` on `Scope::All`.

- Create: `PasswordsService::create_owned_item(&Caller, ItemInput) -> id` (`passwords/service/owned.rs:26`); the entry gets
  `owner = feature`, and only that feature (and the user) sees an owned entry (`access.rs`, `sees_owned`). Delete:
  `delete_owned_item` (purges, owner only). Read the secret: `read_secret_item(&Caller, &[Grant], id)`
  (`service/items.rs:268`). Update: `update_item(..., expected_updated_at, patch)` with a `ReadWrite` grant, refused for
  anything but a login entry.
- There is no find-by-tag method for an Internal caller. `list_headers(caller, grants)` returns the headers an Internal caller
  may see (its own owned entries), which carry `tags`; the lookup filters that list (the marketplace's `candidates` goes
  through `remote_storage::credentials::candidates_in` for the same reason).
- `ItemInput` has `title, username, password, note, url, tags, key_values` (and more); the marketplace puts the secret in
  `password`. `PasswordsService::new(db: VaultDb)`; entries are ordinary CRDT rows and sync; a deleted entry leaves a marker
  that tells "not arrived yet" from "deleted" (the marketplace's `Login::{Ready, Missing, Syncing}`).
- ADR 0007: the secret sits as a plain value in the encrypted vault database, protected by grants; the built-in agent
  never sees it.

**Decision**: a module `providers/delegate_credential.rs` (or beside the adapter) with `FEATURE = "delegate"`,
`CALLER = Caller::Internal { feature: FEATURE }`, one tag per vendor (`haex-delegate-abacusai`), and scope-aware functions
`save(db, vendor, kind, secret, scope)`, `read(db, vendor, scope) -> Credential | Missing`, and scoped deletion.
The title is `Delegate: Abacus AI`, the secret is `password`, and the kind and scope are custom fields `kind` (`device_code`)
and `device`. Portable credentials use scope `vault`; device-bound credentials use the installation ID. Concurrent duplicate
entries in one scope resolve to newest `updated_at` and only same-scope duplicates are purged at the next save. A save on
another device never replaces an existing device-bound credential. The generic names are deliberate: the operator wants
every delegate provider's credential there (Clarifications); only Abacus uses it in this spec.

## 11. Testing without an account

Real runs need the operator (spec FR-015). Everything else runs against stand-ins: the existing stand-in release host
(`cli_tools/stand_in.rs`, `wiremock`) for the download, a shell script that prints `Abacus.AI CLI version <v>` for the
start check, and a script that speaks the JSON-lines protocol of section 5 (emits `system/init`, a `can_use_tool` control
request, reads the `control_response`, emits `result`) for the adapter, under the existing unix stub pattern in
`src-tauri/tests/cli_delegate_*.rs`. The maintainer script is tested against a recorded Sigstore bundle and the archive it
signs (the `linux-x64` 2.10.23 pair measured here), plus a tampered copy.

## 12. Measured with the real account (2026-11-24)

Linux x64, version 2.10.23, the operator's Pro account, a throwaway home below the scratchpad with an empty environment
(`CODELLM_DEVICE_CODE`, `ABACUS_AUTO_UPDATE=off`, `ABACUS_DISABLE_AUTO_MEMORY=1`, `HOME`, `TMPDIR`, `XDG_CONFIG_HOME`), the
release from `add-version.ts --measure-only` (bundle verified with `sigstore` 4.6.0). The device code was kept in a 0600 file
outside the repository and printed nowhere; no file below the throwaway folders contains it.

**3.0 Sign-in service (T025, first part).**

- A plain `curl` without a user agent is answered: HTTP 200 in 0.44 s with `verificationUri`, `userCode`,
  `pollingInterval: 5` and `expiresIn: 300`. The connect from the UI is possible.
- **Not as the quickstart assumed:** before the confirmation `_pollDeviceCode` answers `success: true` with only
  `result.chatWebUrl`, not `success: false`. After the confirmation `result` also holds `deviceCode` (confirmed between 45 s
  and 51 s after the request). The client has to treat "no `deviceCode`" as "not confirmed yet" and must not look at `success`.
- Not measured: the answer for a code that is not confirmed (the code is valid for 300 s), and the install time of SC-001 through
  the app (no catalog entry yet).

**3.1 `auth status`.** With the code: `Method: environment variable`, `Name`, `Email`, `Org`, exit 0. With a wrong code: only
`Method: environment variable`, also exit 0. So a check of the code must read the `Name`/`Email` lines; the exit code does not tell.

**3.2 A request.** `-p "say hi" --output-format json` answers in about 5 s with the model `ROUTE_LLM` (routed to "Smaug Flash
(Thinking)") and **spends credits of the plan**: 2.29 for "say hi", 2.85 for a request with a file read and a command. The usage
meter of the account showed `monthly_credits` 10523 of 30000 on the Pro tier. The run is not free; it draws on the plan's
allowance. Whether `ROUTE_LLM_LOW` or another id costs less was not measured.

**3.3 and 3.6 Files and network.**

- `home/.abacusai/`: `account`, `account-skills`, `builtin-skills` (about 265 files, 36 MB with the skills), `checkpoints`,
  `cli/<pid>.pid`, `history.jsonl`, `logs`, `projects`. No keystore file. `tmpdir`: one 12.9 MB native module
  `.bun-1000-<hash>.node`. `xdgcfg`: the folder `abacusai-cli-nodejs`. Nothing was written to the working folder except what
  the tool calls asked for.
- `strace` on file and network calls: no write with an absolute path outside the throwaway folders. Network: port 443 to
  addresses of the 13.226.244.x range, 172.66.161.95 and 104.20.35.251, and the local resolver sockets; host names were not
  recorded, and a separate update check was not identified.

**3.4 and 3.5 Approvals (T035).** With `--permission-mode supervise` and `--no-agents-md`:

- A **changing** tool asks: `bash` with `touch other.txt` and the `write` tool each sent `can_use_tool` (fields
  `subtype`, `tool_call_id`, `request`, `always_allow_rules`; `request.type` is `run_terminal` with `command` and `cwd`, or
  `write_file` with `filePath`). `accept` ran them (files created). `reject_with_message` did not run them; the tool result was
  "Tool was rejected by user: <message>" and the model reported both actions as rejected.
- A **read-only** tool does not ask: the `read` tool ran without a `can_use_tool` request for a file in the working folder and
  for `/etc/hostname` outside it, and `bash ls` ran without one. `bash cat /etc/os-release` did ask.
- Result without further settings: read-only tools bypass the bridge.
- **The enforcement path:** with `--settings <file>` holding `{"permissions":{"ask":["*"]}}` (the `ask` list of the tool's
  config, described in its bundled `doctor` reference) every call reaches the bridge: `read` in the working folder, `read
/etc/hostname` and `bash ls` each sent `can_use_tool` (`request.type` `generic` for `read`), and after
  `reject_with_message` no file was read (the host name is nowhere in the transcript). The same holds with a list of names
  (`Read`, `Grep`, `Glob`, `LS`, `Bash`, `WebFetch`, `WebSearch`) and with `*` while a `config.json` in the home asks for
  `permissionMode: "YOLO"` and allows `*`. A `config.json` in the working folder is ignored with the warning "is not a
  trusted folder" unless `ABACUS_TRUST_WORKSPACE=1` is set, which the containment removes from the host's environment. The
  adapter now writes that file in every run (plan D6), and the flag of T034 is flipped on this evidence.
- `get_model_catalog` answers with the group `abacus` and the models `ROUTE_LLM`, `ROUTE_LLM_LOW`, `CLAUDE_V5_5_OPUS`,
  `CLAUDE_V5_5_SONNET`, `GEMINI_3_8_FLASH_THINKING`, `KIMI_K3` and others; the model list can come from the account (plan D7).

**3.7 `kill -9` during a request.** The process and its children are gone. Left behind, all inside the throwaway folders:
`cli/<pid>.pid`, three `.flock` files and the `.bun-*.node` in `tmpdir`.

**Not measured here:** the device-binding check on a second machine and the SC-002 runs (T030), macOS and Windows (T043), the
connect, request and removal through the app (needs the catalog entry of T044), the cost comparison on the account page.

## 13. Host names and residuals (2026-11-26, T038)

Linux x64, version 2.10.23, the same release and account as section 12, `ABACUS_AUTO_UPDATE=off`.

- **Host names (3.6).** `strace` with the payloads of `connect`, `sendto`, `sendmsg` and `write` on `auth status` and
  `-p "say hi"` identified these external destinations: `auth status` reached `abacus.ai` (13.226.244.x), and the request
  also reached `api.abacus.ai` (13.226.244.98) and `apps.abacus.ai` (104.20.35.251, 172.66.161.95). Other names in the trace
  (`github.com`, `www.w3.org`, schema addresses) appeared in files the tool unpacked into its home.
- **Update check.** Neither run contacted the release host `static.abacus.ai` (18.64.211.x). No separate update check was
  observed with `ABACUS_AUTO_UPDATE=off`.
- **Residuals.** `check-containment.py --vendor abacusai` with an authenticated run (a wrapper in the operator's temporary
  folder put the device code into the tool's environment) for `auth status` and for a request: exit 0, "only listed
  residuals (or none) outside the vault". Neither run reported an unlisted write outside the test tree. The checker reports
  unresolved paths and allowlisted outside writes separately, and both can accompany exit 0. The recorded result is the
  exit status and summary. Section 12 records the files left after `kill -9` inside the throwaway folders. The observations
  did not identify a residual that requires an Abacus entry in
  `allowed-residuals.json` or a change to the vendor definition for this build.
- Quickstart §7 remains open under T050. It covers the app lifecycle after T044 makes the build installable, including
  host file listings and checks for credentials and log contents.

## 14. The model list from the account (2026-11-26, plan D7)

Linux x64, version 2.10.23, the same release and account as section 12. Question of plan D7: does the `get_model_catalog`
control request spend credits?

- **Method.** The account's usage meter is reported only in sessions that run a model (`system/usage_changed`, once
  before and once after the request). So each measurement brackets catalog-only sessions (the control request, no `user`
  line) between two short requests with `DEEPSEEK_V4_1_FLASH` and compares the meter after the first request with the
  meter before the second.
- **Result.** The meter rose by 0.26 credits between two requests with no catalog session between them, and by the same
  0.26 with five and with ten catalog sessions between them. The 0.26 follow every request (most likely the title the tool
  generates afterwards); the catalog request spends nothing within the meter's resolution of 0.01 credits.
- **The answer.** 71 models in one group `abacus`, each with only `id`, `name` and `origin`; nothing says what a model
  costs or whether the plan includes it. `ROUTE_LLM` ("RouteLLM") comes first, as in section 12.
- **Cost per model differs widely.** "say hi" spent 0.16 credits with `DEEPSEEK_V4_1_FLASH` and 2.3 to 2.9 with the
  default `ROUTE_LLM`.

The adapter therefore lists the account's models from the program (`abacus_models.rs`) instead of one entry for the
account default.

## 15. Ungated and the mode `init` reports (2026-11-26)

Linux x64, version 2.10.23, the same account as section 12, in a throwaway home without settings files.

- **The program's own full-autonomy mode.** Its code maps `--permission-mode unsupervised` to its internal mode `YOLO`
  ("Runs any command without asking"; `bypasspermissions` is an accepted alias). In `YOLO` a tool call is allowed without
  a request unless an ask rule, an ask hook or a critical removal applies, or the tool is `ask_user_question`.
- **Measured.** Started with `--permission-mode unsupervised` and no `--settings`, a prompt asking to read a file, run
  `ls`, write a file and run `touch` ran all four without a single `can_use_tool` request; the files were created.
- **What `init` reports.** `permission_mode` is the internal name, not the flag: `YOLO` for `unsupervised`, and `DEFAULT`
  for `supervise` (all seven runs of section 12). The adapter compared it with `supervise`, so every request in the app
  ended with "did not start in its approving mode"; the stub tests had used the flag name.

**Decision**: `Ungated` (spec 009) runs `--permission-mode unsupervised` without the ask-rule settings file, as Claude
Code's `bypassPermissions` and Codex's `approvalPolicy: "never"` do. A request the program still sends is rejected and
never reaches the approval bridge (`Ungated` has no way to ask, as with Claude Code). The `init` check compares with
`DEFAULT` for `Standard` and `YOLO` for `Ungated`. FR-010 is unchanged: under a posture that asks, nothing weaker is
offered; `Ungated` is the person's own posture, not a fallback. `GatedPermissive` stays unavailable until its deny rules
are mapped to Abacus' tool names and inputs.

**Since spec 102**: `Ungated` does pass a settings file again, with deny rules only: every start blocks Abacus' bundled
browser (`agent-browser`), in every mode (spec 102 research R3/R4). It holds no ask rules, so the paragraph above still
holds for approvals.
