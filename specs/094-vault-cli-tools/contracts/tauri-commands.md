# Contract: Tauri commands and events for CLI tools

All commands work on the **open vault** of the calling window (the vault gate supplies the vault folder); the frontend
never sends a path (spec 084 FR-025 rule, as for the other managed files). All are unavailable where the `cliDelegates`
capability is false (Android, iOS): they return `backend unavailable: platform: CLI delegates are not available on this
device`, the same reason the delegates give. None is added to the settings action catalog.

## Commands

### `cli_tools_status() -> CliToolStatus[]`

One entry per vendor of the catalog, in catalog order. Reads the file tree; no network. Each entry carries `isolation` (`best_effort` or `enforced`, from the `Containment` in use). Never fails for a damaged
installation: that is a state (`needs_reinstall`, `not_runnable`).

### `cli_tools_install(vendor: string, version?: string) -> CliToolStatus`

Installs the newest listed version for this device. The `version` argument (a listed version) exists for tests and the quickstart; the UI never sends it. Used for the first install and for an
update (an update is an install of a newer listed version; the old one stays until the new one is confirmed, spec FR-022). Resolves when the
installation is published or the attempt failed. A further update is refused while the current version is unconfirmed
or an older fallback is still pending deletion because a run uses it. Progress arrives as `cli-tool-progress` events
while it runs.

Errors (stable machine codes in the error text, mapped to messages by `useErrorString`):

| Code                             | When                                                                                                                                                |
| -------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| `cli_tools.unknown_vendor`       | `vendor` not in the catalog                                                                                                                         |
| `cli_tools.smoke_failed`         | the unpacked program did not pass its start check (exit status or reported version); the attempt is discarded and the previous version stays in use |
| `cli_tools.unknown_version`      | `version` not on the list                                                                                                                           |
| `cli_tools.unsupported_platform` | no artifact for this device's platform key                                                                                                          |
| `cli_tools.unconfirmed_update`  | the installed version has not yet been confirmed or reverted, so another version cannot replace its rollback slot                                    |
| `cli_tools.version_in_use`      | a previous version is still used by an active run, so its pending deletion must finish before another update                                         |
| `cli_tools.locked`               | another install of this vendor runs in this vault                                                                                                   |
| `cli_tools.no_space`             | free space below the artifact's `unpacked_size` plus the download                                                                                   |
| `cli_tools.offline`              | the release host cannot be reached                                                                                                                  |
| `cli_tools.server_error`         | the release host answered with an error status (status in the text)                                                                                 |
| `cli_tools.integrity_mismatch`   | the SHA-256 of the download differs from the list; nothing is unpacked or run                                                                       |
| `cli_tools.unpack_refused`       | the archive holds a path that escapes the target, or a link or device entry                                                                         |
| `cli_tools.cancelled`            | `cli_tools_cancel` was called, or the vault closed                                                                                                  |

### `cli_tools_cancel(vendor: string) -> void`

Cancels a running install of that vendor in this vault. Idempotent. The `.part` file and the staging folder are removed;
a previous installation is untouched.

### `cli_tools_revert(vendor: string) -> CliToolStatus`

While the newest installed version of that vendor is unconfirmed and an older complete version exists: deletes the newest
and leaves the older one in use. Fails with `cli_tools.nothing_to_revert` otherwise, and with `cli_tools.in_use` while a
delegate child of that vendor is live in this vault. holzi never calls this by itself (spec FR-022).

### `cli_tools_remove(vendor: string) -> CliToolStatus`

Removes all installed versions of that vendor in this vault (the home folder and other vendors are untouched). Fails with
`cli_tools.in_use` while a delegate child of that vendor is live in this vault. The stored credential is unaffected
(spec 094 US4).

## Events (emitted to the app, one listener in `useCliTools`)

### `cli-tool-progress`

```json
{
  "vendor": "claude",
  "phase": "downloading",
  "bytesDone": 1048576,
  "bytesTotal": 293776752
}
```

`phase`: `downloading` (bytes), `verifying`, `unpacking`, `publishing` (the last three carry `bytesTotal = 0`). At most one
event per 100 ms per vendor while downloading. Names follow the `model-download-progress` pattern; the payload type is
generated by ts-rs with a domain prefix (`CliToolProgress`).

### `cli-tool-changed`

```json
{ "vendor": "claude" }
```

Emitted after an install, a removal, a revert, a confirmation or the vault-open sweep changed what `cli_tools_status` would return, so every open
window of the same vault refreshes its row.

## Delegate-side contract (no new command)

- `CliDelegateAdapter::new(vendor, credentials, runtime: DelegateRuntime, context)` replaces the `binary: String`
  argument. When no complete, fitting installation exists the adapter fails before starting anything with
  `AdapterError::Unavailable { reason: "not installed in this vault: <vendor>" }` (or `"needs reinstall: <vendor>"`), which
  the frontend maps to an actionable message with the install action (spec FR-004, FR-013). There is no fallback to any
  other copy and none to another backend (spec 007 FR-008).
- The connect flows (`start_claude_connect`, `run_device_auth`) take the same `DelegateRuntime`.
- `DelegateRuntime` carries a `confirm()` handle: the adapter calls it when a request or connect flow reaches its regular end
  (Claude: the `result` event; Codex: the completed turn; connect flows: the token or `auth.json` obtained). It creates
  `<version>/.confirmed` and schedules older versions for deletion once no active run uses them (spec FR-022); a stop,
  a missing login and a network error never call it. A second update is refused while the current version is unconfirmed.
- Provider rows: `add_provider` for `cli_delegate` no longer requires `base_url`; `upsert_delegate_provider` stores none.
- `holzi --mcp-agent …` (the MCP helper Claude spawns) receives the endpoint folder as `--endpoint-dir <path>` in the
  generated MCP config and no longer derives it from `XDG_RUNTIME_DIR`/`TMPDIR`/`HOME`.

## Not exposed

No command takes a URL, a checksum or a path: they come from the catalog. No command runs a downloaded program except the
delegate paths above.
