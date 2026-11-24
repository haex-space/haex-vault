# Implementation Plan: Only contacts may write in the chat

**Branch**: `feat/099-chat-contact-gate` | **Date**: 2026-11-25 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/099-chat-contact-gate/spec.md`

## Summary

The receiving vault stores a chat message only from a stored, unblocked contact. A message of a stranger
leaves a request (thread + carried card, no message rows) or, with requests switched off, nothing. The
two answers and blocking write a device-local notice that the chat session sends as a receipt with an
`answer` field; the sender's devices resend at once on `accepted` and park on `blocked`. Blocking a
contact puts its vaults on spec 051's block list, which now also keeps a declined stranger's card;
contacts with a blocked vault are hidden by one flag computed in `store::headers` and one filter in
`ContactsService`. Unblocking restores the contact (or creates it from the kept card) and sends
`accepted`.

Decisions and reasons: [research.md](research.md). Wire: [contracts/nostr-chat-gate.md](contracts/nostr-chat-gate.md).
Commands: [contracts/ipc.md](contracts/ipc.md). Tables: [data-model.md](data-model.md). Checks:
[quickstart.md](quickstart.md).

## Technical Context

**Language/Version**: Rust 1.95, edition 2021 (`src-tauri`); TypeScript / Vue 3 / Nuxt (frontend).

**Primary Dependencies**: none new. Reused: `haex-crdt`, `nostr` 0.45, the chat outbox and ratchet of spec
095, spec 060's card import (`share/merge.rs`).

**Storage**: three columns on `chat_threads`, one on `haex_contacts_blocked_vaults` (synchronized), one new
device-local table `chat_gate_notices_no_sync`, one vault preference ([data-model.md](data-model.md)).

**Testing**: Rust tests in `*_tests.rs` beside each module; two-vault tests on the in-process Nostr-Relay
(`two_vaults_tests.rs`, `requests_tests.rs`); one new e2e scenario; `check:chat-state` for the window
logic.

**Target Platform**: the existing desktop and mobile targets, including Android.

**Project Type**: Tauri desktop/mobile application with Rust backend and Vue frontend.

**Performance Goals**: a knock costs one decryption and one small transaction; with requests off a
stranger's event costs one decryption and nothing is written.

**Constraints**: nothing of a stranger's message reaches the database (FR-002); the gate decision, the
request and the notice row are written in the transaction that receives or answers; no notice goes out
while requests are off.

**Scale/Scope**: chat receive/send/outbox/targets/notify, chat commands and settings, contacts store and
service, contacts commands; frontend: requests view, conversation view (hint, blocked states, "Sperren"),
chat settings (switch), contacts page ("Sperren"), blocked list (names, "Verlauf ansehen", unblock).

## Constitution Check

_GATE: Must pass before Phase 0 research. Re-check after Phase 1 design._

- **I. No secrets in Git**: Pass. No key material; tests generate keys.
- **II. No local absolute paths**: Pass.
- **III. Device-independent project identity**: Not affected.
- **IV. Cross-repo references pin immutable revisions**: Not affected (no external references).
- **V. External sources opt-in**: Not affected.
- **VI. Self-modifying instructions**: Not affected.
- **VII. Relay unavailability never blocks local work**: Pass. Answering, blocking and unblocking are local
  writes; notices wait in the outbox.
- **VIII. No concealment instructions**: Not affected.
- **ADR**: not needed; no earlier ADR decision changes (ADR 0024/0026 wire and session rules stay; a receipt
  gains an optional field).
- **New dependency**: none.
- **Tests in separate files**: Pass by plan.
- **spaex file size (≤ 500 lines or documented)**: `outbox.rs` is over 500 lines with a documented split
  "at the next outbox state-machine change" (its header). R6 adds parking and resend by vault: the split
  into `outbox/state.rs` happens in the same PR as R6.

Re-check after design: no change.

## Project Structure

### Documentation (this feature)

```text
specs/099-chat-contact-gate/
├── spec.md
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── nostr-chat-gate.md
│   └── ipc.md
├── checklists/requirements.md
└── tasks.md            # /speckit-tasks
```

### Source Code (repository root)

```text
src-tauri/src/
├── identity/migrations*.rs           # new migration: chat_threads columns, blocked_vaults.card, gate notices table
├── chat/peer/
│   ├── receive.rs                    # gate decision, request without messages (R1, R2)
│   ├── delivery_receive.rs           # knock: confirm session, no ack, notify once; off: dropped (R3, R9, R10)
│   ├── store.rs                      # request columns, list filter + new fields (R2, R7, R15)
│   ├── requests.rs                   # decline = block with card + notice; switch off (R8, R14)
│   ├── gate.rs (new)                 # notice rows → receipts; applying received notices (R5, R6)
│   ├── send.rs / targets.rs          # blocked-notice exception, no receipts to requests, park while blocked
│   ├── outbox.rs → outbox/state.rs   # park/resend by vault (R6) with the documented split
│   ├── notify.rs                     # request notification once
│   ├── settings.rs                   # chat.requests.allowStrangers
│   └── commands/{requests,conversations}.rs
├── contacts/
│   ├── store.rs                      # ContactHeader.blocked (R11)
│   ├── card/received.rs              # block with card, unblock, blocked list with names (R12)
│   ├── service/{contacts,cards}.rs   # hide filter, block contact, unblock restores (R11, R13, R14)
│   ├── share/preview.rs              # hint candidates skip hidden
│   └── commands/cards.rs             # contacts_block_contact, changed block/unblock/blocked
src/
├── components/chat/                  # RequestBanner (two answers), PeerConversation (hint, blocked states, Sperren)
├── components/settings/chat/         # switch "Anfragen von Fremden zulassen"
├── components/contacts/              # "Sperren" on the contact page, blocked list with "Verlauf ansehen"
└── i18n/locales/{de,en}.json
scripts/e2e/scenarios/chat-contact-gate.test.ts
```

**Structure Decision**: the existing Tauri layout; one new chat module (`gate.rs`) for the notices, the
rest extends the modules that own the rows.

## Open point for the operator

- **Phone address book (spec 051)**: a blocked contact stays in the phone's own address book (research
  R11). Hiding it there would delete the phone's copy through the phone sync, and unblocking could not
  bring back what was on the phone. Confirm, or decide that blocking also removes it from the phone.

## Complexity Tracking

No constitution violation to justify.
