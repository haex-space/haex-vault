# Chat settings, first slice: the Blossom-Server

Date: 2026-11-21. Part of spec 072 US8 (FR-080, FR-038), tasks T099 to T102.

## Problem

A chat file needs a Blossom-Server. The vault setting `chat.blossom.server` exists and every send
reads it, but nothing in the app sets it, and there is no default (T104 is open). Without a server
no file can be sent. This slice makes the setting reachable and testable.

## Decisions

1. **A category "Chat"** (the seventh), as FR-080 asks. It holds one group for now, the server.
2. **No `chat_settings_get/set`.** Since `chat.maxToolRounds` the settings are vault preferences
   read and written with `usePreferences` and checked in `preferences_commands::validate_value`.
   The server address follows that path. `contracts/ipc.md` and T099 are amended.
3. **The address is checked by shape when it is saved**: `https` (or `http` to `127.0.0.1`), no
   credentials, no query, no fragment, or empty for "none". The same function (`blossom::parse_address`)
   serves `Server::new`, so what is saved is what a send accepts. Where the host lies is checked when
   it is used, as before.
4. **`chat_blossom_check { url }`** asks with `HEAD /upload` (BUD-06) and a key made for the test,
   for a file of the largest size. `accepts` ends it. A `413` is asked again for one byte:
   `onlySmall` when that goes through, else `refused`. Any other refusal holds for every size and is
   asked once. The answer is a verdict plus the status and the server's `X-Reason`; the window
   writes the sentence. No exact limit: BUD-06 does not name one.
5. **Saved on blur or Enter**, an empty field is "none" (the pattern of `AliasSetting.vue`); a failed
   save shows a toast and reloads what is stored. A saved change clears the old check result.
6. **No preset list yet.** Presets wait for T104, which tests the public servers; the default stays
   "none".

## Not in this slice

The upload key in the password manager (rest of T099), read receipts, download limit,
notifications, mute (T103), the composer hint without a server (T071), an E2E scenario (T105, T106).

## Tests

Rust in `probe_tests.rs`: accepts with one question and no upload; a limit gives `onlySmall` with
status and reason; a limit of zero gives `refused`; a `403` is asked once; a closed port is
`unreachable`; addresses a file may not go to are `notAllowed`; the shape check. `validate_value`
takes none or an address. `check-settings` expects seven categories; `settings-categories` (E2E)
lists them.
