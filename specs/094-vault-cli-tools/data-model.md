# Data Model: CLI tools per vault

No database tables. State is the catalog (shipped data) and the vault folder's file tree.

## Catalog (shipped with holzi: `src-tauri/src/cli_tools/catalog.json`)

**VendorDefinition**

| Field                 | Type                                              | Meaning                                                                                                                                                                                                                |
| --------------------- | ------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `vendor`              | `"claude"` or `"codex"`                           | Same id as `DelegateVendor::as_str` and the provider row's `adapter`.                                                                                                                                                  |
| `display_name`        | string                                            | Shown in the UI.                                                                                                                                                                                                       |
| `executable`          | per-platform relative path inside an installation | e.g. `claude` / `claude.exe`; for Codex the `entrypoint` of `codex-package.json` (`bin/codex`, `bin\codex.exe`).                                                                                                       |
| `env_set`             | list of name/value                                | Always set on the tool: Claude `DISABLE_UPDATES=1`; Codex none.                                                                                                                                                        |
| `config_args`         | list of strings                                   | Always passed: Codex `-c check_for_update_on_startup=false --disable plugins --disable plugin_discovery_prefetch --disable recommended_plugins --disable remote_plugin` (research §2: no plugin clone, no host `git`). |
| `env_remove_prefixes` | list of strings                                   | Host variables removed before the start: Claude `ANTHROPIC_`, `CLAUDE_CODE_`, `CLAUDE_CONFIG_DIR`; Codex `OPENAI_`, `CODEX_`. holzi sets its own after removal.                                                        |
| `versions`            | list of **CatalogVersion**, newest first          | The tested-version list.                                                                                                                                                                                               |

**CatalogVersion**: `version` (semver string), `platforms` (map platform key → **Artifact**).

**Artifact**: `url` (https), `sha256` (64 lowercase hex), `size` (bytes, the download), `kind` (`"file"` or `"tar.gz"`),
`unpacked_size` (bytes, for the space check; absent for `file`).

**Platform keys**: `linux-x64`, `linux-arm64`, `darwin-x64`, `darwin-arm64`, `win32-x64`, `win32-arm64`. A vendor with no
artifact for a key is "unsupported on this device" (no install offered).

**Validation** (checked by a unit test over the shipped file and by `add-version.ts`): versions strictly descending by
semver; every `sha256` valid; every `url` https and on the vendor's host; every vendor has at least one version.

## Tool installation (on disk, per vault and device)

```text
<vault folder>/cli-tools/<vendor>/
├── <version>/                 # complete: contains install.json
│   ├── install.json
│   ├── .confirmed             # empty; created when a run with this version ended normally (spec FR-022)
│   └── ...                    # the program (raw file or unpacked package)
├── .<version>.<uuid>.staging/ # an attempt in progress or interrupted; swept at vault open
└── .install.lock              # file lock, one install per vendor and vault
```

**install.json** (written last, inside the staging folder, before the rename):

```json
{
  "schema": 1,
  "vendor": "claude",
  "version": "2.1.311",
  "platform": "linux-x64",
  "sha256": "…",
  "size": 293776752,
  "executable_size": 293776752,
  "installed_at": "2026-11-16T14:00:00Z"
}
```

**Complete** means: a non-dot folder `<version>/` with a parsable `install.json` whose `vendor`, `version` match the folder
and whose executable exists with exactly `executable_size` bytes (a cheap check at each start; no hash is read). **Fits this device** means `platform` equals the host's platform key. **In use** is the highest
complete, fitting version. **Confirmed** means `.confirmed` exists. At most two complete versions coexist: the newest
(unconfirmed) and the one before it; when the newest is confirmed, the older ones are deleted (repeated at the next vault
open where a running program blocks the deletion).

## Vault home

```text
<vault folder>/home/             # HOME of every delegate process
├── .config/ .cache/ .local/share/ .local/state/   # defaults derived from HOME; XDG_* point here only if the host defines them
├── run/<short-id>/              # one folder per delegate run or connect flow: config dir, cwd, attachments, system prompt,
│                                #   MCP config, approval socket; removed at the end of the run, swept at vault open
└── records/tools.log            # capped (rotated at 64 kB) record of install, update, connect, run events
```

