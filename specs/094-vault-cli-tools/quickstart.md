# Quickstart: CLI tools per vault (manual verification)

For the reviewer or operator before merge. Needs a machine you can inspect and, for steps 5–7, a real Claude or Codex
subscription. The automated checks (unit, integration with stub delegates, e2e with a stand-in release host) are listed in
`tasks.md`; this file covers what only real tools can show. All paths are below `<AppLocalData>/vaults/<name>/`, written
`<vault>` here.

## 0. Prepare a clean-looking host

1. Make sure `claude`, `codex`, `node`, `npm` are **not** on `PATH` for the started app (for example start holzi from a
   shell with `PATH=/usr/bin:/bin`). A host copy of `claude` may stay installed elsewhere; step 4 tests that it is ignored.
2. Take a baseline listing for step 7: `find ~ /tmp -xdev -newer <some-marker-file> 2>/dev/null` is enough after a run; or
   `find ~ /tmp -xdev -printf '%p %T@\n' | sort > before.txt` now and again later.

## 1. Install (US1)

1. Open a vault, Settings → Providers. Each delegate row shows "Not installed in this vault" and an Install button.
2. Select a delegate in the chat model picker: the entry says "Not installed in this vault", nothing is sent.
3. Install Codex and time it (SC-001: under 5 minutes at 50 Mbit/s). A progress bar runs; afterwards the row shows the
   installed version. Repeat for Claude Code.
4. `ls <vault>/cli-tools/codex/` shows one version folder with `install.json`, and no `.staging` folder.

## 2. Check the installed programs directly (research §1, §2, §4; measured once, repeat per version)

1. From a shell with an empty environment apart from `PATH`, run the installed program directly with the variables holzi
   sets: `env -i PATH=/usr/bin:/bin HOME=<vault>/home DISABLE_UPDATES=1 <vault>/cli-tools/claude/<version>/claude --version`.
   Expected: the version prints (measured for Claude Code 2.1.311).
2. `… claude doctor` with the same environment: the `Auto-updates` line reads disabled and names the source.
3. Codex: `<vault>/cli-tools/codex/<version>/bin/codex -c check_for_update_on_startup=false --version`.

## 3. Host copy ignored (US2, SC-002)

Prepare a harmless host stub named `claude` in a temporary directory, then restart holzi with that directory before
`/usr/bin:/bin` on `PATH`. Connect and send a request. The process list during the request
(`ps -eo pid,args | grep claude`) shows the path under `<vault>/cli-tools/`, never the host's.

## 4. Without an installed tool

Prepare a harmless host stub named `codex` in a temporary directory, then restart holzi with that directory before
`/usr/bin:/bin` on `PATH`. Remove Codex in the UI. Selecting it again shows "Not installed in this vault", even though a
host `codex` exists.

## 5. Connect and answer with real subscriptions

Connect Claude and Codex as in spec 007's quickstart, send one request each, answered by the vault's copy. Check
`<vault>/home/run/` is empty again afterwards (the run folders are removed at the end of a run).

## 6. Update (US3) and a run in flight

1. Configure the test catalog with exactly versions `1.0.0` and `1.1.0`. Install `1.0.0`, then update to `1.1.0`.
   During the update send a request: it finishes normally. After the update both version folders exist and the row offers
   "go back". Send one request with `1.1.0` that ends normally; the old folder is removed when no `1.0.0` run remains.
2. Change the test catalog to list `1.2.0` above `1.1.0`, update to `1.2.0`, and choose "go back" before any run. The
   new folder is deleted and `1.1.0` answers. An update request while a version is unconfirmed is rejected until it is
   confirmed by a normal run or reverted.
3. Make the start check fail (a stand-in release host serving a program that exits 1): the update reports the failure and
   the previous version is untouched.
4. Kill holzi (`kill -9`) in the middle of an install: after the next start there is no `.staging` folder and the previous
   state is usable.

## 7. No traces (US2, SC-003, SC-009)

1. In a live delegate turn, make at least one shell call and one patch call before finishing the request. After a full
   run of steps 1–6 repeat the listing of step 0.2 and diff. Expected outside `<vault>/`: only the
   transient `/tmp/cc-socks-<uid>/` folder of Claude Code's inbox socket (plan D1) and files of holzi itself.
2. `grep -rIl "<vault name>" <AppLocalData>/logs ~/.cache ~/.config /tmp 2>/dev/null` finds nothing that holzi wrote.
3. `<vault>/home/records/tools.log` holds the records of the session.
4. Each tool row shows its isolation level; with the base layer it reads "best effort".
5. Pass-through: start holzi with `HTTPS_PROXY=http://127.0.0.1:9` set and look at the delegate's environment (a stub
   delegate prints it): `HTTPS_PROXY`, `PATH` and `LANG` are present unchanged, `ANTHROPIC_API_KEY` set on the host is absent.

## 8. Moved vault (US5)

Copy the vault folder to a machine with another OS or architecture and open it: the tool rows read "Does not fit this
device — reinstall". On the same machine, delete `<vault>/cli-tools/codex/<version>/bin/codex`, or cut it short with `truncate -s -1`: the
row reads "Needs reinstall".

## 9. Remove (US4)

Remove a tool: its folder is gone, the credential still shows connected. Remove a whole test vault's folder: nothing of
`cli-tools/` or `home/` remains anywhere.
