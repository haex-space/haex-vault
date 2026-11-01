# Quickstart: validating spec 072

Automated checks first, then the manual run with real devices. Contracts:
[nostr-chat.md](./contracts/nostr-chat.md), [blossom.md](./contracts/blossom.md),
[ipc.md](./contracts/ipc.md); tables: [data-model.md](./data-model.md).

## Prerequisites

- `nix develop` (cargo, node); build only the targets named below (full builds have crashed the editor).
- Rust tests use `MockRelay` (nostr-sdk `local-relay`) and the in-process test Blossom server
  (`src-tauri/tests/support/blossom.rs`); no network.
- E2E: the spec 016/033 rig with `e2e_nostr_relay` and `e2e_blossom` (`--features e2e`).

## Automated

| #   | What                                               | Command (from `src-tauri/`)                                                                         | Expect                                                                                            |
| --- | -------------------------------------------------- | --------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------- |
| A1  | Migration 0049 fresh + upgrade, agent threads kept | `cargo test --lib migrations_chat_contacts`                                                         | existing `chat_threads` rows have `kind = agent`, messages untouched (SC-001)                     |
| A2  | Wrap/unwrap, size limit, checks of R5              | `cargo test --lib chat::peer::verify`                                                               | forged seal key, older card, blocked vault, wrong `to`: dropped, no receipt                       |
| A3  | Two vaults, one closed: send, open, delivered      | `cargo test --test chat_two_vaults`                                                                 | US2 1–5, 7                                                                                        |
| A4  | Own devices never online together                  | `cargo test --test chat_own_devices`                                                                | both devices end with the same rows (FR-016)                                                      |
| A5  | 1,000 messages, random on/offline                  | `cargo test --test chat_soak -- --ignored`                                                          | no duplicate, none lost (SC-006)                                                                  |
| A6  | Files via test Blossom                             | `cargo test --test chat_files`                                                                      | hashes checked, HXCF tamper rejected, blob deleted after all `files` receipts, 30-day sweep (US3) |
| A7  | Groups: concurrent add/remove converge             | `cargo test --lib chat::peer::group`                                                                | same member list on all members for every op order (FR-056)                                       |
| A8  | Three-vault group                                  | `cargo test --test chat_group_three_vaults`                                                         | US4 1–10                                                                                          |
| A9  | Requests and blocking                              | `cargo test --test chat_requests`                                                                   | US5 1–6                                                                                           |
| A10 | Agent gate                                         | `cargo test --lib chat::peer::agent_actions`                                                        | no read without grant, send only after confirm (SC-010)                                           |
| A11 | Nostr-Relay inspection                             | `cargo test --test chat_privacy`                                                                    | stored events and blobs contain no plaintext, names or vault ids (SC-005)                         |
| A12 | Frontend checks                                    | from repository root: `pnpm check:chat-state && pnpm check:settings && pnpm lint && pnpm typecheck` | seven settings categories, i18n de/en complete                                                    |
| A13 | E2E two apps                                       | from repository root: `pnpm test:e2e -- --grep chat-two-vaults`                                     | text and a 5 MB file both ways; state _read_                                                      |

## Manual (M)

| #   | Setup                                                       | Steps                                          | Expect                                                                             |
| --- | ----------------------------------------------------------- | ---------------------------------------------- | ---------------------------------------------------------------------------------- |
| M1  | Vault with old agent chats                                  | update, open chat                              | every agent chat there and continues (US1 1)                                       |
| M2  | Anna (laptop + phone), Ben (laptop), contacts of each other | Anna writes while Ben is closed; Ben opens     | message arrives < 30 s after opening (SC-003); Anna sees _delivered_, later _read_ |
| M3  | Anna offline (flight mode)                                  | send, go online                                | _waiting_ → _sent_ < 30 s (SC-004)                                                 |
| M4  | Anna's phone and laptop never online together               | write from both, receive on both               | same conversation on both (US2 6)                                                  |
| M5  | Files                                                       | 3 photos + 10 MB PDF; Ben offline, then online | arrive, open, "Speichern unter"; blob gone after all devices confirmed (SC-007)    |
| M6  | Android on mobile data                                      | 8 MB file arrives                              | "Herunterladen" shown, fetched on tap (FR-036)                                     |
| M7  | Group of three, one non-contact                             | create, accept, rename, add, remove, leave     | notes, member states, "nicht in deinen Kontakten" (US4)                            |
| M8  | Unknown sender                                              | Clara (has Anna's card) writes                 | "Anfragen", accept → contact, block → silence (US5)                                |
| M9  | Contact share                                               | "Teilen" → "Im Chat senden"                    | card with photo, preview, add (US6)                                                |
| M10 | Agent                                                       | ask to read Ben's chat, then to send           | permission dialog, send confirmation (US7)                                         |
| M11 | Settings                                                    | change server, receipts off, limits, mute      | effects as US8; Blossom key in password manager                                    |
| M12 | Platforms                                                   | M2, M5 on Windows, macOS, Android              | same behaviour (SC-011)                                                            |
| M13 | Public Blossom presets                                      | upload 20 MB encrypted blob to each preset     | accepted, `HEAD` reports limits, `DELETE` works (R12)                              |
