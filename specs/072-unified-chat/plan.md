# Implementation Plan: One chat for the agent and for contacts, delivered over Nostr

**Branch**: `072-unified-chat` | **Date**: 2026-11-01 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/072-unified-chat/spec.md`

## Summary

The chat app shows agent, direct and group conversations in one list. Contact conversations travel only
over Nostr: every message, receipt and group change is a rumor, sealed with the sender's device key and
gift-wrapped (kind 1059, stored) to every device on the recipients' Visitenkarten and to the sender's own
other devices. Files are encrypted on the device (AES-256-GCM in 1 MiB segments) and put on a
Blossom-Server the user chooses; the message carries the address and key; every receiving device
downloads them itself, and the uploading device deletes the blob once all devices confirmed or after 30
days. Groups use per-member last-writer-wins over membership ops, so all members converge without an
admin or a server. Agents read contact conversations only with a grant and send only after a
confirmation dialog.

Technical approach (reasons and rejected alternatives in [research.md](./research.md)):

- **Wrapping** (R1, R2): generalise `sync::presence::wrap_payload` / `unwrap_rumor` (wrap kind, time
  tweak, size limit as parameters); chat uses kind 1059, rumor kinds 24120/24121, wraps ≤ 60 KiB, text
  ≤ 4,000 characters.
- **Delivery service** (R3, R6): `chat::peer::delivery` owns its own `nostr_sdk::Client` per vault
  session, subscribes `#p = own device key` since cursor − 48 h, publishes to the recipients' Nostr-Relays
  from their cards; outbox with back-off in `chat_outbox_no_sync`; delivery states as set-once timestamp
  columns.
- **Trust** (R5): every rumor carries the sender's signed card; seal key must be a device of it; older
  cards refused; newer cards update contacts by spec 060's rule; blocked vaults dropped silently.
- **Dedup and own devices** (R4): derived message ids (UUIDv5), seen-wrap table, wraps to own devices
  plus own-device CRDT sync converge on the same rows.
- **Files** (R7, R8, R11–R14): Nostr + Blossom only, no iroh path; BUD-01/02/06 client on the pinned
  reqwest client; per-blob upload key, optional registered upload key from the password manager; files
  kept per device in a vault-owned encrypted folder (spec 048, `ProfileStore` pattern).
- **Groups** (R9) and **requests** (R10): membership ops with LWW per member; invitations in parts;
  unknown senders land in "Anfragen", accepted through spec 060's preview/merge.
- **Agents** (R16): grant kind `contact_chats`, permission prompt, `chat.contacts.message.send` with a
  mandatory confirmation, same rules over MCP.
- **Window** (R17, R18): `ChatApp.vue` keeps the agent conversation (model wiring in `useAgentModelState.ts`) and shows `PeerConversation.vue` for contact conversations;
  address book "Nachricht senden"; share dialog "Im Chat senden"; settings category "Chat"; system
  notifications through the shared notification routing.

## Technical Context

**Language/Version**: Rust 2021 (stable from the nix devshell), TypeScript 5 / Vue 3 (Nuxt layer, Tauri 2)

**Primary Dependencies**: nostr 0.45 + nostr-sdk 0.45 (`nip44`, `nip59`; `nips::nipb7`), reqwest 0.13
(rustls, stream), aes-gcm 0.11, sha2 0.11, uuid (v5), ts-rs 12, tauri-plugin-notification 2.5,
tauri-plugin-dialog/fs, haex-ui. **No new crate**; `hyper` (already a dev-dependency) also becomes an
optional dependency for the `e2e` feature's test Blossom server.

**Storage**: vault SQLite (SQLCipher) via haex-crdt; migration `0049_chat_contacts` (data-model.md),
`HOLZI_TRIGGER_VERSION` 30 → 31; chat files in a per-device encrypted folder (spec 048).

