# Quickstart: see the diagnostic log work

Run from the worktree with `pnpm tauri:dev` (a release build behaves the same; the dev build also prints to the terminal).

1. **Nothing before the unlock.** Start the app and stay on the vault list. Look in `vaults/<name>/`: there is no `logs/` folder
   yet. Unlock the vault; `logs/haex.log` appears and its first line is after the unlock.
2. **Standard.** In "Einstellungen → Allgemein → Grundeinstellung" find "Protokoll", level "Standard". Import a wrong file
   (a text file in the password manager's import with the source "haex-vault 2.x (Legacy, Vault-Datei)"): `haex.log` gets a
   `WARN` line that names the cause.
3. **Ausführlich.** Switch to "Ausführlich": the setting shows the end time (24 hours from now). Repeat step 2; detail lines
   that exist by now appear without a restart. Switch back and the detail lines stop.
4. **The interface is in the same log.** In the dev tools console run `console.error(new Error('probe'))`; a line
   `[ui] Error: probe` with a stack appears in the same file within a second. `console.debug('x')` appears only in "Ausführlich".
5. **The end.** Set `until_ms` in the table `haex_diagnostic_log_no_sync` to a time in the past (or wait): after the next unlock the level is "Standard" again and
   the setting says so. Restart the app while "Ausführlich" is on: it is "Standard" again.
6. **Folder and share.** Desktop: the button opens the file manager at `logs/`. Android (emulator): "Protokoll teilen" opens the
   share sheet with `haex.log`.
7. **No secret.** Run `cargo test --test diagnostic_log_secrets`: it imports the pre-rename fixture in "Ausführlich" and finds none
   of the fixture's secrets in the log.
8. **Unwritable.** Make `logs/` read-only: the app keeps working, the setting says the log cannot be written.
