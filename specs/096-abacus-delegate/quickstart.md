# Quickstart: Abacus AI CLI delegate (manual verification, operator tasks)

**Spec**: [spec.md](spec.md) | Everything here needs a real Abacus account (FR-015) except steps 0 and 1. Run on a machine
whose `PATH` has no `abacusai`, `node` or `npm`. Record each result in `research.md` (a "Measured" line per step); a result
that contradicts the spec goes to the maintainer before the version enters the catalog.

## 0. Prepare a clean-looking host

List the host's home, `/tmp` and config folders so step 7 has something to compare with (spec 094 quickstart §0 has the
commands). You need your normal Abacus account (the one whose sign-in unlocks the free models); no API key is used anywhere.
Keep every device code you obtain out of shell history and of any file in the repository.

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

## 3. The sign-in service and the authenticated measurement (FR-005, FR-013, FR-021, SC-003, SC-004)

**3.0 The sign-in service** (decides whether connecting from the UI is possible at all). With `curl`, no tool involved:

1. `POST https://apps.abacus.ai/api/_authenticateDeviceForCodeLlm` with body `{"email":"<your address>","deviceId":"<any
stable string, e.g. a random uuid>"}`; expect `result.verificationUri`, `result.userCode`, `result.pollingInterval`. Record
   whether the service answers a plain client, or needs headers (a user agent such as `abacusai-cli/2.10.23`).
2. Open the address, enter the code, confirm with your account.
3. `POST https://apps.abacus.ai/api/_pollDeviceCode` with `{"userCode":"<code>"}` every 5 s; expect `result.deviceCode`
   once confirmed, and `success: false` before. Record the time to confirm, and what the service answers for a code that is
   not confirmed in ten minutes.

Keep the `deviceCode` for the next steps (never in a file of the repository). If step 1 or 3 refuses a client other than the
vendor's own, stop and tell the maintainer: connecting from the UI then has no way, and the operator decides.

Run the rest in a throwaway home with the isolation of research §4 (empty environment, read-only rest of the disk, `strace`
on file and network calls), `CODELLM_DEVICE_CODE=<the device code>` set for the process (no `ABACUS_API_KEY`),
`ABACUS_AUTO_UPDATE=off`, `TMPDIR` and `XDG_CONFIG_HOME` below the throwaway home. Note for each: exit code, stdout,
stderr, and every file created or changed.

| #   | Command                                                                                                                                                                                                                                                                      | What to record                                                                                                                                                                                                                 |
| --- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 3.1 | `<vault>/cli-tools/abacusai/<version>/abacusai auth status`                                                                                                                                                                                                                  | Text and exit code with the device code; with a wrong one (decides whether connecting can validate)                                                                                                                            |
| 3.2 | `<vault>/cli-tools/abacusai/<version>/abacusai -p "say hi" --output-format json`                                                                                                                                                                                             | The reply, **and the cost: does the run use a model that costs nothing for your account, or does it spend credits? Compare before and after on the account page**                                                              |
| 3.3 | the same, then list `home`, `tmpdir`, `xdgcfg` and the agent directory                                                                                                                                                                                                       | Files created: a keystore (`keystore.enc`)? sessions? memory? checkpoints? caches? which variable moved each                                                                                                                   |
| 3.4 | `<vault>/cli-tools/abacusai/<version>/abacusai -p --input-format stream-json --output-format stream-json --permission-mode supervise --no-agents-md`, stdin: one `user` line asking for a read-only tool and then a shell command (`ls`), waiting for each `control_request` | The exact fields of each `request` in `can_use_tool`; the `system/init` line; answer `accept`, then run again and answer `reject_with_message`; record whether both tool classes require approval and how the transcript reads |
| 3.5 | in that session send `{"type":"control_request","request_id":"m1","request":{"subtype":"get_model_catalog"}}`                                                                                                                                                                | The reply (decides whether the model list can come from the account)                                                                                                                                                           |
| 3.6 | any of the above, network calls from `strace`                                                                                                                                                                                                                                | Hosts connected to; whether an update check happens with `ABACUS_AUTO_UPDATE=off`                                                                                                                                              |
| 3.7 | kill the process during 3.4 (`kill -9`)                                                                                                                                                                                                                                      | Files left behind (SC-004)                                                                                                                                                                                                     |

Pass: nothing outside the throwaway home, `tmpdir`, `xdgcfg` and the agent directory is created or changed; no file contains
the device code (`grep -r` for it); 3.4 shows a working approval round trip. Anything else is listed under the spec's
Assumptions as a residual and decided by the maintainer (FR-013). If 3.2 shows that the sign-in's code is billed like an API
key, tell the maintainer: the operator wanted the free models of the sign-in.

## 4. Connect from the app (US1, US2)

Settings, the Abacus row, Connect: the app asks for your e-mail address, then shows an address and a code and offers to open
the address. Confirm the code in the browser; the backend shows as connected. Cancel the dialog once while waiting: nothing is
stored and no further request goes out (watch the network). Let one code expire (ten minutes): the app says so and offers to
start again. Open the password manager window: the entry `Delegate: Abacus AI` exists, with the device code as its
secret (and the built-in agent's search finds nothing of it).

## 5. A real request and an approval (US3)

Pick Abacus as the model, ask for something that needs a shell command. With the approving posture the command waits for
you; reject once (the reply says why), accept once. With the blocking posture the command is not run. Stop a running
response: the process ends (`pgrep abacusai`).

## 6. A second device (US2, SC-002)

**First check, before the ten runs:** does the device code work on a device other than the one it was obtained on? Take the
code of step 3.0 (obtained with one `deviceId`) and run 3.2 with it on a second machine. If it is refused, the code is bound
to its device: record the answer, and the plan's device-binding case applies (each device connects once, data-model §3).

After sync, open the vault on another device, install the tool there, send a request without any connect step. Repeat ten
times alternating between the two devices.

## 7. No traces (SC-003, SC-004, SC-008)

Compare the listings of step 0 after install, connect, request, stop, update (when two versions exist), disconnect and
removal: no new or changed file outside the vault's folder except listed residuals. Search every device-wide file
HaexVault writes (its log in a build with logging on) for the vault's name, `abacus` and the request's text, and the vault's
folder and the host for the device code.

## 8. Moved vault and removal (US5)

Copy the vault folder to a machine of another operating system or architecture: the tool shows "does not fit this device -
reinstall". Remove the tool and disconnect: the files, the provider and the password-manager entry are gone.

## 9. Windows (when its change lands)

Install the zip build on `win32-x64`, run steps 2, 3.1 and 3.4 there (the isolation is `best effort`), and record the
folders the tool writes under the account folders (`USERPROFILE`, `APPDATA`, `LOCALAPPDATA` are moved by the base layer).
