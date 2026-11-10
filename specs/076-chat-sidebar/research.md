# Research: Chat sidebar as a rail, search, and one composer for contacts

## R1 — Rail without changing `useSidebarFrame`

**Decision**: keep `useSidebarFrame` as it is. `SidebarLayout` passes a tiny, invisible probe element
(`absolute @2xl:static`, `aria-hidden`) as the `panel`, so the composable keeps deciding "wide" from the
CSS threshold and nobody copies `@2xl`. The visible panel is always in the flow with a rail width
(`w-14`) and widens to `w-64` when expanded. In a wide window `wideHidden` means "collapsed to the rail"
(`!visible` = rail). In a narrow window the rail stays in the flow and `menuOpen` lays the full sidebar
over the content (`absolute inset-y-0 left-0 w-64 z-20`), the content keeping the rail's width beside it.

**Rationale**: Settings, Passwords, Contacts and Files share the composable and expect "hidden entirely";
changing its contract would touch four unrelated apps (surgical change). `visible`, `toggle`, `close`,
`overlaying` already carry the semantics the chat needs.

**Alternatives**: (a) a `rail: true` option on the composable — rejected: dead branches in four callers'
code path and tests; (b) measuring the width in the layout with a second constant — rejected: the
composable's own comment says not to copy the threshold.

## R2 — Where the collapsed flag lives (FR-008)

**Decision**: a small Pinia store `src/stores/chatSidebar.ts` with one `collapsed` ref, like the other
view-state stores in `src/stores/` (the app is `ssr: false`; no code uses Nuxt `useState`). It survives
remounts of the chat tab within the window and is not persisted, not synced.

**Rationale**: the spec says "in the same window"; a preference row would sync to other devices (a
vault-scoped pref is visible everywhere) or need a device scope for a cosmetic flag. `localStorage` is
not used for such flags in this codebase (only the extension host touches it).

**Remount check** (Pinia/remount lesson): the flag survives a remount on purpose; the narrow overlay's
`menuOpen` and the search text reset on remount on purpose (a fresh chat opens without a covering
overlay and without a stale search).

## R3 — Search is a Unicode case-insensitive scan, no index

**Decision**: `chat_search` runs two parameterized, read-only queries joined to `chat_threads`, ordered
newest first. Rust filters each row using Unicode lowercase matching, so FR-011's case-insensitive
contract applies to non-ASCII text too. It stops after finding 51 hits, then returns the newest 50 and
sets `more`; a linear scan may inspect all rows when matches are sparse:

- peer: `chat_peer_messages.text` of threads with `state <> 'request'`, `text IS NOT NULL`;
- agent: `chat_messages.content` where `role IN ('user','assistant')` — tool calls, tool results and
  system rows are not conversation text.

**Rationale**: SQLite `LIKE` folds ASCII case only, which would violate FR-011 for non-ASCII letters.
Filtering with Rust's Unicode lowercase conversion keeps the matching behavior explicit and does not
require a database extension or schema/index change. The 50,000-message target must be measured in B2;
if the scan misses SC-003, revisit an indexed normalized column. No FTS5 for now: the tables are
CRDT-tracked and a virtual table would need to join the sync story (`SYNCED_TABLES` tests).

**Alternatives**: SQLite `LIKE` — rejected because its default case folding is ASCII-only; FTS5 —
rejected for now (above); filtering in the window over loaded messages — rejected because it would only
see the open page of messages.

## R4 — Extract and hit shape

**Decision**: the command returns for each hit `{ threadId, kind, messageId, at, snippet }` where the
snippet is cut in Rust around the first match (about 40 characters before and 80 after, on char
boundaries, ellipsis marks). The window resolves the conversation's name and picture from data it
already has (`PeerConversationEntry`, `Thread`), so the command carries no names.

**Rationale**: keeps the command small and the IPC payload bounded (≤ 50 × ~130 characters). For agent
hits `threadId` is the session id and opens through `ui.openConversation`.

## R5 — Agents must not reach the search

**Decision**: `chat_search` is a window-only Tauri command, registered in `lib.rs`, with **no action in
the catalog** (`chatActions.ts`) and no MCP route. Spec 072 FR-071 / SC-010 say an agent reads contact
conversations only with its own permission; a search over all conversations would bypass that.

**Rationale / test**: a Tauri command is not an agent tool unless an action exposes it (guardrail-scoped
actions are never agent-callable, `chatActions.ts`). The plan adds no action; a task adds an assertion in
`check:agent-actions` that no catalog entry or tool name mentions `chat_search`.

## R6 — Reuse candidates (graph consulted, stale)

| Need                          | Existing                                                                                             | Decision                                                  |
| ----------------------------- | ---------------------------------------------------------------------------------------------------- | --------------------------------------------------------- |
| Search input with clear, icon | `UiInput` with `prepend-icon`, `clearable` (contacts `Toolbar.vue`), `watchDebounced` (files search) | reuse, no new field component                             |
| Avatar                        | `ContactsAvatar`; the agent's round icon in `AgentListItem.vue`                                      | reuse; the rail item wraps them                           |
| Unread badge                  | `ShadcnBadge` in `PeerListItem.vue`                                                                  | reuse                                                     |
| Composer box                  | `Composer.vue` (agent)                                                                               | extract `ComposerShell` (R7), not a parallel copy         |
| Dictation                     | `ChatVoiceInputControl`                                                                              | reuse unchanged                                           |
| Time label                    | `listTime`                                                                                           | reuse                                                     |
| Rust text search              | `files::search` (file system, fuzzy)                                                                 | not reusable: different domain; the SQL here is ~40 lines |

## R7 — Composer shell

**Decision**: extract `ComposerShell.vue` from `Composer.vue`: the rounded box, the auto-growing
textarea (`useAutoResizeTextarea`), a `controls` slot, `ChatVoiceInputControl`, the send button, the
`voiceRecording` model. `Composer.vue` becomes the agent wrapper that fills the slot with attachments,
agent activity, model/effort, permission and choice prompts and keeps its test ids (`chat-input`,
`chat-send`) and its cancel-while-streaming button (also a slot). `PeerConversation.vue` uses the shell
with an empty slot and keeps `chat-peer-input` / `chat-peer-send` through shell props, so no E2E handle
changes.

**Rationale**: the agent composer's twelve props are about the agent; making them optional would put
`v-if`s on every control. The shell carries only what both share.

**Dictation behavior**: `transcript(text, autoSend)` → `autoSend` runs the peer `submit()` with that
text; otherwise the text is appended to the draft. The peer send keeps its idempotency key rule (one
key per message; a dictation that auto-sends makes a new draft and key).

**Attachments** in the peer composer are not part of this slice (072's attachment send comes with the
file transfer, PR E); the slot is where they go later.

## R8 — E2E handles

`chat-sidebar-toggle` stays the "chat is ready" signal in `scripts/e2e/lib/flows.ts` and
`tab-content-isolation`: it must be displayed in the rail and the expanded sidebar alike. `#chat-sidebar`
stays the panel's id, but "shown" no longer means "visible"; the panel gets `data-state="rail|expanded|overlay"`
and `chat-narrow-window` asserts that. `chat-new-conversation` stays the test id of the new-conversation
button in both states.
