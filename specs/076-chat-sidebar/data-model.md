# Data model: Chat sidebar as a rail, search, and one composer for contacts

No table, column or migration is added. Everything below is read-only or window state.

## Window state (A)

| Name                | Where                                     | Lives                          | On remount                                             |
| ------------------- | ----------------------------------------- | ------------------------------ | ------------------------------------------------------ |
| `collapsed`         | Pinia store `src/stores/chatSidebar.ts`   | until the window closes        | survives (FR-008)                                      |
| narrow overlay open | `useSidebarFrame().menuOpen`              | until pick, Escape or toggle   | resets (a fresh chat opens without a covering overlay) |
| search text         | `useChatSearch()` (component-local `ref`) | until cleared or chat remounts | resets                                                 |

## ChatSearchHit (B2, ts-rs, `src/types/bindings/ChatSearchHit.ts`)

| Field       | Type                   | Notes                                                            |
| ----------- | ---------------------- | ---------------------------------------------------------------- |
| `threadId`  | string (UUID)          | the conversation, or for `agent` the session                     |
| `kind`      | `ChatConversationKind` | `agent`, `direct`, `group`, `note`; named with the `Chat` prefix |
| `messageId` | string (UUID)          | for a later "jump to"; unused by the window in this spec         |
| `at`        | number (ms)            | `sort_at` for peer messages, `created_at` for agent messages     |
| `snippet`   | string                 | text around the first match, ellipsised, ≤ ~130 characters       |

## ChatSearchResult (B2)

| Field  | Type              | Notes                                   |
| ------ | ----------------- | --------------------------------------- |
| `hits` | `ChatSearchHit[]` | newest first, at most 50                |
| `more` | boolean           | true when a 51st match existed (FR-011) |

The names are domain-prefixed (`ChatSearch…`) so they cannot overwrite another binding.

## Validation rules

- The text is trimmed; fewer than 2 characters (counted as Unicode scalar values) returns an empty
  result without a query (FR-011).
- The text is matched literally: `\`, `%` and `_` are escaped (FR-013).
- Requests (`chat_threads.state = 'request'`) and non-conversation rows (tool calls, tool results,
  system messages) never match (FR-013, R3).

## State transitions

None. The sidebar's `expanded ⇄ rail` flag and the narrow overlay are independent: in a wide window the
flag decides; in a narrow window the rail is fixed and only the overlay opens.
