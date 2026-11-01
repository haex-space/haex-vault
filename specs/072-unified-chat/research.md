# Research: One chat for the agent and for contacts, delivered over Nostr

Spec: [spec.md](./spec.md). Code facts come from the graph and reads on `f54cb1e9` (main, 2026-10-31).
Paths are relative to `src-tauri/src/` unless they start with `src/`, `specs/` or `docs/`.

## R1 Message transport: stored NIP-59 gift wraps to device keys

**Decision**: Every chat event is a rumor inside a seal inside a gift wrap of kind **1059** (stored by
Nostr-Relays), exactly the layers of spec 051's contract
([nostr-invite.md](../051-phone-contacts/contracts/nostr-invite.md)):

- the gift wrap is signed by a one-time key and encrypted (NIP-44 v2) to one **device key** of the
  recipient; tag `p` = that device key; `created_at` randomly up to 48 h in the past (NIP-59);
  tag `expiration` (NIP-40) = 30 days, so Nostr-Relays may drop it after the pickup window;
- the seal (kind 13) is signed by the sender's **device key** and encrypted to the same device key;
- the rumor is unsigned; its kind and JSON content are this spec's (contract
  [nostr-chat.md](./contracts/nostr-chat.md)).

The wrap is built with the existing `sync::presence::wrap_payload` / `unwrap_rumor` (presence.rs:225-305)
after generalising them: wrap kind and `created_at` tweak become parameters (presence keeps 21059 and
the real time), and the size limit becomes an argument (presence keeps 16 KiB).

**Rationale**: device keys exist on every device (`sync::keys::DeviceKeys`, keys.rs:64) and are valid
Nostr keys; the vault identity's secret is only on main devices, so addressing the vault key would
exclude linked devices (spec Assumptions). The 051 contract and ADR 0014 already chose these layers;
one implementation of wrapping (presence) is reused instead of a second (constitution "one
implementation per rule"). nostr 0.45 is in the tree with `nip44` and `nip59` (Cargo.toml:135).

**Alternatives considered**:

- NIP-17 kind 14/15 rumors addressed to a person's npub: interoperable, but needs the vault secret on
  every device or a new shared chat key; interop was ruled out (Clarifications 2026-10-31).
- nostr's own `EventBuilder::gift_wrap`: equivalent, but a second wrapping path next to presence's; the
  generalised presence functions keep one.
- Ephemeral kind 21059 (as presence): Nostr-Relays never store it, so no offline delivery.

## R2 Event size and text limit

**Decision**: A gift wrap must be ≤ **60 KiB**. With two NIP-44 layers (base64 ≈ ×1.33 each) the rumor
content may be ≤ **32 KiB**. Text is limited to **4,000 characters** (≤ 16 KiB UTF-8 worst case); the
rest of the budget holds the sender's card (R5) and attachment metadata. A rumor that would exceed the
budget is refused before sending (`chat.error.tooLarge`, FR-012). Group snapshots are split (R9).

**Rationale**: common Nostr-Relays accept 64 KiB events (strfry's default `maxEventSize` is 65,536;
others accept more). Presence's 16 KiB would leave ~8 KiB of rumor: too little for 4,000 non-ASCII
characters plus a card.

**Alternatives considered**: 16 KiB like presence (too small); splitting long texts over several
wraps (reassembly, partial delivery states; long texts can go as a file instead, edge case in spec).

## R3 One Nostr client per vault session for the chat

**Decision**: A new `chat::delivery` service owns its own `nostr_sdk::Client`, started and stopped with
the vault session from the same hooks as sync (`instances/open.rs:149-174`, `create.rs`, `import.rs`),
under the vault gate's tracker and cancel token. It:

- connects to the vault's effective Nostr-Relays (`ServerConfig::effective_nostr_relays`,
  servers.rs:91) for receiving;
- subscribes `{"kinds":[1059],"#p":[own device key]}` with `since` = last seen `created_at` − 48 h
  per relay (kept in `chat_relay_cursors_no_sync`);
- adds a contact's Nostr-Relays (from `contacts::card::received::addresses`, received.rs:180, or from a
  group member's stored card) as write-only relays on demand and publishes with `send_event_to`;
  idle write-only relays are removed after 10 minutes (`ponytail:` fixed idle timeout, upgrade:
  LRU with a cap);
