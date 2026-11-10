# Quickstart: Vault folders

Run these by hand before the last PR. Work on a **copy** of the data folder (`cp -a <AppLocalData> /tmp/holzi-data-copy` and
start holzi with it as `XDG_DATA_HOME`/its data folder), never on the only copy of real vaults.

**M1 Move.** Copy a data folder with at least three vaults in the old layout (one with an agent browser profile and chat files).
Start holzi. Expect: the list shows the same vaults; each opens with its passphrase; `vaults/<name>/` holds `vault.db*`,
`agent-browser/`, `chat-files/` and nothing else; `instances/` and the vault-named folders in `agent-browser/` and
`chat-files/` are gone.

**M2 Interrupted move.** Start holzi, kill it (`kill -9`) while it moves (a data folder with many vaults, or the test's
step hook). Start again. Expect: all vaults moved, none twice, none lost.

**M3 Open elsewhere.** Start a holzi process with vault A open, start a second one on the same data folder. Expect: A is not
moved by the second process; after closing both and starting once, A is moved.

**M4 Name `filters`.** A vault named `filters` in the old layout, with tracker lists in `agent-browser/filters/`. Expect: the
vault moves like another; the lists end up in `filter-lists/`; the browser still blocks trackers.

**M5 Choices per vault.** Two vaults. Switch a tracker list off in A, choose another DNS provider in A. Open B in a second
run. Expect: B shows the defaults. On a second device synced to A: the tracker choice arrives, the DNS choice does not.

**M6 Interrupted creation.** Kill holzi while a vault is being created. Start again. Expect: no folder and no file of that vault.