**Testing**: `cargo test` (lib + integration tests with `MockRelay` and an in-process Blossom server),
`pnpm` check scripts, lint, typecheck, e2e rig of specs 016/033 (tauri-driver).

**Target Platform**: Linux, Windows, macOS, Android (iOS builds where they exist; nothing platform-specific
except notifications permission and "open with system app", which stays desktop-only as today).

**Project Type**: desktop + mobile app (Tauri: Rust core `src-tauri/`, Vue window `src/`).

**Performance Goals**: SC-002 (95 % < 5 s online), SC-003 (< 30 s after opening), SC-007 (10 MB file < 1
min); receipts batched every 2 s; ≤ ~100 wraps per group message.

**Constraints**: wraps ≤ 60 KiB; no plaintext, names or vault ids outside the encrypted layers (SC-005);
chat files ≤ 100 MB and ≤ the server's limit; one file in memory at a time while sealing into the
encrypted folder (`ponytail:` R14); Nostr-Relay changes apply at the next vault open (as presence).

**Scale/Scope**: conversations in the hundreds, messages in the tens of thousands per vault; groups ≤ 32
members; 8 user stories, ~80 FRs; ~25 new Rust files, ~15 new Vue/TS files.

## Constitution Check

_GATE: Must pass before Phase 0 research. Re-check after Phase 1 design._