`<short-id>` is 8 hex characters so that `run/<short-id>/approval.sock` is as short as it can be. If the whole path still
exceeds the socket limit, the approval socket alone falls back to a random-named file in the system temp folder (plan, D1).

## DelegateRuntime (in memory)

| Field        | Meaning                                                                                                                               |
| ------------ | ------------------------------------------------------------------------------------------------------------------------------------- |
| `executable` | Absolute path of the resolved installation's program (version chosen at the start of the run: the highest complete, fitting version). |
| `home`       | `<vault folder>/home/`                                                                                                                |
| `run_root`   | `<vault folder>/home/run/`                                                                                                            |
| `vendor`     | the vendor definition's `env_set`, `config_args`, `env_remove_prefixes`                                                               |

`DelegateRuntime::for_stub(path, dir)` builds one for tests from a stub script and a temporary directory.

**Process environment of the base layer (`EnvContainment`)**. All of it applies to the child process alone; holzi's own
environment and every other process on the host are untouched.

1. Start from the host environment unchanged.
2. Remove the vendor's `env_remove_prefixes`: only variables that decide which account or endpoint the tool talks to
   (Claude: `ANTHROPIC_`, `CLAUDE_CODE_`; Codex: `OPENAI_`, `CODEX_`), so that a request is answered by the connected
   subscription and not by whatever the host exports (spec 007 FR-009).
3. Set what the measurements showed must move: `HOME` (unix) or `USERPROFILE`, `APPDATA`, `LOCALAPPDATA` (windows) to
   `home`; `TMPDIR` (`TEMP`, `TMP` on windows) to `<run>/tmp`; the vendor's relocation variable (`CLAUDE_CONFIG_DIR`,
   `CODEX_HOME`) to `<run>`; `CLAUDE_CODE_TMPDIR` for Claude; the vendor's `env_set` (`DISABLE_UPDATES=1` for Claude).
4. Set an `XDG_*` variable (`XDG_CONFIG_HOME`, `XDG_CACHE_HOME`, `XDG_DATA_HOME`, `XDG_STATE_HOME`, `XDG_RUNTIME_DIR`) only
   where the host defines it, pointing it below `home`; where the host leaves it unset it stays unset and the tool
   derives its default from the new `HOME`. This keeps the change to what a host variable would otherwise defeat.
5. Never touched: `PATH`, locale, proxy variables (`HTTP_PROXY`, `HTTPS_PROXY`, `NO_PROXY` and lowercase forms),
   certificate variables (`SSL_CERT_FILE`, `SSL_CERT_DIR`, `NODE_EXTRA_CA_CERTS`, `CURL_CA_BUNDLE`), terminal variables.
   A unit test asserts that these survive.
6. holzi's own components get what they need explicitly, not through the environment: the MCP helper receives
   `--endpoint-dir <path>` in the generated MCP config.

For Claude Code and Codex steps 3 and 4 are the only way to relocate their state (neither has a flag for it), which is
why the base layer reports `best effort` and the residue is measured, not assumed away.

## Tool status (sent to the UI)

`CliToolStatus { vendor, display_name, state, installed_version?, latest_listed_version?, download_bytes?, unpacked_bytes?, untested: bool, confirmed: bool, previous_version?, isolation, reason? }`
(`isolation`: `best_effort` or `enforced`, from the `Containment` in use, spec FR-020/FR-021; `previous_version` is set while
the newest version is unconfirmed and an older one is still on disk, which is when the UI offers "go back") with
`state` one of: `not_installed`, `installed`, `update_available`, `installing`, `needs_reinstall` (`reason`: `platform`,
`incomplete`), `not_runnable` (`reason`: the start error), `unsupported` (no artifact for this device). Derived on demand
from the catalog and the file tree; held only in memory for `installing`.

## State transitions

```text
not_installed --install--> installing --ok--> installed
                               |--fail/cancel--> not_installed (or previous state if an update)
installed --newer listed--> update_available --update--> installing --start check fails--> installed(previous, unchanged)
                                                           --start check ok--> installed(new, unconfirmed; previous kept)
installed(new, unconfirmed) --first run ends normally--> installed(new, confirmed) [older versions deleted]
installed(new, unconfirmed) --go back--> installed(previous) [new deleted]
installed --platform differs / file missing--> needs_reinstall --install--> installing
installed --remove (no live child)--> not_installed
```
