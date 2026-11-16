# Phase 0 Research: CLI tools per vault

Facts below were read from the vendors' own pages and release hosts or measured on this machine on 2026-11-16. Where a
fact was **not** verified, it says so. Measurements marked "downloaded" used the programs fetched from the release hosts
(Claude Code 2.1.311, Codex 0.169.0) and checked against their published checksums; the others used the installed host
tools `claude 2.1.305` and `codex-cli 0.147.0`.

## 1. Claude Code: source and integrity

**Decision**: Download the single program file from
`https://downloads.claude.ai/claude-code-releases/<version>/<platform>/claude` (`claude.exe` on Windows) and check its
SHA-256 against a value held in holzi's tested-version list. The list value is taken from the release's `manifest.json`
after its GPG signature was verified by a maintainer script, not by the app.

**Evidence**:

- The vendor's setup page documents `manifest.json` per release with `platforms.<platform>.checksum` (SHA-256) and `size`,
  and a detached signature `manifest.json.sig` for releases from 2.1.89 on, signed by the key with fingerprint
  `31DD DE24 DDFA B679 F42D 7BD2 BAA9 29FF 1A7E CACE` (published at `https://downloads.claude.ai/keys/claude-code.asc`).
  Source: `code.claude.com/docs/en/setup`, section "Binary integrity and code signing".
- Verified here: the key imported into an empty keyring, `gpg --verify` on `2.1.311/manifest.json` reports
  `Good signature from "Anthropic Claude Code Release Signing"` with that primary fingerprint.
- The vendor's installer (`https://claude.ai/install.sh`) builds the URL exactly as above, reads `latest`/`stable` from
  `<base>/latest|stable`, takes `checksum` and `size` from the manifest, and compares SHA-256 after the download. It also
  offers a `claude.zst` variant (not used: no zstd crate declared).
- Verified here: the downloaded `2.1.311/linux-x64/claude` (293 776 752 bytes) has SHA-256
  `86dc1fcc…8489ada`, equal to the manifest value.
- Platforms in the manifest: `darwin-arm64`, `darwin-x64`, `linux-arm64`, `linux-x64`, `linux-arm64-musl`,
  `linux-x64-musl`, `win32-arm64`, `win32-x64`. Sizes 276–299 MB. Channel pointers on 2026-11-16: `stable` = 2.1.311,
  `latest` = 2.1.325.
- Platform code signatures: macOS signed and notarized, Windows Authenticode, Linux not individually signed (manifest only).

**Verified (operator approved the run, 2026-11-16)**: the downloaded standalone file starts from an arbitrary folder in
an empty environment: `--version` prints `2.1.311 (Claude Code)`, exit 0. `claude doctor` reports
`Auto-updates: disabled (set by env: DISABLE_UPDATES)` with the variable set in the process environment, and
`Last update attempt: none recorded`. It also prints install-method hints ("run `claude install`") that holzi ignores:
holzi never runs `claude install`, which would write a launcher and profile changes. An unauthenticated `-p` run returns
`Not logged in · Please run /login` and writes, outside the test tree, only the inbox socket (section 3).

**Rationale**: The signature check needs a PGP implementation. Doing it in the maintainer script keeps a PGP crate out of
the app (spaex: no new dependency where avoidable) and keeps the install independent of the vendor's server at run time
(spec FR-006, FR-009).

**Alternatives rejected**: `npm install` (needs host Node, spec FR-005); the vendor's `install.sh`/`claude install` (writes
`~/.local`, `~/.claude`, edits shell profiles: the opposite of the goal); apt/dnf/apk/brew (need root or the host's
package manager); `latest` channel (untested versions, spec FR-009).

## 2. Codex: source and integrity

**Decision**: Download `codex-package-<target>.tar.gz` from the GitHub release `rust-v<version>` of `openai/codex`, the
asset the vendor's own installer uses, and check SHA-256 against the tested-version list. The list value is taken by a
maintainer script from `codex-package_SHA256SUMS` and cross-checked against the SHA-256 `digest` the GitHub API shows for
the asset.

**Evidence**:

- Release `rust-v0.169.0` (2026-11-13) has 194 assets. Per target `codex-package-<target>.tar.gz` (and a `.tar.zst`),
  for `x86_64|aarch64-apple-darwin`, `x86_64|aarch64-pc-windows-msvc`, `x86_64|aarch64-unknown-linux-musl`, plus
  `codex-package_SHA256SUMS` listing each package archive. Linux musl packages also have a `.sigstore` bundle; other
  targets have none.
