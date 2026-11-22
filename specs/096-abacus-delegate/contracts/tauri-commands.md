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
remove" (the settings page deletes the provider with the tool, spec 094 US4). Deleting the provider removes the credential
entry (next command), so removing a tool and its connection stays the two calls the page already makes.

### `connect_cli_delegate(args: { vendor, name }) -> ConnectCliDelegateResult`

For `vendor = "abacusai"` the command does not start a flow. It checks that the tool is installed in this vault (otherwise
the same "not installed" error the other vendors return, which the UI turns into the install action) and returns

```json
{ "status": "awaiting_key", "vendor": "abacusai" }
```

(`ConnectCliDelegateResult` gains the variant `AwaitingKey { vendor }`, snake_case tag like the others.)

## Commands that are new

### `connect_cli_delegate_with_key(args: { vendor, name, key }) -> ConnectCliDelegateResult`

Saves the key as the vendor's credential (`delegate_credential::save`, kind `api_key`), upserts the provider row
(`write_delegate_provider` with empty credentials), and returns `Connected { provider }`. Errors: `InvalidInput` for an empty key or
an unknown vendor, `CliToolNotInstalled` when the tool is missing. The key is never logged and never part of an event.
Whether the command also checks the key by starting the tool is decided after the authenticated measurement (quickstart §3);
until then a wrong key shows at the first request as `not_authenticated`.

`vendor` must be a vendor whose credential is a key (`abacusai`); `claude` and `codex` are refused with `InvalidInput`, since
their credentials stay on the provider row (spec Assumptions).

### `delete_provider(id)` (existing command, extended)

Disconnecting is deleting the provider row. For a `cli_delegate` row whose `adapter` is `abacusai`, the command first deletes
the password-manager entry (`delegate_credential::delete`; a missing entry is not an error) and then the row. A response
already running is not disturbed (it holds its own copy of the key in its process environment). Rows of `claude` and
`codex` are deleted as before.

## Events

No new event. `cli-tool-progress` and `cli-tool-changed` carry `vendor: "abacusai"`. `delegate-connect-progress` is not used
by this vendor.

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
   `ABACUS_API_KEY` for the child.

3. It writes one `user` line, reads stdout lines (data-model §4), answers `can_use_tool` through
   `approval_bridge::request_approval`, and closes stdin after the `result` line.
4. Stop kills the process (spec 007 FR-011); closing the vault stops it too (spec 094 FR-010). Exit 143 after a stop is not an
   error.
5. The run folder is removed when the run ends; a folder that survived a crash is cleared at the next vault open (spec 094).

## Not exposed

No command reads the credential back to the frontend, and no command lists, prints or exports it. The entry is an owned entry:
only the user (in the password manager's window) and the `delegate` feature see it; the built-in agent does not see it at
all, not even its title (ADR 0007, `sees_owned`).