- reacts to `wakeups.relays` / `network` (spec 043 app resume) by reconnecting and resubscribing.

**Rationale**: presence's client calls `unsubscribe_all()` on every refresh (presence_loop.rs:221-226)
and would kill the chat subscription; link host and join also run their own clients
(link/host_task.rs:368). A separate client is the existing pattern. Nostr-Relay changes apply on the
next vault open, as for presence (sync/commands.rs:450-483).

**Alternatives considered**: sharing presence's client (needs a subscription-id discipline across
modules and couples two lifecycles); a client per contact (connection count grows with contacts).

## R4 Deduplication, own devices and ordering

**Decision**:

- Every chat event carries a random 128-bit `id`. A message row's primary key is
  `UUIDv5(CHAT_NS, conversation id ‖ sender vault ‖ id)`; the receiver checks-then-inserts (no UNIQUE on
  CRDT tables, migrations_contacts.rs:6-10). The same message reaching a device by two Nostr-Relays, as
  wraps to two own devices, or by own-device sync lands on the same row (FR-015).
- Seen gift wrap ids are kept 30 days in `chat_wraps_seen_no_sync` so a wrap is decrypted once.
- The sender wraps every event to its **own other devices** too (device list of the own vault), and every
  sender wraps to every device on the recipients' cards, so all own devices receive everything even if
  they are never online together (FR-016). Own-device CRDT sync carries the same rows as well; both paths
  converge on the same primary keys.
- Order: `sort_at = min(sent_at, received_at + 5 min)` (FR-021, edge case "wrong clock"), ties by id.

**Rationale**: derived ids are the project's way to get uniqueness under CRDT (contacts::ids,
extensions::ids). Per-column LWW (haex-crdt row.rs) merges identical inserts without conflict.

**Alternatives considered**: relying only on own-device sync (fails when own devices are never online
together, US2 scenario 6); a UNIQUE index on message ids (halts sync on conflict).

## R5 Sender verification and the carried card

**Decision**: Every rumor carries the sender's **current signed Visitenkarte** (`holzi:card:` text,
~0.5–2 KiB). The receiver:

1. unwraps; drops if the wrap id was seen;
2. decodes and verifies the card (vault identity signature, contacts/card);
3. requires the seal's key to be a device on that card, not removed;
4. compares with the stored card of that vault (`received::stored`, received.rs:223): a card older than
   the stored one by `(list_generation, issued_at)` is refused (same threshold as `foreign_trust`,
   foreign_auth.rs:376-408); a newer card of an existing contact replaces the stored one through
   `received::replace` (spec 060 rule, FR-020);
5. drops everything from a blocked vault (`received::is_blocked`, received.rs:307) without receipts
   (FR-042);
6. checks the rumor's `to` (the conversation's member set or group id) includes the own vault.

Group members that are not contacts keep their latest verified card in the member row (R9), not in the
address book.

**Rationale**: always carrying the card keeps every receiver's view of the sender's devices and
Nostr-Relays current without a lookup service; the size fits R2. Reuses the card format and the
generation rule of 060/057 (one implementation).

**Alternatives considered**: carrying only a card hash (needs a fetch path for unknown hashes);
carrying the card only on change (needs per-recipient bookkeeping; the bytes saved do not matter at
chat volumes).

## R6 Delivery states, receipts and the outbox

**Decision**:

- Per recipient vault a `chat_deliveries` row with independent timestamp columns `sent_at`,
  `delivered_at`, `read_at`, `failed_at` + `failed_reason`, `files_at`. The displayed state is derived
  (failed > read > delivered > sent > waiting). Each column is written once, so concurrent writers on
  own devices merge per column without losing a state (haex-crdt per-column LWW).
- _sent_: at least one Nostr-Relay of the recipient answered OK to the publish.
- _delivered_: a receipt rumor (`receipt`, kind 24121) arrived. Only the device that **inserted** the
  message row (not the one that got it by own-device sync) sends the delivery receipt; receipts are
  batched per conversation and sent at most every 2 s. File receipts (`files`) are the exception: every
  device that stored a file confirms it for itself (R13).
- _read_: sent by the device on which the user opened the conversation, only with read receipts on
  (FR-024) and never for an unaccepted request (FR-043).
