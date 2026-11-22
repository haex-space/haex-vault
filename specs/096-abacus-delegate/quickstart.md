# Quickstart: Abacus AI CLI delegate (manual verification, operator tasks)

**Spec**: [spec.md](spec.md) | Everything here needs a real Abacus account (FR-015) except steps 0 and 1. Run on a machine
whose `PATH` has no `abacusai`, `node` or `npm`. Record each result in `research.md` (a "Measured" line per step); a result
that contradicts the spec goes to the maintainer before the version enters the catalog.

## 0. Prepare a clean-looking host

List the host's home, `/tmp` and config folders so step 7 has something to compare with (spec 094 quickstart §0 has the
commands). Create an API key in your Abacus account (the account's API-key page, or `/login` in a throwaway terminal
session, your choice) and keep it out of shell history and of any file in the repository.

## 1. Check the release and its signature (maintainer, no account)

For each platform key of the version to be added (`linux-x64`, `linux-x64-baseline`, `linux-arm64`, `darwin-x64`,
`darwin-arm64`, `win32-x64`, `win32-arm64`): download the archive, unpack it into its own empty folder, and verify the
Sigstore bundle against the program with identity
`https://github.com/abacusai/codellm/.github/workflows/release-cli-native.yml@refs/tags/cli-v<version>` and issuer
`https://token.actions.githubusercontent.com` (`scripts/cli-tools/add-version.ts` does this once extended; research §1 shows
the manual call). Expect `OK` for every platform and a failure for a changed byte. Only after that, copy each archive's
SHA-256 and size into the catalog entry.

## 2. Install (US1, SC-001)

Open a vault, Settings, the Abacus row: the download size and the space it takes are shown (about 79 MB and about 158 MB on
Linux x64). Install; progress shows; cancel once and install again. The installed version is shown as `2.10.23`. Time it
on a 50 Mbit/s connection (SC-001: under 5 minutes).

## 3. Authenticated measurement (FR-013, FR-021, SC-003, SC-004)

Run these in a throwaway home with the isolation of research §4 (empty environment, read-only rest of the disk, `strace`
on file and network calls), `ABACUS_API_KEY` set for the process, `ABACUS_AUTO_UPDATE=off`, `TMPDIR` and `XDG_CONFIG_HOME`
below the throwaway home. Note for each: exit code, stdout, stderr, and every file created or changed.

| #   | Command                                                                                                                                                                                                        | What to record                                                                                                                                                                 |
| --- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 3.1 | `abacusai auth status`                                                                                                                                                                                         | Text and exit code with a valid key; with a wrong key (decides whether connecting can validate)                                                                                |
| 3.2 | `abacusai -p "say hi" --output-format json`                                                                                                                                                                    | The reply; **credit use before and after in the Abacus account page (does a key draw on the subscription's credits, or is it billed separately?)**                             |
| 3.3 | the same, then list `home`, `tmpdir`, `xdgcfg` and the agent directory                                                                                                                                         | Files created: a keystore (`keystore.enc`)? sessions? memory? checkpoints? caches? which variable moved each                                                                   |
| 3.4 | `abacusai -p --input-format stream-json --output-format stream-json --permission-mode supervise --no-agents-md`, stdin: one `user` line asking for a shell command (`ls`), then wait for the `control_request` | The exact fields of `request` in `can_use_tool`; the `system/init` line; then answer `accept`, run again and answer `reject_with_message`, and record how the transcript reads |
| 3.5 | in that session send `{"type":"control_request","request_id":"m1","request":{"subtype":"get_model_catalog"}}`                                                                                                  | The reply (decides whether the model list can come from the account)                                                                                                           |
| 3.6 | any of the above, network calls from `strace`                                                                                                                                                                  | Hosts connected to; whether an update check happens with `ABACUS_AUTO_UPDATE=off`                                                                                              |
| 3.7 | kill the process during 3.4 (`kill -9`)                                                                                                                                                                        | Files left behind (SC-004)                                                                                                                                                     |

Pass: nothing outside the throwaway home, `tmpdir`, `xdgcfg` and the agent directory is created or changed; no file contains
the key (`grep -r` for it); 3.4 shows a working approval round trip. Anything else is listed under the spec's Assumptions as a
residual and decided by the maintainer (FR-013). If 3.2 shows a separate bill, tell the maintainer before building the
connect UI: the operator wanted the subscription path.

## 4. Connect from the app (US1, US2)

Settings, the Abacus row, Connect: the app asks for the key, nothing else. Paste it; the backend shows as connected. Cancel
the dialog once: nothing is stored. Open the password manager window: the entry `Delegate: Abacus AI` exists (and the
built-in agent's search finds nothing of it).

## 5. A real request and an approval (US3)

Pick Abacus as the model, ask for something that needs a shell command. With the approving posture the command waits for
you; reject once (the reply says why), accept once. With the blocking posture the command is not run. Stop a running
response: the process ends (`pgrep abacusai`).

## 6. A second device (US2, SC-002)

After sync, open the vault on another device, install the tool there, send a request without any connect step. Repeat ten
times alternating between the two devices.

## 7. No traces (SC-003, SC-004, SC-008)

Compare the listings of step 0 after install, connect, request, stop, update (when two versions exist), disconnect and
removal: no new or changed file outside the vault's folder except listed residuals. Search every device-wide file
HaexVault writes (its log in a build with logging on) for the vault's name, `abacus` and the request's text, and the vault's
folder and the host for the key.

## 8. Moved vault and removal (US5)

Copy the vault folder to a machine of another operating system or architecture: the tool shows "does not fit this device -
reinstall". Remove the tool and disconnect: the files, the provider and the password-manager entry are gone.

## 9. Windows (when its change lands)

Install the zip build on `win32-x64`, run steps 2, 3.1 and 3.4 there (the isolation is `best effort`), and record the
folders the tool writes under the account folders (`USERPROFILE`, `APPDATA`, `LOCALAPPDATA` are moved by the base layer).
