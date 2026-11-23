---
description: 'Task list for more secure contact chat (spec 095)'
---

# Tasks: More secure contact chat

**Input**: Design documents from `/specs/095-chat-security/`

**Prerequisites**: [plan.md](plan.md), [spec.md](spec.md), [research.md](research.md), [data-model.md](data-model.md), [contracts/nostr-chat-ratchet.md](contracts/nostr-chat-ratchet.md), [quickstart.md](quickstart.md), [ADR 0026](../../docs/adr/0026-chat-ratchet-per-device-pair.md)

**Tests**: Included. The success criteria (SC-001 to SC-011) are security properties that only tests show, and the repository rules require tests for boundaries and failure behaviour. Rust tests live in sibling `*_tests.rs` files, never inline.

**Organization**: Grouped by user story. US1, US2 and US3 are all P1; their order here is the order of the plan's slices (sessions on the wire, then reset and sending again, then keep-alive and cleanup), because each builds on the one before. US1 alone already replaces the long-lived keys.

**State of the code this builds on** (checked 2026-11-17): the chat with contacts has direct conversations and notes; group conversations (spec 072 US4) are not built. Sessions belong to pairs of devices, so groups get them through the same outbox once they exist (T048).

## Format: `[ID] [P?] [Story] Description`

- **[P]**: can run in parallel (different files, no dependency on an unfinished task)
- **[Story]**: US1 to US4 as in spec.md; Setup, Foundational and Polish tasks carry none
- Paths are relative to the repository root

## Commands used by the verification tasks

- Rust, through the host bridge: `nix develop --command scripts/with-nix-host-bridge.sh cargo <args>` (a plain `nix develop --command cargo` fails in the GTK/WebKit sys crates). On a memory-limited machine use `-j 1` and `CARGO_PROFILE_DEV_DEBUG=line-tables-only CARGO_INCREMENTAL=0`; host builds outside the bridge need `env -u CC -u CXX -u AR`.
- Frontend and scripts: `nix develop --command pnpm <script>`; in a fresh worktree run `nix develop --command pnpm install --frozen-lockfile` first.
- After `cargo test` regenerates ts-rs bindings: `sed -i -E "s/[[:space:]]+$//" src/types/bindings/*.ts`.

## Names and numbers used below

