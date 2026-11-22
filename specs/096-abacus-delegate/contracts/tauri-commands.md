# Contract: Tauri commands for the Abacus delegate

**Spec**: [../spec.md](../spec.md) | Builds on `specs/094-vault-cli-tools/contracts/tauri-commands.md` and
`specs/007-cli-delegate/contracts/tauri-commands.md`; only what differs is listed. Command arguments are camelCase,
as everywhere.

## Commands that change

### `cli_tools_status() -> CliToolStatus[]`, `cli_tools_install`, `cli_tools_cancel`, `cli_tools_revert`, `cli_tools_remove`

Unchanged in shape. `vendor` accepts `"abacusai"` in addition to `"claude"` and `"codex"`. `CliToolStatus.vendor` and
`CliToolProgress.vendor` carry it, and the generated bindings name it. On a device none of whose catalog keys fit (for example
Linux on a CPU architecture the vendor does not build), the status says `unsupported_platform` with the reason, as spec 094
defines it; a musl host gets the glibc build and is reported by the start check as `start_failed`.

`cli_tools_remove("abacusai")` removes the tool only, as for the other vendors; "the stored connection is the caller's to
remove" (the settings page deletes the provider with the tool, spec 094 US4). Deleting the provider removes all credential
scopes (next command), so removing a tool and its connection stays the two calls the page already makes.

### `connect_cli_delegate(args: { vendor, name, email? }) -> ConnectCliDelegateResult`

`ConnectCliDelegateArgs` gains an optional `email`. For `vendor = "abacusai"` it is required (else `InvalidInput`), and the
command runs the whole flow like the Codex branch does, returning `Connected` at the end:

1. It checks that the tool is installed in this vault (otherwise the same "not installed" error as the other vendors, which the
   UI turns into the install action), records `connect_started`.
2. It calls the sign-in service (data-model §3a): `_authenticateDeviceForCodeLlm` with the e-mail address and a random UUID
   as `deviceId`. A refusal ends the flow with the vendor's text as `InvalidInput`/`HaexError` reason.
3. It emits `delegate-connect-progress` with `status: "awaiting_browser"`, `url` (the `verificationUri`) and `code` (the
   `userCode`), the shape the Codex flow already uses and the UI already renders.
4. It polls `_pollDeviceCode` until the code is confirmed, the user cancels (below), the vault closes or ten minutes pass
   (`status: "error"`, reason `expired`).
5. It saves the device code with `delegate_credential::save` (kind `device_code`) in the measured scope (`vault` for a
   portable code, this installation ID for a device-bound code) and upserts the provider row
   (`write_delegate_provider` with empty credentials). Treat these writes as one logical operation: keep the prior
   credential in this scope and provider row until both writes succeed; if either write fails, restore the prior state and return an
   error. A failed connection must not leave an orphan credential or replace a working connection with a partial one.
   Then record `connect_finished`, emit `status: "success"` and return `Connected { provider }`.

There is exactly one `cli_delegate` provider row with `adapter = "abacusai"` per vault. Upsert reuses that row. If legacy
or synced data contains duplicates, retain the row with the lexicographically smallest provider id, remove the other rows,
and preserve the vendor credentials while connecting. Connecting on a device-bound installation updates only that
installation's credential entry; it does not replace another device's entry. Disconnecting the last Abacus provider row is
vault-wide and removes every Abacus credential scope. Cleanup of duplicate provider rows must not delete credentials while a
provider row survives.

No `AwaitingCode`/`AwaitingKey` result exists for this vendor. The e-mail address, the code and the device code are never
logged and appear only in the event (address and code) and in the entry (device code).

### `cancel_cli_delegate_connect(vendor)` (new)

Stops a running connect flow of `vendor` (the polling loop; idempotent). The UI calls it when the dialog is closed. For
`claude` and `codex` it does nothing new (their flows end with their processes).

### `delete_provider(id)` (existing command, extended)

Disconnecting is deleting the provider row. For a `cli_delegate` row whose `adapter` is `abacusai`, the command deletes all
password-manager entries for that vendor (`delegate_credential::delete`; a missing entry is not an error) only when this is
the last Abacus provider row. A response already running is not disturbed (it holds its own copy of the device code in its
process environment). Rows of `claude` and `codex` are deleted as before.

## Events

No new event. `cli-tool-progress` and `cli-tool-changed` carry `vendor: "abacusai"`. `delegate-connect-progress` carries the
connect flow: `awaiting_browser` (with `url` and `code`), then `success` or `error` (with a `message`, `expired` among them),
as the Codex flow emits them.

## Delegate-side contract (no command)

A request answered by Abacus:

1. `build_adapter` builds `CliDelegateAdapter` with `DelegateVendor::Abacus` and no credential bytes.
2. `stream_chat` resolves the `DelegateRuntime` (the vault's copy, spec 094), reads the credential (`Ready`, `Missing` or
   `Syncing`; the last two end the request with `not_connected`, the second saying the vault is still syncing), creates the
   run folder, builds the command:

   ```text
   <vault>/cli-tools/abacusai/<version>/abacusai -p --input-format stream-json --output-format stream-json
       --permission-mode supervise --no-agents-md [--model <id>] [--mcp-config <run folder>/mcp.json]
   ```

   runs it through `Containment` (the base layer sets `HOME`, `TMPDIR`, the XDG variables, `ABACUSAI_CODING_AGENT_DIR` and
   the vendor's `env_set`, and removes the host's `ABACUS_*`, `ABACUSAI_*`, `CODELLM_*` variables), and only then sets
   `CODELLM_DEVICE_CODE` for the child (an `ABACUS_API_KEY` is never set).

3. It writes one `user` line, reads stdout lines (data-model §4), answers `can_use_tool` through
   `approval_bridge::request_approval`, and closes stdin after the `result` line.
4. Stop kills the process (spec 007 FR-011); closing the vault stops it too (spec 094 FR-010). Exit 143 after a stop is not an
   error.
5. The run folder is removed when the run ends; a folder that survived a crash is cleared at the next vault open (spec 094).

## Not exposed

No command reads the credential back to the frontend, and no command lists, prints or exports it. The entry is an owned entry:
only the user (in the password manager's window) and the `delegate` feature see it; the built-in agent does not see it at
all, not even its title (ADR 0007, `sees_owned`).