- `install.sh` (a release asset) prefers the "package" layout when `codex-package-<target>.tar.gz` and the SUMS file exist,
  verifies the archive against the SUMS file, and verifies the SUMS file against the GitHub asset digest.
- `codex-package-x86_64-unknown-linux-musl.tar.gz` (192 339 692 bytes) downloaded: its SHA-256 equals the
  `codex-package_SHA256SUMS` value (`3b8ed091…71ba0`). Unpacked into an empty folder it is **475 MB**: `bin/codex`
  (294 MB), `bin/codex-code-mode-host` (70 MB), `bin/codex-exec-server` (74 MB), `codex-package.json`, `codex-path/rg`,
  `codex-resources/bwrap`, `codex-resources/voice/` (a host program and GStreamer libraries).
- `codex-package.json` names the layout: `{"layoutVersion":1,"version":"0.169.0","target":"x86_64-unknown-linux-musl",
"variant":"codex","entrypoint":"bin/codex","execServer":"bin/codex-exec-server","resourcesDir":"codex-resources",
"pathDir":"codex-path"}`; holzi runs `entrypoint`.
- Linux assets exist for musl only: they are statically linked, so one asset serves glibc hosts.

**Verified (operator approved the run)**: the unpacked package starts from an arbitrary folder in an empty
environment: `bin/codex --version` prints `codex-cli 0.169.0`, and `bin/codex app-server --stdio` answers `initialize` and
`thread/start`. **Not verified**: whether `app-server` needs `codex-path/rg`, `codex-resources/bwrap` or the other `bin/`
programs during a real turn, and whether the bare `codex-<target>.tar.gz` (118 MB gzip) would do. The plan follows the
vendor's own layout (the whole package).

**Finding, plugin sync**: on start `codex app-server` clones a plugin catalog from GitHub with the host's `git` into
`$CODEX_HOME/.tmp/plugins-clone-*` (about 25 MB, 751 file operations). With a fresh `CODEX_HOME` per run this would repeat
at every run: a download from a third party and a dependency on a host program. Starting with
`--disable plugins --disable plugin_discovery_prefetch --disable recommended_plugins --disable remote_plugin` (feature flags
of the shipped `config-schema.json`) removes it: no clone, no `git`, the per-run state drops from 26 MB to 1.2 MB. The flags
are part of the Codex vendor definition (`config_args`); whether any of them changes behaviour that holzi's own
delegate use needs (its MCP server is configured in `config.toml`, not as a plugin) is a check in the quickstart.

**Weakness stated**: the SUMS file and the API digest both come from GitHub, so they are not independent of each other.
The maintainer script additionally verifies the `.sigstore` bundle with `cosign` where one exists (Linux musl); macOS and
Windows packages rest on the two GitHub-served values.

**Alternatives rejected**: `npm`/`pip` packages (host tooling); `install.sh` (writes under `~/.codex` and `~/.local/bin`,
edits shell profiles, starts an updater); bare binary (need for `rg`/`bwrap` unverified).

## 3. Are the tools' writes contained by `HOME`? (measured)

Method: each tool run unauthenticated under `strace -f -y` in an empty directory tree, with an environment cleared to
`PATH` plus the variables under test, then every path created, written, renamed or removed outside that tree listed
(a small analysis script that resolves directory fds; it lives in the planning session's scratch folder and becomes
`scripts/cli-tools/check-containment.*` in `tasks.md`, so a new tool version can be measured again before it enters the
tested-version list).

