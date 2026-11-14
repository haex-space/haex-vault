# Contract: the search command and the window's own contracts

Extends [spec 072's contract](../../072-unified-chat/contracts/ipc.md); nothing there changes.

## Command (Tauri)

| Command       | Arguments          | Result             | Notes                                                                                                                                                           |
| ------------- | ------------------ | ------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `chat_search` | `{ text: string }` | `ChatSearchResult` | read-only; `text` is matched literally and case-insensitively with Unicode lowercase matching (R3); < 2 characters after trimming → `{ hits: [], more: false }` |

Errors: the vault is locked → the usual `HolziError` for an inactive database. No new error codes.

**Not an action and not an MCP tool.** `chat_search` is registered with the window's commands only. It
has no entry in `src/lib/actions/chatActions.ts`, is not in the data scopes of spec 021, and no agent can
call it (R5, 072 FR-071 / SC-010).

## Events

None new. The result is not live: a changed vault does not re-run an open search; the next keystroke or
clearing does. (A search result that arrives after the text changed is dropped by the window.)

## Window contracts

| Test id                             | Where               | Stays / new                          |
| ----------------------------------- | ------------------- | ------------------------------------ |
| `chat-sidebar-toggle`               | the window's header | stays; now a header control          |
| `chat-new-conversation`             | the window's header | stays; now a header control          |
| `chat-sidebar-search`               | expanded sidebar    | new                                  |
| `chat-search-hit`                   | each message hit    | new                                  |
| `chat-input`, `chat-send`           | agent composer      | stay                                 |
| `chat-peer-input`, `chat-peer-send` | contact composer    | stay                                 |
| `#chat-sidebar[data-state]`         | the panel           | new: `hidden`, `expanded`, `overlay` |

Catalog actions: `chat.conversation.new` (072) is what the new-conversation icon runs; the sidebar
button and the search field are view state and draft editing, `action-exempt:` like the
existing toolbar button.