Fixed here so every task means the same thing; reasons are in [research.md](research.md#checks-done-for-tasks).

| Name                     | Value                                                        |
| ------------------------ | ------------------------------------------------------------ |
| `RATCHET_KIND`           | rumor kind 24122 (`chat.ratchet`)                            |
| `RESET_KIND`             | rumor kind 24123 (`chat.reset`)                              |
| `KEY_RECORD_KIND`        | event kind 10120 (chat key record)                           |
| `MAX_PLAINTEXT_BYTES`    | 20 KiB (20,480): the plaintext of a ratchet message          |
| `START_KEY_RENEW_MS`     | 30 days                                                      |
| `KEEP_ALIVE_MS`          | 24 hours                                                     |
| `RENEWAL_OVERDUE_MS`     | 7 days                                                       |
| `RESET_EVERY_MS`         | 1 hour                                                       |
| `RESEND_AFTER_MS`        | 1 day, 3 days, then 7 days                                   |
| `REACK_AFTER_MS`         | 12 hours                                                     |
| `MAX_SESSIONS`           | 4 per peer device                                            |
| Session config           | `SessionConfig::version_1()` everywhere                      |
| Session of a peer to use | the retained one with the smallest session id (string order) |

---

## Phase 1: Setup

**Purpose**: the dependency, the module skeleton and time that tests can move

- [x] T001 Add `vodozemac = { version = "=0.11.2", default-features = false }` to `src-tauri/Cargo.toml` (Olm only; the libolm pickle format is not used) and run `cargo check` through the host bridge and `cargo check --target aarch64-linux-android --no-default-features`. `cargo tree -d` must show no second `curve25519-dalek`, `ed25519-dalek` or `x25519-dalek`.
- [x] T002 Create `src-tauri/src/chat/peer/ratchet/mod.rs` with the module documentation (spec 095, ADR 0026: what the module owns, that it reads and writes only this installation's rows, that no secret reaches a log or an error) and the constants of the table above; declare `pub mod ratchet;` in `src-tauri/src/chat/peer/mod.rs`. Submodules are added as `pub(crate) mod` lines as their files appear.
- [x] T003 Time that tests can move: `Delivery` in `src-tauri/src/chat/peer/delivery.rs` gets a clock field (`Arc<dyn Fn() -> i64 + Send + Sync>`, milliseconds) passed to `Delivery::connect`; every `now_ms()` call inside `impl Delivery` (also `delivery_run.rs`) uses it; `service.rs` passes `now_ms`. `chat::peer::wrap::wrap` takes the time as an argument and `sync::gift_wrap` gets a variant of `wrap_shaped` that draws `created_at` and sets `expiration` from that time (presence keeps the existing function). `src-tauri/src/chat/peer/test_support.rs` gets a clock the tests set and advance. First find out, in a test in `src-tauri/src/chat/peer/delivery_tests.rs`, how the in-process Nostr-Relay treats events dated in the future and events past their expiration; that decides whether the later tests move their clock forward from now or start it in the past, and write the answer into the doc comment of the test clock.

---

## Phase 2: Foundational (blocks every user story)

**Purpose**: storage, the ratchet module and the chat key record. Nothing on the wire changes; the chat keeps working as before.

**CRITICAL**: no user story starts before this phase is done

### Storage and ratchet module (pull request A)

- [x] T004 Migration with the next free number (`0063_chat_ratchet` when this list was written) in `src-tauri/src/identity/migrations_chat_ratchet.rs`, registered in `src-tauri/src/identity/migrations.rs`, tests in `src-tauri/src/identity/migrations_chat_ratchet_tests.rs`. Tables as in `data-model.md`: `chat_ratchet_account_no_sync` (`installation_uuid` primary key, `account`, `start_key_made_at`, `record_published_at`), `chat_ratchet_sessions_no_sync` (primary key `installation_uuid`, `peer_device`, `session_id`; `session`, `peer_identity_key`, `created_at`, `last_received_at`, `last_sent_at`, `own_ratchet_key`, `own_key_since`, `renewed_at`), `chat_ratchet_peers_no_sync` (primary key `installation_uuid`, `peer_device`; `reset_sent_at`, `reset_received_at`, `recovering_since`, `refetch_record`), `chat_key_records_no_sync` (`peer_device` primary key, `identity_key`, `start_key`, `made_at`, `valid_until`), `chat_outbox_sealed_no_sync` (primary key `outbox_id`, `recipient_device`; `wrap`, `accepted_at`); and on `chat_outbox_no_sync` the columns `awaiting` (JSON list, default `[]`), `accepted_at`, `resends` (default 0). Device keys and vaults are hex text as in the other chat tables; pickles are JSON text. No synchronized table changes, so `HAEX_TRIGGER_VERSION` stays. Tests: every table exists and is outside the sync, the outbox keeps its rows.
- [x] T005 [P] `src-tauri/src/chat/peer/ratchet/wire.rs` with tests in `wire_tests.rs`: `RatchetRumor { v, type, body }` (body = base64 of the Olm message bytes) and `Plaintext { kind, content }` exactly as in the contract, `encode`/`decode` that refuse an unknown `v`, a `type` other than 0 or 1, and a plaintext `kind` other than 24120, 24121, 0 or 2; `Plaintext::encode` refuses more than `MAX_PLAINTEXT_BYTES` with `envelope::too_large()`. Test: an opening message (`type: 0`) with a plaintext of exactly `MAX_PLAINTEXT_BYTES` wraps to at most `wrap::MAX_WRAP_BYTES`, and keep-alive request/reply plaintexts use kinds 0 and 2.
- [x] T006 [P] `Me` in `src-tauri/src/chat/peer/session.rs` gains `installation: Uuid`; `service.rs` passes the installation id it already reads, `test_support.rs` the device's one. A helper `targets::vault_of_device(q, me, device) -> Option<String>` in `src-tauri/src/chat/peer/targets.rs` answers the vault (hex) on whose stored card a device key stands as a non-removed device: a contact's stored card, the card of an open request (`store::request_card`), or the own device list; `None` otherwise and for a blocked vault. Tests in `src-tauri/src/chat/peer/targets_tests.rs`.
- [x] T007 `src-tauri/src/chat/peer/ratchet/store.rs` with tests in `store_tests.rs`: reading and writing the rows of T004 for one installation. Every function takes the installation id and puts it into the `WHERE`; none reads, changes or deletes a row of another installation (test: rows of a second installation are byte-identical after every function ran, SC-007). Chat key records come with T011 and the stored wraps with T019, where they are first used. Pickles are handled as `Zeroizing<String>`; no type that holds one derives `Debug` with its content.
- [x] T008 `src-tauri/src/chat/peer/ratchet/account.rs` with tests in `account_tests.rs`: `load_or_create(tx, installation, now)` (a new `vodozemac::olm::Account` with one start key from `generate_fallback_key`, `start_key_made_at = now`), `own_keys` (ratchet identity key and current start key), `renew_start_key(tx, installation, now)` (`generate_fallback_key`; the account then holds the current and the previous start key). Tests: an opening made for a start key still creates a session after one renewal and no longer after two; an installation without a row gets its own account while the row of another installation stays (scenario 5).
- [x] T009 `src-tauri/src/chat/peer/ratchet/session.rs` with tests in `session_tests.rs`. Depends on T007, T008.
  - `open_outbound(tx, me, peer_device, record, now)`; `encrypt(tx, me, peer_device, plaintext, now) -> RatchetRumor` on the session with the smallest id; `decrypt(tx, me, peer_device, rumor, now) -> Decrypted { plaintext, opened: bool } | Undecryptable`: `type: 0` on the session its id names, else a new inbound session; `type: 1` tried on every retained session with that device, and only the one that decrypted is saved.
  - A session created by an opening is saved only when the caller confirms it (the receiver checks run after decrypting, T021); an opening whose identity key differs from the retained sessions' `peer_identity_key` deletes those sessions when it is confirmed.
  - At most `MAX_SESSIONS` per peer device; the one whose last use (the newest of `last_received_at`, `last_sent_at` and `created_at`) is longest ago goes, ties by session id.
  - Renewal: on encrypt, when the message's ratchet key differs from `own_ratchet_key`, store it and set `own_key_since = now`; on decrypt, a throwaway copy of the advanced session encrypts once, and when that copy would send with another ratchet key than `own_ratchet_key`, set `renewed_at = own_key_since` if that is set. `renewed_at` never moves otherwise. (Comparing the message's ratchet key with the last one seen, as first planned, would count a message delayed on an older chain of the peer as an answer; vodozemac does not expose its chains, the copy shows whether the session advanced.)
  - Every operation loads the pickle inside the caller's write transaction and stores it in the same one; no session is held in memory between operations. The vault's write transactions run one after another, which is the per-session lock of `data-model.md`.
  - Tests: round trip in both directions; opening messages until the first answer; 40 skipped messages decrypt later and a gap of 41 loses the first one; a message delivered twice is `Undecryptable` the second time; two devices that open at the same moment and receive both openings in either order then send on the same session; a fifth session removes the least recently used; with a pickle copied at a time T, `renewed_at` stays at or before T until this device sent with a ratchet key made after T and received an answer with a new peer ratchet key, and from then on the copy decrypts nothing (FR-022, SC-010 at module level); 50 sends, receives and keep-alives issued concurrently through `VaultDb::write` lose no advance (if this test fails, add a lock keyed by installation, peer device and session id around load, crypto and commit).
- [x] T010 [P] Erasure at storage level (quickstart scenario 9, step 3) in `src-tauri/src/chat/peer/ratchet/erasure_tests.rs`: an unencrypted SQLite file opened with `rusqlite` in WAL mode with `secure_delete = ON` and the tables of T004. Write a session pickle, checkpoint, find a byte string of its key material in the file (positive control); replace the pickle by the advanced one, delete the row, close with `wal_checkpoint(TRUNCATE)`; the earlier bytes are in neither the file nor the `-wal` file. If the update leaves the earlier bytes behind, write the row as delete plus insert in `store.rs` and say so in `research.md` R9.

### Chat key record (pull request B)

- [x] T011 `src-tauri/src/chat/peer/ratchet/record.rs` with tests in `record_tests.rs`: `own_event(me, account, now)` builds the kind 10120 event of the contract, signed by the device key, `until` = `start_key_made_at` + 30 days, `expiration` tag = `until`; `accept(tx, me, event, now)` checks the signature, the kind, `v`, that the signer is a device `targets::vault_of_device` knows, that `until` has not passed by this device's clock, and that the record is not older than the stored one, then stores it in `chat_key_records_no_sync`; a stored record with another identity key deletes the sessions with that device. Tests: each refusal (expired, older than the stored one, unknown or removed signer, wrong kind, bad signature, unknown `v`), a newer record replaces, a changed identity key drops the sessions, an older record handed back by a relay is refused (scenario 7, step 4).
- [x] T012 `src-tauri/src/chat/peer/delivery_records.rs` (an `impl Delivery` block like `delivery_run.rs`) with tests in `delivery_records_tests.rs`: `publish_own_record()` to the Nostr-Relays on the own card, or to `targets::listening_relays` when the card names none, setting `record_published_at` when one accepted; `fetch_record(device, relays)` with the client's fetch and a timeout, newest event first, through `record::accept`. Tests with two vaults on the in-process relay: the record is on the relay, the other vault fetches and accepts it; a vault whose card names no relay publishes where its contact looks for it.
- [x] T013 Renewal without the user in `src-tauri/src/chat/peer/delivery_run.rs`: at the start of a vault session run one `catch_up()` and, once every fetched wrap was processed and committed, create the account if it is missing, publish the record if `record_published_at` is empty or the own Nostr-Relays changed since, and call `renew_start_key` plus a publish when `START_KEY_RENEW_MS` passed since `start_key_made_at`; repeat that check after every later catch-up. Tests in `delivery_records_tests.rs` with the test clock: no renewal before 30 days, one after, none while the first catch-up has not finished, the previous start key still opens a session afterwards. (Done with migration `0066_chat_record_relays`, which keeps the relays that accepted the record, so the device republishes when one is missing and not on every open; a catch-up counts only with a connected Nostr-Relay, since a fetch from unreachable relays ends empty without an error.)

**Checkpoint**: accounts and records exist and renew; messages still travel as in spec 072

---

## Phase 3: User Story 1 - Old copied messages stay protected after a device key is exposed (Priority: P1) MVP

**Goal**: every chat event is encrypted on the session with the receiving device; the earlier format is neither sent nor accepted (FR-016). A relay archive plus a device key and key state taken later decrypt nothing whose keys were erased.

**Independent Test**: quickstart scenarios 1, 8 and 9, as the tests below.

**This is the one change of the wire format**: after pull request C, development vaults must be updated together.

### Tests for User Story 1

- [x] T014 [P] [US1] Move `src-tauri/src/chat/peer/two_vaults_tests.rs` and `own_devices_tests.rs` to sessions: a message, its delivery receipt and a read receipt arrive; an own other device gets the own message; the in-process relay holds only kind 1059 events and kind 10120 records; the first message to a vault whose devices are all offline is delivered when one comes online without the sender being there (scenario 8, step 1); a device without a record gets nothing, the message stays pending for it and is delivered after the record appears (scenario 8, step 2).
- [x] T015 [P] [US1] Forward secrecy in `src-tauri/src/chat/peer/forward_secrecy_tests.rs` (declared in `mod.rs` like the other test modules), scenario 1: two vaults exchange messages in both directions while the relay keeps every event; the recipient renews its start key twice (after a catch-up each time) and closes and opens its vault; then the test takes the recipient's device secret and all its ratchet rows and tries every archived wrap: wraps and seals open, no ratchet message decrypts. Also: a session state that holds exactly one skipped message key decrypts that message once and no earlier one (SC-002; vodozemac does not hand out single message keys).
- [x] T016 [P] [US1] Receiving in `src-tauri/src/chat/peer/delivery_tests.rs`: a fetched batch of 60 messages in reverse order stores all 60 in order; a rumor of kind 24120 or 24121 directly in a seal is dropped and stores nothing; a wrap past its expiration or without one is dropped before it is opened; a keep-alive from a device on no stored card is dropped and leaves no session; an opening whose envelope fails the spec 072 receiver checks leaves no session; an event sealed by another device than the session's is dropped.
- [x] T017 [P] [US1] Crash safety (scenario 9, steps 1 and 2) in `src-tauri/src/chat/peer/crash_tests.rs`: a failure injected before, inside and after the transaction that saves a session, on sending and on receiving; after opening again, session and message rows agree (a stored wrap exists exactly when the session advanced for it; a stored message exists exactly when its wrap is marked seen) and the conversation goes on.

### Implementation for User Story 1

- [x] T018 [US1] Sizes in `src-tauri/src/chat/peer/envelope.rs` and `send.rs`: `MAX_RUMOR_BYTES` and its check in `envelope::encode` go; `send::message` and `send::receipt` build the `Plaintext` around the encoded event and `Plaintext::encode` refuses what is over `MAX_PLAINTEXT_BYTES` (the event's quotes are escaped once more inside the plaintext, so only the plaintext's size counts); the trial wrap in `send::message` goes. The outbox keeps storing kind and content of the spec 072 event (the sender must be able to encrypt it again). Adjust `send_tests.rs` and the size cases in `wrap_tests.rs`.
- [x] T019 [US1] Outbox in `src-tauri/src/chat/peer/outbox.rs`: `enqueue` takes `awaiting` (for a message: the recipient vaults other than the own one; for a receipt: none); functions for the stored wraps (`sealed`, `store_sealed`, `mark_sealed_accepted`, `drop_sealed_of_device`); `record` removes a vault from `pending` when a wrap was accepted for at least one of its devices and for every device that has a session or a usable record (a device without a record is tried again with every later attempt at the row), sets `accepted_at` on the first accepted wrap, and deletes a row only when `pending` and `awaiting` are both empty. The 24-hour end stays as it is until T033. Tests in `outbox_tests.rs`.
- [x] T020 [US1] Sending per device in `src-tauri/src/chat/peer/delivery_send.rs` (an `impl Delivery` block; `Delivery::publish` in `delivery.rs` calls it). For each device of a target: use the stored wrap if there is one; else, if there is neither a session nor a usable stored record, `fetch_record` first; then in one write transaction open the session if needed, encrypt the plaintext, build the wrap and store it; then publish that wrap and mark it accepted when a Nostr-Relay took it. A device without a usable record is left out, reported as not yet done with a fixed reason, and tried again with the next attempt; nothing is ever wrapped for it under the device key alone. Makes T014 pass together with T021.
- [x] T021 [US1] Receiving in `src-tauri/src/chat/peer/delivery_receive.rs` (an `impl Delivery` block; `Delivery::receive` moves there). Per wrap, in one write transaction as today: first sight, relay cursor, expiration, open. Then by rumor kind: 24120/24121 dropped with a new `verify::Drop::EarlierFormat`; `RESET_KIND` ignored until T030; `RATCHET_KIND` decrypted with `ratchet::session::decrypt`. A keep-alive is accepted only when `targets::vault_of_device` knows the sender device. A message or receipt goes through `verify::check` unchanged (as an `Opened` with the seal's device key) and then through `receive::message` or `receive::receipt`. Only then is a session that this event opened confirmed; an event that fails leaves an existing session advanced and a new one unsaved. `Undecryptable` is logged with a fixed text until T030. Makes T016 pass.
- [x] T022 [US1] Order within a batch in `delivery_receive.rs` and `delivery_run.rs`: `catch_up` and the live reader hand over what they have as one batch (the live reader takes everything that is waiting in its channel); the batch is opened first, then sorted per sender device (openings by session id and counter, other messages by ratchet key and counter, read without decrypting) and stored one event per transaction in that order.
- [x] T023 [US1] No way around a session: `chat::peer::wrap::wrap` gets a doc comment that only `delivery_send.rs` calls it, and a test in `src-tauri/src/chat/peer/wrap_tests.rs` (or a check script next to the other source checks in `scripts/ci/`, whichever pattern the repository already uses for such greps) fails when another production file under `src-tauri/src/chat/peer/` calls `wrap::wrap` or names `CHAT_KIND`/`RECEIPT_KIND` as a rumor kind for wrapping.
- [x] T024 [US1] The vault's settings (scenario 9, step 4): a test beside the existing close tests of `src-tauri/src/instances/` that `PRAGMA secure_delete` is on in an opened vault and that the write-ahead log file is empty after `close`.

Done in pull request C. Beyond the tasks above: `receive::receipt` already removes the vault from `awaiting` (named in T031), so a row ends once its receipts came; until T031 a row whose vaults were all accepted stays parked for its receipt. `drop_sealed_of_device` comes with T029, where it is first used. A wrap is marked seen in the same transaction that stores its event (the batch is opened without writing first), and a wrap past its `expiration` or without one is dropped there too. T015 and T017 are declared in `delivery.rs`, since they drive the delivery's internals. T024: `secure_delete` after opening was already tested in `storage/maintenance_tests.rs`; `instances/close_tests.rs` adds the empty write-ahead log after the close. The chat test support's `start` waits until the device's record is on a Nostr-Relay, since a contact reaches a device only from then on.

**Checkpoint**: US1 works alone. A session that breaks does not heal yet (US3), a quiet session does not renew yet (US2).

---

## Phase 4: User Story 3 - Relay interference does not silently erase the only copy (Priority: P1)

**Goal**: a message without a delivery receipt is never given up; a session that broke on either side heals by a reset; nothing is lost or shown twice.

**Independent Test**: quickstart scenarios 6 and 7, as the tests below.

### Tests for User Story 3

- [x] T025 [P] [US3] Relays that drop, reorder and replay (scenario 6) in `src-tauri/src/chat/peer/relay_interference_tests.rs`: two in-process relays, one accepting and withholding, one delivering in reverse order and repeating; 60 messages in a row with a restart of sender and recipient in between; every message is stored once and in order. _Delivered_ appears only after the recipient's receipt. A message that every relay withholds for more than 24 hours of test time stays in the outbox, is encrypted and published again after 1 day, 3 days and then every 7 days, and arrives once a relay delivers (SC-004, SC-005, SC-009).
- [x] T026 [P] [US3] Lost and older key state (scenario 7) in `src-tauri/src/chat/peer/reset_tests.rs`: a device whose sessions were deleted receives from its peer; a vault file put back to an older copy on the same installation sends and receives. In both, a reset is sent, both sides end with one new session, and every message without a receipt arrives exactly once without a user action (SC-011). A second undecryptable event within the hour sends no second reset; an undecryptable event from a device on no stored card sends none; a wrap delivered twice or past its expiration sends none.
- [x] T027 [P] [US3] Receipts in `src-tauri/src/chat/peer/delivery_tests.rs`: a receipt that never arrives makes the sender send again, the receiver stores nothing new and acknowledges again, and the sender's outbox row ends; a duplicate that arrives within 12 hours (the copy another own device synchronized) is not acknowledged again.
- [x] T028 [P] [US3] No end and a clear failure in `src-tauri/src/chat/peer/outbox_tests.rs`: after 24 hours without any relay accepting, the delivery is `failed` with `noRelay` and the row is still tried at the last back-off step; when a relay accepts later, the failure is cleared and `sent_at` set; `noDevices` and `tooLarge` stay parked as today.

### Implementation for User Story 3

- [x] T029 [US3] `src-tauri/src/chat/peer/ratchet/reset.rs` with tests in `reset_tests.rs` of the module: `undecryptable(tx, me, peer_device, now) -> bool` (when `targets::vault_of_device` knows the device and `reset_sent_at` is older than `RESET_EVERY_MS`: delete every session with it, set `reset_sent_at` and `recovering_since`, answer `true`); `received(tx, me, peer_device, now)` (same conditions with `reset_received_at`: delete every session with it, set `refetch_record` and `recovering_since`, drop the stored wraps of that device for every outbox row whose `awaiting` holds its vault, put that vault back into `pending` and make those rows due now). `recovering_since` is emptied by `session::decrypt` when it decrypts on a session created after it.
- [x] T030 [US3] Sending and accepting a reset in `delivery_send.rs` and `delivery_receive.rs`: after `reset::undecryptable` answered `true`, wrap a `RESET_KIND` rumor with the content `{"v":1}` to that device and publish it once to its target's relays; if no relay accepts, empty `reset_sent_at` so the next undecryptable event tries again. A received `RESET_KIND` goes to `reset::received` and kicks the outbox. `delivery_send.rs` fetches the record again before it opens a session for a device with `refetch_record`, and clears the flag when a record was accepted. Makes T026 pass.
- [x] T031 [US3] Sending again in `outbox.rs` and `delivery_send.rs`: a row with `accepted_at` and a non-empty `awaiting` is due again `RESEND_AFTER_MS` after its last attempt (1 day, 3 days, then 7 days by `resends`). Before the attempt, drop from `awaiting` every vault whose delivery row has `delivered_at` (the receipt may have come through the sync between own devices); for the rest drop the stored wraps, put the vaults back into `pending`, count `resends`, and for a device on whose session nothing was ever received fetch the record again and open a new session if its identity key or start key changed. `receive::receipt` removes the vault from `awaiting` of the row of each message it names. Makes T025 pass.
- [x] T032 [US3] Acknowledging a resend in `delivery_receive.rs`: a message that `receive::message` reports as already stored, that came from the other vault, and whose row has `received_at` older than `REACK_AFTER_MS`, is put on the list of delivery receipts like a new one. Makes T027 pass.
- [x] T033 [US3] No end in `outbox.rs`: `GIVE_UP_MS` no longer parks a row; it marks `failed` with `noRelay`, once, the deliveries of the vaults that have stored wraps of which none was accepted, and keeps trying at the last back-off step; an accepted wrap clears `failed_at` and `failed_reason` of that delivery. A vault that waits only because its devices have no record is not marked failed (the conversation shows `waiting for keys`). `resend` ("Erneut senden") makes the row due at once as before. Update the module documentation. Makes T028 pass.
- [x] T034 [US3] Deleting ends sending: `store::delete` in `src-tauri/src/chat/peer/store.rs` removes the outbox rows of the conversation and their stored wraps (check what it removes today; the stored wraps are new). Test in `store_tests.rs`.

Done in pull request D. Beyond the tasks above: accepting a reset also drops the device's wraps that no Nostr-Relay took yet on every row (own copies and receipts too), since they are on sessions the device no longer has; those rows are due at once. `refetch_record` is cleared by `record::accept`, so any accepted record of the device ends it, and `seal` opens no session while it is set. A resend starts in `outbox::start_resend` (called by the pump): a row whose message is gone, here or through the sync from another own device, ends there, which also covers T034 for the other own devices. The resend schedule needs no new column: `next_at` of a row that awaits a receipt is set from `resends` after each attempt. T033: a vault with no Nostr-Relay at all still fails with `noRelay` at once, as before, but stays pending; a delivery that was sent already is never marked failed (a resend that no relay takes for 24 hours keeps showing _sent_). In T025 the relay that delivers in reverse order and twice is a small WebSocket proxy in front of an in-process relay; withholding refuses subscriptions to wraps, so a device that subscribed before keeps receiving live, and the test restarts the recipient after switching it on. The soak test (1,000 messages, devices coming and going) found three more things, fixed here: the subscription handed out stored wraps again, in the relay's order and in pieces beside the catch-up's ordered batch, so a returning device skipped more than the 40 message keys a chain keeps and could not decrypt older messages (already so since T022; the reset made it lose the sender's following messages too); the subscription now asks only for new wraps (`limit 0`), the run loop catches up before it takes live wraps, and a reconnect catches up once a relay reports connected (up to 5 s; otherwise the next catch-up does). A reset is decided after the batch, so the sender's other messages in it still decrypt on the sessions it drops. And `due_now` (vault opened, connections back) leaves a row that only awaits a receipt at its resend time.

**Checkpoint**: delivery survives relays and broken sessions

---

## Phase 5: User Story 2 - Compromised devices can recover future message security (Priority: P1)

**Goal**: a quiet session renews by itself; a copied key state stops working after both devices contributed new ratchet keys; removed devices lose their sessions; a new device and a copied vault file behave as the spec says.

**Independent Test**: quickstart scenarios 2, 3, 4 and 5, as the tests below.

### Tests for User Story 2

- [x] T035 [P] [US2] A copied key state (scenario 2) in `src-tauri/src/chat/peer/keep_alive_tests.rs`: copy all ratchet rows of a device that stays in use; with both vaults open and no user message, move the test clock; within 48 hours `renewed_at` of the session is later than the copy and the copy decrypts no event sent after that; before that point the test shows the copy still decrypts (so the test would notice a renewal claimed too early). Over 72 hours of test time with no user message, each device sends at most four proactive keep-alive requests to the other; a request gets one immediate response that does not trigger another response, and a closed vault sends none.
- [x] T036 [P] [US2] A removed device (scenario 3) and a new one (scenario 4) in `src-tauri/src/chat/peer/devices_tests.rs`: after the contact received the newer card, no wrap is addressed to the removed device, its copied state decrypts none of the later messages, and the sessions with it are gone; a newly linked device decrypts no archived wrap from before it was on the card and receives the messages sent after the contact got the newer card (SC-003, SC-008).
- [x] T037 [P] [US2] A copied vault file (scenario 5) in `src-tauri/src/chat/peer/devices_tests.rs`: the same database opened under a second installation id creates its own account and record, never reads the first one's rows (they are unchanged afterwards), and both installations send and receive (SC-007).

### Implementation for User Story 2

- [x] T038 [US2] Keep-alive in `src-tauri/src/chat/peer/delivery_send.rs` and `delivery_run.rs`: at the start of a vault session (after the first catch-up) and then every 15 minutes, for each peer device with a session that `targets::vault_of_device` still knows and whose vault still has a conversation: if `last_sent_at` of the session it would send on is older than `KEEP_ALIVE_MS` (or empty), encrypt the keep-alive plaintext, wrap it and publish it once, session and `last_sent_at` saved in the transaction that encrypts. A keep-alive never opens a session and never enters the outbox, and only this timer sends one: an arriving event never does. Makes T035 pass.
- [x] T039 [US2] Cleanup in `src-tauri/src/chat/peer/ratchet/session.rs` and `delivery_run.rs`: after every catch-up, delete the sessions, the peer state and the stored record of each peer device that `targets::vault_of_device` no longer knows or whose vault has no conversation left (the own vault's devices count as long as they are on the own device list). Makes T036 pass.

Done in pull request E. Beyond the tasks above: "still addressed" is one check, `targets::addressed_vault`, used by the keep-alive and the cleanup: the own vault for a device on the own device list, another vault only while a `chat_members` row names it (a direct conversation, a request, later a group). The cleanup runs inside `catch_up_and_keep_record` right after the catch-up, so every catch-up does it (start, timer, reconnect), not only the run loop's; it also forgets a stored record that no session uses. No keep-alive is made without a connected Nostr-Relay, so a session never moves for a keep-alive that goes nowhere; one that no relay accepts is not tried again. In T035 the attacker's copy follows every wrap to the device in order; with the 24-hour interval the renewal lands at 48 hours of test time, each device sends three keep-alives in 72 hours. In T036 the earlier history does reach the new device, through the own sync at linking (`ChatDevice::link` pulls); the test shows the later message came on its own session with the contact.

**Checkpoint**: all three P1 stories work together

---

## Phase 6: User Story 4 - Users understand the remaining privacy and delivery limits (Priority: P2)

**Goal**: the conversation view names the protection state in plain words and says what relays see and that the kept history is readable to whoever opens the vault.

**Independent Test**: quickstart scenario 10, as the tests below.

### Tests for User Story 4

- [x] T040 [P] [US4] `src-tauri/src/chat/peer/ratchet/protection_tests.rs`: each state of `data-model.md` from rows that produce it (`protected`, `renewal outstanding` at more than 7 days since `renewed_at` or `created_at`, `waiting for keys`, `recovering`), the weakest over all devices of the conversation wins, a conversation with the notes has none.
- [x] T041 [P] [US4] A Node check `scripts/check-chat-protection.ts`, added to `check:chat-state` in `package.json`: the pure mapping from state to label, icon and explanation key covers every state, and no text says that a relay's acceptance means delivery.

### Implementation for User Story 4

- [x] T042 [US4] `src-tauri/src/chat/peer/ratchet/protection.rs`: `of(q, me, thread_id) -> Protection { state, renewed_at }` over the devices of the conversation's targets. Makes T040 pass.
- [x] T043 [US4] Command `chat_conversation_protection` in `src-tauri/src/chat/peer/commands/conversations.rs`, registered in `src-tauri/src/lib.rs`, with the ts-rs type `ChatPeerProtection` (regenerate the binding, then the trailing-whitespace `sed`); test in the commands' test file.
- [x] T044 [P] [US4] `src/composables/usePeerConversation.ts` reads the protection when a conversation opens and on the events it already refreshes on; `src/components/chat/ProtectionState.vue` shows the state in the header of `src/components/chat/PeerConversation.vue` and, on demand, the explanation: the state in plain words (US4 scenario 1), what Nostr-Relays can see and do (FR-014), that the kept history is outside this protection (FR-024). If a matching building block is missing in haex-ui, add it there first.
- [x] T045 [P] [US4] Delivery states in `src/components/chat/PeerMessageList.vue`: a failed delivery that is still being tried says so (the reason `noRelay` with "is tried again"), and _sent_ keeps meaning that a relay accepted, _delivered_ that the recipient confirmed (US4 scenario 3).
- [x] T046 [P] [US4] Strings `chat.peer.protection.*` and the changed delivery texts in `src/i18n/locales/en.json` and `src/i18n/locales/de.json`; check the new group name against existing keys.

Done in pull request F. Decisions the data model left open: the order from strongest to weakest is `protected`, `renewalOutstanding`, `recovering`, `waitingForKeys` (nothing reaches a device that waits for keys, so that is the weakest); a device without a session that no event waits for has had nothing sent to it and does not weaken the state; the conversation's devices are those on the current cards of its other vaults and the own other devices, which get a copy of every message; `renewedAt` is the oldest last renewal over the devices with a session. `of` takes `now` from its caller, so the tests set it. The commands have no test file; the command only reads `of`, whose tests are in `ratchet/protection_tests.rs`. The protection is read with the messages, so it follows the same table changes; a renewal by keep-alives alone shows at the next change or when the conversation opens again. The explanation opens below the header instead of floating (`ProtectionExplanation.vue`), so no new building block was needed. The pure mapping of T041 is `src/lib/chat/protection.ts`; the check also compares its states with the exported binding. In T045 a message no relay accepted for a day keeps `failed` with `noRelay`, whose text now says it is tried again; the tooltips of _waiting_, _sent_ and _delivered_ say what each means.

**Checkpoint**: every state a user can reach has a text

---

## Phase 7: Polish and cross-cutting

- [ ] T047 [P] `CONTEXT.md`: entries for Device session, Chat key record, Keep-alive, Reset and Protection state, in the words of `spec.md`.
- [ ] T048 [P] A note "amended by spec 095" in `specs/072-unified-chat/contracts/nostr-chat.md` (what a rumor contains, the 20 KiB limit, no 24-hour end) and in `specs/072-unified-chat/tasks.md` at T078 (invitation parts are cut by size, about 9 members) and at T079 (group addressing goes through the outbox of this spec: one session per member device, nothing wrapped directly); no other change to those files.
- [ ] T049 Set [ADR 0026](../../docs/adr/0026-chat-ratchet-per-device-pair.md) to accepted once the operator agrees, and add the consequences that turned up while building, if any.
- [ ] T050 Gate checks, fail the task if one hits: no `GIVE_UP_MS` parking, no call of `wrap::wrap` outside `delivery_send.rs` (T023), no `log::` line or error text in `src-tauri/src/chat/peer/ratchet/` that formats a pickle, a key or a plaintext, no synchronized table named in the migration of T004.
- [ ] T051 Full verification of `src-tauri/` and the frontend through the bridge: `cargo fmt --check`, `cargo check`, `cargo clippy` (host and the `aarch64-linux-android` target), `cargo test` (lib with `-j 1`, then the integration tests), `pnpm lint`, `pnpm typecheck`, `pnpm typecheck:scripts`, every `pnpm check:*`, `pnpm format:check`, prettier on every touched Markdown and JSON file; report what ran and what was unavailable.
- [ ] T052 Run the e2e suite on the branch of each pull request that changes what the user sees or the wire (`gh workflow run ci.yml --ref <branch>`; e2e does not run on pull requests) and read the result.
- [ ] T053 Hand check by the operator with two real devices and a public Nostr-Relay: quickstart scenarios 6, 8 and 10.

---

## Dependencies and execution order

- **Phase 1** has none. **Phase 2** depends on Phase 1 and blocks everything below.
- Inside Phase 2: T004 first; T005 and T006 are parallel to it; T007 needs T004; T008 needs T007; T009 needs T007 and T008; T010 needs T004. T011 needs T006 and T009; T012 needs T011 and T003; T013 needs T012 and T008.
- **US1** needs Phase 2. T018 and T019 first (different files), then T020 and T021 (they need both), then T022; T023 and T024 are independent of them.
- **US3** needs US1. T029 before T030; T031, T032, T033 and T034 touch `outbox.rs`, `delivery_send.rs` and `delivery_receive.rs` in turn, so they follow one another in this order.
- **US2** needs US1 and, for scenario 2's "the copy decrypts nothing afterwards" under relay faults, benefits from US3 but does not depend on it. T038 before T039.
- **US4** needs US3 (`recovering`) and US2 (`renewed_at` moving by itself); T042 before T043 before T044.
- **Polish** needs the stories that are included.

## Parallel examples

```text
Phase 2:      T005 wire        T006 Me/targets     T010 erasure (after T004)
US1 tests:    T014 two vaults  T015 forward secrecy  T016 receiving  T017 crash safety
US3 tests:    T025 relays      T026 reset          T027 receipts   T028 outbox
US2 tests:    T035 keep-alive  T036 devices        T037 copied vault file
US4:          T040 states      T041 Node check     T044 view   T045 delivery texts   T046 strings
```

## Implementation strategy

1. **Pull requests** follow the plan's slices: A = Phase 1 and T004 to T010; B = T011 to T013; C = US1; D = US3; E = US2; F = US4; G = Polish. A and B change nothing on the wire and can be merged one by one. The repository rules require rebase-merge or a merge commit, never squash.
2. **C is the cut-over**: from then on devices neither send nor accept the earlier format (FR-016, no migration). Merge C only together with the decision to update every development vault; D and E should follow soon, because until D a broken session does not heal and until E a quiet session does not renew.
3. After each slice run the checks of T051 that apply to it, and let the operator review before the next slice is stacked (the operator often pushes review fixes to open branches; fetch before pushing and never force-push).
4. **Stop and validate** after C with quickstart scenarios 1, 8 and 9, after D with 6 and 7, after E with 2 to 5, after F with 10.