| Tool                                                  | Variables set                                                                  | Writes outside the tree                                                                                |
| ----------------------------------------------------- | ------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------ |
| Claude Code `-p`                                      | `HOME`, `CLAUDE_CONFIG_DIR`                                                    | `/tmp/claude-<uid>/fswatch-probe-*` (created, removed), `/tmp/cc-socks/<pid>.sock` (removed)           |
| Claude Code `-p`                                      | + `TMPDIR`                                                                     | `/tmp/cc-socks-<uid>/<pid>.sock` (removed at exit; the folder stays)                                   |
| Codex `app-server --stdio` (initialize, thread/start) | `HOME`, `CODEX_HOME`                                                           | none (two `mkdir newroot/oldroot` without a resolvable parent: the sandbox helper's private namespace) |
| Codex, same                                           | + `TMPDIR`                                                                     | same as above                                                                                          |
| Claude Code 2.1.311 downloaded, `-p`                  | `HOME`, `CLAUDE_CONFIG_DIR`, `TMPDIR`, `CLAUDE_CODE_TMPDIR`, `DISABLE_UPDATES` | `/tmp/cc-socks-<uid>/<pid>.sock` only (removed at exit)                                                |
| Codex 0.169.0 downloaded, `app-server`                | `HOME`, `CODEX_HOME`, `TMPDIR`, `-c check_for_update_on_startup=false`         | none outside the tree; inside `CODEX_HOME` a 25 MB plugin clone via `git` (see section 2)              |
| Same, with the four `--disable` plugin flags          | same                                                                           | none outside the tree; per-run state 1.2 MB                                                            |

Findings:

1. Claude: with `HOME`, `CLAUDE_CONFIG_DIR` and `TMPDIR` set, `HOME` stays empty and all state lands in the config dir
   (`.claude.json`, `projects/`, `sessions/`, `backups/`). `CLAUDE_CODE_TMPDIR` is the documented override of its
   internal temp folder (it appends `/claude-<uid>/`); the plan sets it too.
2. Claude binds an inbox socket for `-p` sessions (documented: `code.claude.com/docs/en/cross-session-messaging`,
   "Non-interactive sessions"). The directory is `/tmp/cc-socks-<uid>` and is not moved by `TMPDIR`. Only bare mode skips
   it, and bare mode does not read subscription logins (spec 007 research §3). This is a **residual host trace** with no
   vault-specific content (named by process id).
3. Codex honours `HOME` (`home/.cache` appeared in the empty home) and `CODEX_HOME` (`state_5.sqlite`, `logs_2.sqlite`,
   `skills/`, `shell_snapshots/`, `installation_id`). Its `config-schema.json` has `log_dir` (defaults to
   `$CODEX_HOME/log`) and `history` (`~/.codex/history.jsonl`).
4. Linux: `XDG_CONFIG_HOME`, `XDG_CACHE_HOME`, `XDG_DATA_HOME`, `XDG_STATE_HOME` and `XDG_RUNTIME_DIR` from the host, if
   set, take precedence over `HOME`-derived defaults. The measurement ran with them cleared. The base layer therefore
   points such a variable below the vault home **only where the host defines it** and leaves it unset otherwise, so the
   change is the smallest one that a host variable would not defeat (operator, 2026-11-16: no reliance on the host
   leaving variables unused, and no blanket override of them).
   Neither Claude Code (`claude --help`: `--settings`, `--debug-file`, `--mcp-config`, `--plugin-dir`, no state or
   config-dir flag) nor Codex (`-c log_dir`, `-c sqlite_home` only; config, auth, skills and snapshots stay in
   `CODEX_HOME`) can relocate its state without an environment variable, so for these two the variables
   `CLAUDE_CONFIG_DIR` and `CODEX_HOME`, set for the child alone, remain the only mechanism.
5. Windows tools read `USERPROFILE`, `APPDATA`, `LOCALAPPDATA`, not `HOME`; macOS tools use `HOME` and `TMPDIR`.
   Not measured here (Linux only).

**Limits of the measurement**: unauthenticated, no model turn, no tool use inside a turn (shell, patch), host versions.
An authenticated run with a tool call is a manual quickstart step and an automated check with the stub delegates.

## 4. Switching the tools' own updates off

- Claude Code: `DISABLE_UPDATES=1` "blocks all updates including manual `claude update` and `claude install`"; stricter than
  `DISABLE_AUTOUPDATER=1` (documented on `code.claude.com/docs/en/setup` and the env-vars page). The setup page shows it
  in settings `env`; it is listed as an environment variable. That a variable in the process environment is honoured was
  measured with the downloaded 2.1.311: `claude doctor` prints `Auto-updates: disabled (set by env: DISABLE_UPDATES)`.
- Codex: config key `check_for_update_on_startup` ("Set to `false` only if your Codex updates are centrally managed"),
  settable per run as `-c check_for_update_on_startup=false`. holzi never runs the vendor's installer, so the standalone
  updater and its `app-server daemon` are not installed. Measured with the downloaded 0.169.0: the flag is accepted by
  `app-server`, and no update file appeared in the tree. Only file operations were traced, so network calls are not
  covered by this measurement.
- Decision: both are part of the vendor definition (`env_set`, `config_args`), applied on every start (spec FR-008).

## 5. Where things live

- Folders: `<vault folder>/cli-tools/<vendor>/<version>/` and `<vault folder>/home/`, added to `VaultPaths`
  (`instances/vault_paths.rs`) as `cli_tools(name)` and `home(name)`. The CI check `scripts/ci/check-vault-paths.py`
  fails on a `.join("<name>")` outside `vault_paths.rs` for the names it lists; `cli-tools` and `home` are added to its
  name list, with a test.
- State is the file system, not the database. `providers` is a CRDT table, so a row would reach other devices while
  the install exists on one (spec FR-018). A tool counts as installed when `<version>/install.json` exists
  (written last, before the staging folder is renamed).
- The provider row's `base_url` stays unread for delegates. It is a synced column; a per-device path must never go in it.
  `add.rs` stops requiring it for `cli_delegate`, `connect.rs:370` stops writing the vendor name there, and
  `providers/mod.rs:298` stops reading it. No migration (holzi has no users).

## 6. Download, verify, unpack

- **Download**: `models::download::download_to_file` already does `.part` files, Range resume with `If-Range`, retries with
  backoff, a 60 s read timeout, throttled progress. It has no cancellation token and no checksum; the caller wraps it in
  `gate.run` with a `tokio::select!` on a cancel token, as `models/commands.rs:491` does for models. reqwest 0.13 with
  `rustls, stream` is declared.
- **Hash**: `models::hash::sha256_file` (blocking; `spawn_blocking`), `sha2 0.11`.
- **Free space**: the `sysinfo` check of `models/download_check.rs` before the download.
- **Unpack**: Claude's file needs no unpacking. Codex needs `.tar.gz`. `zip` is the only archive crate declared; `tar 0.4.46`
  and `flate2 1.1.10` are already in `Cargo.lock` (transitive), so they become direct dependencies without growing the tree.
  `flate2` with the pure-Rust backend. `tar`'s `Entry::unpack_in` refuses paths that escape the target; entries that are
  links or devices are refused too.
- **Staging and atomic publish**: unpack into `<vault>/cli-tools/<vendor>/.<version>.<uuid>.staging/`, write `install.json`
  last, then rename to `<version>/` (one rename, same file system). Leftover `*.staging` folders are removed when the vault
  opens (same pattern as `models/import.rs` `cleanup_staging_in_dir` and the `.moving-` prefix of spec 084).
- **Concurrency**: one install per vendor and vault: the publication lock pattern of `models/paths.rs` (in-process mutex plus
  file lock, cancellable by the vault token).
- **One version in use, an older one kept only as fallback** (operator, 2026-11-16): the version in use is always the
  highest complete one. An update first passes a **smoke check** inside the staging folder (the vendor's program run with
  `--version` under the same containment as a real run; it must exit 0 and print the version): on failure the staging
  folder is deleted and the previous version is untouched. After the rename the new version is _unconfirmed_ and the old
  one stays on disk. The new version becomes _confirmed_ when the first delegate request or connect flow that uses it
  ends normally (the adapter reaches its regular end: Claude's `result` event, Codex's completed turn); that writes an
  empty `<version>/.confirmed` and **then** the older versions are deleted. A stop by the user, a missing login or a
  network error does not confirm. While the newest is unconfirmed the UI offers "go back to <old version>", which deletes
  the newest and leaves the old one in use. An automatic fallback at run time was considered and rejected: a failing run
  cannot be told reliably from a user or network problem, and a silent switch to an older version would hide a broken update.
  Each run resolves its version at its start, so an update never disturbs a run (FR-010). Where a program cannot be
  deleted because a child still runs it (Windows), the deletion is repeated at the next vault open. Disk cost: two
  versions of one tool exist only between an update and its first successful run (at most about 1 GB for Codex).

## 7. The delegate spawn path (from the code survey)

- Binary name flows `provider.base_url` → `CliDelegateAdapter.binary` → `Command::new` at `claude.rs:229`, `codex.rs:35`;
  the connect flows hard-code `"claude"` / `"codex"` (`providers/connect.rs:117,139`). All four spawn sites change.
- Per-run folders come from `TempDir::new()` (OS temp): `claude.rs:343`, `codex.rs:336`, `connect_claude.rs:299`,
  `connect_codex.rs:89`. Each folder is both the config dir and the working directory and holds attachments, the system
  prompt, the MCP config, the approval socket and (Codex) `auth.json`, the raw credential. They move to
  `<vault>/home/run/<short-id>/`. `kill_all` followed by process end skips `Drop`, so today's temp folders can leak on a
  vault close; run folders in the vault folder are swept when the vault opens.
- No delegate child's environment is touched apart from three variables (`env_remove`/`env_clear` are not used), so host
  `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `*_BASE_URL`, `CODEX_*`, `CLAUDE_CODE_*` reach the tool and can change which
  account answers. Spec 007 FR-009 (the host does not influence a request) needs exactly those removed, in the child only.
  The vendor definition lists the prefixes. Proxy, certificate, `PATH` and locale variables are deliberately left as the
  host has them: clearing them would break the tool behind a corporate proxy or with a custom certificate store.
- **Helper hazard**: Claude spawns `holzi --mcp-agent …` as an MCP server; it finds holzi's MCP endpoint through
  `dirs::runtime_dir()` / `TMPDIR` / `dirs::cache_dir()` (`mcp_server/endpoint.rs:44-62`), the very variables the plan
  overrides. Without a change the helper would look in the wrong folder. The endpoint folder is passed to the helper
  explicitly in the MCP config arguments instead of being recomputed from the environment (`HAEX_MCP_ENDPOINT_DIR`
  per the repo's naming rule).
- **Socket path length**: `approval.sock` lies in the run folder; a vault name may be 64 characters and the data folder
  about 46, already above the 104-byte limit (`MAX_SOCKET_PATH_BYTES`). `endpoint.rs:49-62` solves this for the MCP
  endpoint by falling back to `/tmp/holzi-mcp-<hash>`. The same fallback is needed for the approval socket and is, like
  Claude's inbox socket, a transient system-temp file without vault content (Open decisions, D1).
- `DelegateChatContext` is a six-field struct literal in about 12 integration test files; a new field breaks all of
  them. The plan therefore replaces the `binary: String` argument of `CliDelegateAdapter::new` by one value
  `DelegateRuntime { executable, home, run_root }`, with `DelegateRuntime::for_stub(path, dir)` for the tests that pass a stub script.

## 8. Logging

- File logging exists only in debug builds (`lib.rs:369-379`, `tauri_plugin_log`, one 40 kB file for the device). No
  `tracing::` call sites. The only `log::` in `adapters/cli_delegate/` is `connect_codex.rs:168`, which prints the raw
  stderr of `codex login` (can hold the device code). `providers/connect.rs:386` interpolates a full error that can quote a
  provider's answer; `providers/refresh_all.rs:26` already logs only the provider id for that reason.
- Decision: new code logs no vault name, path, tool name or request text through `log::`. Records of install, update,
  connect and run go to a capped append-only file in the vault's own home folder (`home/records/tools.log`, rotated at
  64 kB). The two log lines above are changed to log a fixed text. Cleanup logging in `instances/cleanup.rs:20,68` prints
  vault paths already and belongs to the audit of other places (spec assumption), not to this feature.
- The runtime switch for the debug log (operator request, 2026-11-16) is a separate spec; it must respect FR-019.

## 9. Frontend

- The place is `src/components/settings/ConnectDelegateProvider.vue` (route `ai.providers`): it shows no install state
  today, only connect state and toasts. A per-vendor tool row is added: state (not installed, installed version,
  update available, untested version, reinstall, not runnable), buttons install / update / remove, a progress bar.
- Constitution (spaex, UI feedback): a failed install, update or removal is a `toast.error` with a fixed id; no inline
  success text (the new state is visible in the row); the progress bar is the only transient state.
- Progress events: a new `cli-tool-progress` event on the pattern of `model-download-progress`; one listener in a
  composable `useCliTools`.
- The model picker's "not connected" entry (`useModelInventory.ts:124-160`) gets a "not installed in this vault" variant.
- **Not** added to the settings action catalog (`settings.delegate.*`): an install runs a download from the internet
  and makes the file runnable; only a user's click starts it, never a model's tool call.
- Error text path: `useErrorString.ts` special-cases only one `adapter start:` reason; "not installed in this vault" gets
  its own mapping so the user sees an actionable message (spec FR-004).

## 10. Testing

- Unit tests in `*_tests.rs` beside each module (repo rule). Catalog parsing, platform key mapping, `install.json` fit
  checks, staging cleanup, tar escape refusal, env building.
- Download tests against `wiremock` (declared dev-dependency): a fake release host serving a tiny archive and a wrong
  checksum (SC-005), a cut-off connection (SC-004 step tests by dropping the future at each phase).
- Integration tests with the existing stub scripts (`src-tauri/tests/cli_delegate_*.rs`, unix): a stub that writes to
  `$HOME`, `$XDG_*`, `$TMPDIR`, `$CODEX_HOME`/`$CLAUDE_CONFIG_DIR` and prints its environment; the test lists the file tree
  outside the vault folder before and after (SC-003) and greps device-wide files for the vault name (SC-009).
- E2E: a stand-in release host like `models/stand_in.rs` (`HAEX_CLI_TOOLS_STAND_IN`), so the e2e suite installs a tiny
  fake tool without internet. No CI job installs the real tools; real-tool checks stay manual (quickstart).
- CI is Linux only: macOS and Windows paths (`TMPDIR`, `USERPROFILE`/`APPDATA`, `.exe`, executable bit) are not covered by
  CI; the platform module is unit-tested with injected `(os, arch)` values, the rest is manual.