- The outbox `chat_outbox_no_sync` holds per event: payload, recipients still to do, attempts,
  `next_attempt_at`, last error. Back-off 5 s, 15 s, 1 min, 5 min, then every 15 min; after 24 h without
  any Nostr-Relay accepting, the delivery is _failed_ ("Erneut senden" re-queues). The outbox is
  device-local: the device that wrote the message sends it (`ponytail:` a message queued on a device
  that never comes back stays _waiting_; upgrade: hand-over to another own device via a synced claim).

**Rationale**: timestamp columns turn a monotone state into set-once fields, which per-column LWW merges
correctly; one receiving device per vault keeps receipts from multiplying.

**Alternatives considered**: one `state` column (two devices raising it concurrently may lower it
again); a receipt from every receiving device (N× wraps for no information).

## R7 No direct connection between vaults for the chat

**Decision**: Messages and files travel only through Nostr-Relays and Blossom-Servers (operator,
2026-11-01, spec Clarifications and FR-018). No new iroh protocol, no reachability check, no second
path to test.

**Rationale**: one path for online and offline; every device (recipients' and the sender's own) gets
everything the same way. The cost is that every file passes a Blossom-Server (encrypted) and file size
is bounded by that server (R12).

**Alternatives considered**: direct transfer over a new ALPN `haex-vault-chat/1` when both sides are
online, Blossom otherwise (the first plan draft; rejected by the operator for a single Nostr path);
spec 056's `haex-vault-files/1` (cannot deliver to a contact without a read-write share,
files/peer/serve/check.rs:21-29).

## R8 Files over Nostr-Relays without Blossom

**Decision**: Not done. A file is never split into Nostr events.

**Rationale**: Nostr-Relays take events up to about 64 KiB; a 100 MB file would be ~1,700 events per
recipient device, which Nostr-Relays rate-limit or ban, and no Nostr client does it. Blossom is the
Nostr way for files (BUD-01/02, NIP-B7; nostr 0.45 ships `nips::nipb7`).

## R9 Groups: membership as per-member last-writer-wins over ops

**Decision**:

- A group has a random id; its membership is a set of **member ops** `add`, `remove`, `leave`,
  `accept`, `decline` and **meta ops** `rename`, `picture`, each with `(op_id, at = sender time, by)`.
- Receivers reject an op whose `at` is more than five minutes ahead of their current time; this bounds
  sender-controlled future timestamps while allowing normal clock skew. Every member applies each
  accepted op it receives; per member the op with the greatest `(at, op_id)` wins;
  likewise per meta field. All members receive all ops (ops go to every current member plus the
  affected one), so they converge on the same list (FR-056) independent of arrival order.
- Ops by a vault that is not a current member at `at` are ignored. Any member may add or remove
  (FR-053/FR-054, no admins).
- An invitation (`group.invite`) carries the group id, name, the current member list with each member's
  signed card and state, split into parts so each wrap stays within R2 (≤ 12 cards per part); the
  invitee stores the parts and shows the invitation once all parts arrived (part count in each part).
- Members store `card` (latest verified) per member row; messages from a member are verified against it
  (R5) and update it when newer.
- Messages are only addressed to members in state `member` at send time; a new member never receives
  older messages (FR-051).
- Limit 32 members (FR-052): each message costs Σ devices wraps; 32 members × ~3 devices ≈ 100 wraps per
  message, still small for Nostr-Relays.

**Rationale**: NIP-17 identifies a group by its participant set; renaming or changing members would
create a new chat. Ops with LWW per member are the smallest model that supports add/remove/leave and
converges without a coordinator. One row per member (not a JSON list) is required by per-column LWW
(haex-crdt).

**Alternatives considered**: NIP-29 relay-based groups (a relay is the authority, contradicts
E2E-only Nostr-Relays); MLS / NIP-EE (much larger; ruled out for sync in 2026-09-27 decisions);
participant-set identity (no renames, no stable group).

## R10 Requests and unknown senders

**Decision**: A direct conversation whose other vault has no stored, non-blocked card
(`ContactsService::card_of_vault` → `Unknown`) is created with `state = request`. Group invitations
from anyone are requests until accepted (US4/US5); from a non-contact they show under "Anfragen" without
sound. Accepting a direct request runs spec 060's preview/merge with the carried card
(contacts/share/preview.rs, merge.rs) and sets `state = active`. Declining deletes the conversation
locally (sender gets a delivery receipt only, never _read_). A later message from the same vault creates
a new request.