| Rule                                                 | State                                                                                                                                                                                                                                               |
| ---------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Worktree on a topic branch, PR to `main`             | ✅ `.worktrees/072-unified-chat`, branch `072-unified-chat`; deliveries as separate PRs, rebase or merge commit, no squash                                                                                                                          |
| Speckit workflow                                     | ✅ specify → clarify (operator, 2026-10-31 and 2026-11-01) → plan; then tasks, analyze, implement                                                                                                                                                   |
| Tests in their own files                             | ✅ every new Rust file with a `*_tests.rs` beside it; multi-vault support in `chat/peer/test_support.rs`; integration tests in `src-tauri/tests/chat_*.rs`                                                                                          |
| 500 LoC per file                                     | ✅ module split below (wrap, envelope, verify, outbox, delivery loop, receipts, groups, blossom, crypto, store, agent actions each its own file). `ChatApp.vue` (493 lines) is split before it grows                                                |
| Graphify first / reuse                               | ✅ graph queried (research R21); reused presence wrapping, device keys, server config, card lookup/replace/block, `ProfileStore` pattern, files viewer, permission prompt, confirm pattern, notification routing, preferences, `vault-data-changed` |
| Laziness ladder / no unneeded dependency             | ✅ no new crate; one transport (Nostr + Blossom), no iroh protocol (R7); no group server, no MLS                                                                                                                                                    |
| One implementation per rule                          | ✅ one wrap implementation (presence's, generalised); card trust via `received::*` and 060's replace rule; agent prompts via `PermissionPrompt`; notification routing shared with extensions                                                        |
| No `unwrap`/`expect` on input, don't swallow errors  | ✅ rumors parsed once with size limits; drops are logged with reason codes, without content (R5)                                                                                                                                                    |
| Async not blocking                                   | ✅ DB access on the blocking pool; encryption of segments in `spawn_blocking`; network on the vault gate's tracker                                                                                                                                  |
| `ponytail:` markers                                  | ✅ idle write-relay timeout (R3), device-local outbox (R6), blob kept for dead devices until the sweep (R13), whole-file seal into the encrypted folder (R14)                                                                                       |
| I. No secrets in git                                 | ✅ keys only at runtime (per-blob keys in `_no_sync`, upload key in the password manager); test keys generated                                                                                                                                      |
| II./III. No local paths, device-independent identity | ✅ no paths in config; chat file folder resolved from `app_local_data` at runtime                                                                                                                                                                   |
| IV. Foreign content pinned                           | ✅ not touched (Blossom/NIP references are public specs, not consumed content)                                                                                                                                                                      |
| VII. Relay unavailability never blocks local work    | ✅ without Nostr-Relays or Blossom-Server everything local keeps working: messages wait in the outbox (FR-017), agent chats unaffected, received files open offline                                                                                 |
| VIII. No concealment                                 | ✅ not touched                                                                                                                                                                                                                                      |
| ADR for principle-affecting decisions                | ✅ new ADR 0024 "Chat between vaults over stored Nostr gift wraps and Blossom" (accepts ADR 0014's layers for chat; records "no direct path", per-device Blossom pickup, LWW group membership)                                                      |
| No agent attribution in commits and PRs              | ✅                                                                                                                                                                                                                                                  |
| Phase order                                          | ✅ builds on 024, 048, 051, 060, all in use. 068 has no code; this plan keeps the agent UI in `ChatApp.vue` (model wiring in `useAgentModelState.ts`) and adds `kind` to `chat_threads` (R20)                                                       |

After Phase 1, re-checked: no violation; Complexity Tracking empty.

## Project Structure

### Documentation (this feature)

```text
specs/072-unified-chat/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── nostr-chat.md      # gift wrap, rumor kinds 24120/24121, envelope, checks
│   ├── blossom.md         # HXCF format, BUD-01/02/06 calls, delete, HTTP client
│   └── ipc.md             # commands, events, actions, MCP scope
├── checklists/requirements.md
└── tasks.md               # /speckit-tasks
docs/adr/0024-chat-over-nostr-and-blossom.md
```

### Source Code (repository root)

```text
src-tauri/src/
├── sync/presence.rs                 # wrap_payload/unwrap_rumor: kind, time tweak, size limit as parameters
├── identity/
│   ├── migrations.rs                # register 0049, HOLZI_TRIGGER_VERSION 31
│   ├── migrations_chat_contacts.rs  # NEW: tables of data-model.md
│   └── migrations_chat_contacts_tests.rs
├── chat/
│   ├── mod.rs                       # + pub mod peer
│   └── peer/                        # NEW: contact and group conversations
│       ├── mod.rs                   # start_for_active_instance, state
│       ├── ids.rs                   # CHAT_NS, derived ids (R4)
│       ├── envelope.rs              # rumor JSON types, size budget (contracts/nostr-chat.md)
│       ├── wrap.rs                  # wrap/unwrap via presence, 1059, expiration
│       ├── verify.rs                # R5 checks, card update, block
│       ├── store.rs                 # chat_threads/members/peer_messages/attachments/deliveries access
│       ├── outbox.rs                # chat_outbox_no_sync, back-off
│       ├── delivery.rs              # nostr client, subscribe, publish, cursors, seen wraps
│       ├── receive.rs               # incoming rumor → rows, notes, requests
│       ├── receipts.rs              # batching, delivered/read/files
│       ├── group.rs                 # ops, LWW per member, invitations in parts (R9)
│       ├── files/
│       │   ├── crypto.rs            # HXCF segments (contracts/blossom.md)
│       │   ├── blossom.rs           # HEAD/PUT/GET/DELETE, auth events
│       │   ├── store.rs             # per-device encrypted folder (ProfileStore pattern)
│       │   ├── transfers.rs         # upload before send, downloads, chat-transfer events, limits
│       │   ├── sweep.rs             # delete after all `files`, 30-day sweep
│       │   └── network.rs           # metered network (Android plugin; desktop = not metered)
│       ├── settings.rs              # chat.* preferences, presets
│       ├── notify.rs                # system notifications (R17)
│       ├── agent_actions.rs         # chat.contacts.* executor (R16)
│       ├── agent_send_confirm.rs    # chat-send-confirm-request / answer
│       ├── commands/                # Tauri commands of contracts/ipc.md, split to stay < 500 lines:
│       │   └── conversations.rs, messages.rs, requests.rs, files.rs, groups.rs, settings.rs
│       └── test_support.rs          # two/three vaults, MockRelay, test Blossom
├── notifications/                   # NEW shared routing, extracted from extensions/desktop.rs
├── files/permissions.rs             # AgentFileKind::ContactChats
├── files/agent/prompt.rs            # ask_contact_chats
├── mcp_server/grants.rs             # data scope chat.contacts.read
├── sync/foreign_auth.rs             # (unchanged; no new ALPN)
├── instances/{open,create,import}.rs # start chat::peer with the session
├── lib.rs                           # commands, executor registration
└── bin/e2e_blossom.rs               # NEW, feature e2e
src-tauri/plugins/holzi-android/     # HolziAndroidPlugin.kt: isActiveNetworkMetered (FR-036)
src-tauri/tests/
├── support/blossom.rs               # in-process Blossom server (hyper)
└── chat_two_vaults.rs, chat_own_devices.rs, chat_files.rs, chat_group_three_vaults.rs,
    chat_requests.rs, chat_privacy.rs, chat_soak.rs

src/
├── components/apps/ChatApp.vue              # list + navigation only
├── components/chat/
│   ├── PeerListItem.vue, NewContactDialog.vue   # NEW (PR B)
│   ├── PeerConversation.vue                 # NEW
│   ├── PeerMessageList.vue, AttachmentBubble.vue, ContactCardBubble.vue,
│   │   GroupMembersPanel.vue, RequestBanner.vue, NewConversationDialog.vue,
│   │   ConversationPicker.vue, SendConfirmDialog.vue   # NEW
│   └── ThreadSidebar.vue                    # → conversation list with filter, avatars, unread, Anfragen
├── composables/useConversations.ts, usePeerConversation.ts, useChatTransfers.ts   # NEW
├── components/contacts/EntryPage.vue        # "Nachricht senden"
├── components/contacts/ShareDialog.vue      # "Im Chat senden"
├── components/settings/ChatView.vue         # NEW category
├── lib/settings/registry.ts, components/wm/appRoutes.ts   # category chat, chat routes requests/…
├── lib/actions/chatActions.ts               # chat.contacts.*, chat.group.*, chat.request.*
├── pages/workspace/[instance].vue           # mount SendConfirmDialog
└── i18n/locales/{de,en}.json
scripts/check-settings.ts                    # seven categories
```

**Structure Decision**: contact chat lives in a new `chat::peer` module beside the agent chat (`chat::turn`
untouched, R20), with its own tables but the shared `chat_threads` list (R15). The window keeps one chat
app; agent and peer views are siblings under it.

## Delivery slices (PRs)

Each slice is mergeable and testable on its own; later ones build on earlier ones.

| PR  | Content                                                                                                                                                                                       | Stories         |
| --- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------- |
| A   | docs: spec, plan, research, contracts, ADR 0024 (this PR #724)                                                                                                                                | –               |
| B   | migration 0049, `chat::peer::{ids, store}`, `chat_conversations_list`, model wiring into useAgentModelState, conversation list with kinds/filter, "Nachricht senden" + empty PeerConversation | US1             |
| C   | wrap generalisation, envelope, verify, outbox, delivery service, receive, receipts, direct send/receive, read state, notifications                                                            | US2, FR-023/024 |
| D   | requests and blocking                                                                                                                                                                         | US5             |
| E   | files: HXCF, Blossom client, test server, transfers, per-device store, sweep, viewer/save, download limits, chat settings (Blossom part)                                                      | US3, US8 (part) |
| F   | groups: ops, invitations, members panel, notes                                                                                                                                                | US4             |
| G   | contact share in chat                                                                                                                                                                         | US6             |
| H   | agent grant, actions, confirm dialog, MCP scope                                                                                                                                               | US7             |
| I   | remaining settings, Blossom key, presets test (M13), e2e scenarios, polish                                                                                                                    | US8, SC-011     |

## Complexity Tracking

No violations.
