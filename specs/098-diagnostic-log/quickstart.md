# Quickstart: see the diagnostic log work

Run from the worktree with `pnpm tauri:dev` (a release build behaves the same; the dev build also prints to the terminal).

1. **Nothing before the unlock.** Use a fresh vault that has never been opened. Start the app and stay on the vault list. Look
   in its folder: there is no `logs/` folder yet. Unlock the vault; `logs/haex.log` appears and its first line is after unlock.
2. **Standard.** In "Einstellungen → Allgemein → Grundeinstellung" find "Protokoll", level "Standard". Import a legacy-format
   database with a malformed field (as prepared by T016): `haex.log` gets a `WARN` line that names the corruption cause.
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
8. **Unwritable.** Close the app, make both the existing `logs/haex.log` file and its containing `logs/` directory read-only,
   then unlock the vault. The app keeps working and the setting says the log cannot be written.