**Rationale**: reuses 060's import path (one implementation) and 051's "Von Unbekannten" idea.

## R11 File encryption for Blossom

**Decision**: Per file a random 256-bit key. The file is encrypted in **1 MiB segments with
AES-256-GCM** (`aes-gcm` 0.11, Cargo.toml:184), nonce = 8 random bytes ‖ 32-bit segment counter, the
last segment flagged in the AAD (STREAM construction against truncation and reordering). Format header
`HXCF` v1 + segment size. Blossom addresses the blob by SHA-256 of the ciphertext; the rumor carries
key, ciphertext hash, plaintext SHA-256, size, MIME type and name. The receiver checks both hashes
(FR-032). Segmenting keeps memory bounded on Android for 100 MB files.

**Alternatives considered**: NIP-17 kind 15's single AES-GCM (whole file in memory, 100 MB on phones);
ChaCha20-Poly1305 (equally fine; AES-GCM matches NIP-17's choice and spec 048 already uses it).

## R12 Blossom protocol and keys

**Decision**: BUD-01/02: `PUT /upload` (body = ciphertext, header `Authorization: Nostr <base64 event>`
with a kind 24242 event, tags `t=upload`, `x=<sha256>`, `expiration`), `GET /<sha256>`,
`DELETE /<sha256>` (kind 24242 `t=delete`). `HEAD /upload` (BUD-06) first to learn size limits and
refusals. Each upload is signed with a **fresh key per blob**; its secret is kept in
`chat_blobs_no_sync` on the uploading device so that device can delete the blob later (BUD-02 ties
deletion to the uploader's key). Downloads need no auth.

Servers that only accept registered keys: the user may create a **Blossom upload key** in the chat
settings; it is stored as an entry of the password manager (tag `haex-blossom`, spec 052 pattern) and
its npub is shown to register at the server. With such a key, uploads use it instead of a fresh key.
This is how US8 scenario 2 ("credentials from or into the password manager") is met: Blossom has no
passwords, its credential is a key.

**Size limits and public servers** (checked 2026-11-01): Blossom itself has no size limit; each server
sets its own and reports it on `HEAD /upload` (BUD-06). nostr.build's Blossom server (blossom.band /
blossom.nostr.build) states "a hard cap of 100 MiB per upload (20 MiB for free uploads)" and that free
uploads accept images, audio and video only; the other servers probed (blossom.primal.net,
nostr.download, cdn.nostrcheck.me) require a signed auth event even for `HEAD /upload`, so their limits
could not be read without a key. Many public Blossom-Servers are media hosts for Nostr posts and may
refuse opaque encrypted blobs (`application/octet-stream`).

Consequences:

- Before every upload HaexVault sends `HEAD /upload` (with auth) and shows the server's refusal or limit
  before the message is sent (FR-034: 100 MB is HaexVault's own cap; a lower server limit wins).
- A server enters the preset list only after a test (implementation task) shows it accepts an
  encrypted 20 MB blob anonymously, reports limits and honours `DELETE`. If no public server passes,
  the default is "Keiner" and the settings explain the choice (FR-038); the presets live in code next
  to the default Nostr-Relays (servers.rs:35-39) and are reviewed when they change.
- Paid plans work with the Blossom upload key above (registered npub).
- A self-hosted Blossom-Server (open-source implementations exist, some store into S3) is entered as an
  own address; documenting a recommended setup is out of scope.

HTTP uses `reqwest` 0.13 with `rustls` and `stream` (Cargo.toml:247) through the pinned client
(remote_storage/http.rs:19, no redirects, no proxy); upload streams the segments
(`Body::wrap_stream`, as webdav/upload.rs:84).

**Alternatives considered**: device key for uploads (links all uploads of a device for the server);
one vault-wide upload key always (same linking; kept only as opt-in for servers that need it); own S3
from spec 038 (operator chose Blossom).

## R13 Blob lifetime

**Decision**: Every device that stores a file sends a receipt with `files` (the attachment index) for
**that device**. The uploading device keeps in `chat_blobs_no_sync` the set of device keys still expected
(every device on every recipient's card at send time plus the sender's own other devices). When all have
confirmed, it `DELETE`s the blob. A sweep on vault open and every 6 h deletes blobs older than 30 days
(FR-035). Devices that get `404` show "Datei nicht mehr verfügbar". A device added to a vault after the
blob was deleted has no way to get the file (spec FR-033).

`ponytail:` a recipient device that is never opened again keeps the blob until the 30-day sweep;
upgrade: delete earlier when the recipient's newer card no longer lists that device.

**Alternatives considered**: delete after one device per recipient vault confirmed and let other own
devices fetch from it (needs a direct device-to-device path, ruled out in R7).

## R14 Storing chat files on the device

**Decision**: Received and sent files are kept in a vault-owned encrypted folder (spec 048) per
device, modelled on `agent_browser::profile_store::ProfileStore` (profile_store.rs:30-121):
`<app_local_data>/chat-files/<vault instance>/` holds an encrypted folder `Chat`, created if missing;
files are written under `<conversation id>/<attachment id>` so equal names never clash. Whether this
device holds a file, and the state of its download, is device-local (`chat_attachment_files_no_sync`).

`files::encrypted::transfer::put_file` seals a whole file in memory (transfer.rs:256). `ponytail:` chat
files ≤ 100 MB use it; ceiling = one file in memory at a time (a semaphore of 1 for chat file writes);
upgrade = a streaming put in `files::encrypted` (also wanted by 048 for big files).

"Öffnen" goes through the files viewer (`files_open` → `Opened`, browser_commands/view.rs:58-80;
`Viewer.vue`, `PdfViewer.vue`, `MediaViewer.vue`) with a chat source; "Mit System-App öffnen" through
`files_open_system` (view.rs:306, desktop only; on Android the action is hidden, as today).
"Speichern unter" uses `pickSaveTargetAsync` (src/composables/usePickedFile.ts) and Rust writes through
`tauri_plugin_fs` (files/picked.rs), which handles Android `content://` targets.

**Alternatives considered**: CRDT BLOBs like password attachments (passwords/binaries.rs; syncs every
file to every own device, 25 MiB limit; too heavy for 100 MB files); plain files in the download folder
(unencrypted at rest; violates the encrypted-storage rule).

## R15 Data model: extend `chat_threads`, new tables for contact messages

**Decision**: `chat_threads` stays the one list of conversations and gets `kind` (`agent` default,
`direct`, `group`), `state`, `peer_vault`, `muted`, `last_read_at`, `picture`. Contact messages go into a
**new** table `chat_peer_messages` instead of `chat_messages`: agent messages keep their LLM roles and
validation (storage/chat_messages.rs:112-151), and spec 068 can change them without touching contact
messages. Details in [data-model.md](./data-model.md). Migration `0049_chat_contacts`;
`HOLZI_TRIGGER_VERSION` 30 → 31 (new CRDT tables and columns, migrations.rs:121).

**Alternatives considered**: one message table with nullable sender and LLM columns (role validation
and 068 rework would entangle both kinds); a separate conversation table (two lists to merge, two
navigation paths, the agent list's CRUD duplicated).

## R16 Agent access

**Decision**:

- New `AgentFileKind::ContactChats` in `agent_file_permissions` (files/permissions.rs:28-36), targets `*`
  or `thread:<id>`; asked through `PermissionPrompt` with a new `ask_contact_chats` (pattern of
  `ask_contacts`, files/agent/prompt.rs), "Einmal erlauben" / "Immer erlauben" / "Ablehnen"; a refusal is
  mapped to "not found" (pattern `contacts::agent::hidden`, agent.rs:92).
- Native executor `chat::agent_actions` (pattern contacts/agent_actions.rs) with actions
  `chat.contacts.conversations.list`, `chat.contacts.messages.read` (serves external agents), and
  `chat.contacts.message.send` with `confirming_action_ids` + mandatory dialog (pattern
  contacts/agent_share.rs + agent_share_confirm.rs: event, 120 s timeout, answer command); the dialog
  shows text, attachments and recipients.
- MCP: data scope `chat.contacts.read` in mcp_server/grants.rs:294-327; send stays `confirmsItself`
  (mcp_server/dispatch.rs:236-243).
- Agents never get group, request, block or settings actions (FR-072): none are agent-callable.

**Rationale**: the same three-part pattern (grant table, prompt, confirm executor) already guards
contacts and passwords; reusing it keeps one permission UI (AgentPermissionDialog.vue).

## R17 Notifications

**Decision**: `chat::notify` shows a system notification through `tauri-plugin-notification` (Cargo.toml:347,
patched for desktop clicks) for a new message in a conversation that is not muted and not open in a
focused window, while the vault is open (FR-023). Tap routes to the chat app with the conversation, via
the same `on_action` mechanism as extensions/desktop.rs:65-95, generalised into a shared
`notifications` module with a typed target instead of a second registry. Requests: title only ("Neue
Anfrage"), no text. Android: permission requested on first use (desktop.rs:135-152 pattern).

## R18 Frontend structure

**Decision**:

- `ChatApp.vue` (493 lines) is split: `ChatApp.vue` keeps layout, list and navigation; the existing agent
  parts move into `chat/AgentConversation.vue` unchanged in behaviour; new `chat/PeerConversation.vue`
  (message list, composer without agent controls, members panel for groups).
- `ThreadSidebar.vue` becomes the conversation list with kind filter, avatars (`ContactsAvatar`), unread
  badge (`ShadcnBadge`) and "Anfragen" entry.
- Routes (appRoutes.ts:277-285): keep `''` and `thread/:id` for every kind, add `requests` and
  `requests/:id`. The `Thread` type gains `kind`, `state`, `peerVault`, `muted`, `unread`.
- New: `NewConversationDialog.vue` (agent / contact / group), `PeerMessageList.vue`,
  `AttachmentBubble.vue`, `ContactCardBubble.vue`, `GroupMembersPanel.vue`, `RequestBanner.vue`,
  `SendConfirmDialog.vue` (agent send, mounted beside ShareConfirmDialog in
  `src/pages/workspace/[instance].vue`).
- Address book `EntryPage.vue`: "Nachricht senden" next to "Teilen" when `contact.card` exists and not
  `isSelf`. `ShareDialog.vue`: target "Im Chat senden" with a conversation picker; the attachment is
  the 060 share output plus the photo (FR-060).
- Settings: category `chat` in `src/lib/settings/registry.ts`, view `settings/ChatView.vue`;
  `scripts/check-settings.ts:119` expects seven categories.
- Actions in `src/lib/actions/chatActions.ts` get `chat.contacts.*` (agent-callable per R16) and UI
  actions for groups/requests (not agent-callable); i18n de/en; literal `lucide:*` icon names
  (nuxt.config.ts:79-88 icon scan).

## R19 Test rigs

**Decision**:

- Rust integration tests with `MockRelay` (nostr-sdk `local-relay`, Cargo.toml:436) and a test
  Blossom server on `hyper` (already a dev-dependency, Cargo.toml:458) in
  `src-tauri/tests/support/blossom.rs` (in memory: PUT/GET/HEAD/DELETE, auth check, size limit).
- Multi-vault test support like `extensions/peers/test_support.rs`: two and three vaults with cards
  exchanged, one with two devices.
- E2E (spec 016/033 rig): `e2e_nostr_relay` exists; a new `src/bin/e2e_blossom.rs` behind the `e2e`
  feature using `hyper` as an optional dependency (no new crate). Scenarios in quickstart.md.
- Only the relevant test targets are built (memory: full builds crashed VS Code).

## R20 Relationship to spec 068 and ordering

**Decision**: 068 has no code yet (0/52 tasks). This spec moves the agent UI into
`AgentConversation.vue` and adds `kind` to `chat_threads` with default `agent`; 068's later changes to the
turn engine and agent UI land in those places. Nothing here touches `chat/turn/`, `ChatState` or
`model_jobs`.

## R21 Graph check (constitution "graphify first")

Queried for: gift wrap, presence wrap, nostr client, foreign auth, peer protocol, encrypted folder
store, attachment viewer, agent permission prompt, confirm dialog, notification, chat thread,
conversation, delivery receipt, outbox, blossom. Reused: `presence::wrap_payload`/`unwrap_rumor`,
`DeviceKeys`, `ServerConfig::effective_nostr_relays`, `received::{stored, addresses, replace, is_blocked}`,
`ProfileStore` pattern, `files_open`/`files_open_system`,
`PermissionPrompt`, `agent_share_confirm` pattern, `extensions::desktop` notification routing,
`preferences` accessors, `vault-data-changed`. No existing outbox, receipt, Blossom or group-membership
code exists; these are new. No iroh protocol is added (R7).
