# Contract: commands

Amends spec 072 [contracts/ipc.md](../../072-unified-chat/contracts/ipc.md) and the contacts commands of
spec 051.

| Command                     | Args                              | Returns            | Notes                                                                                                                                                                           |
| --------------------------- | --------------------------------- | ------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `chat_request_answer`       | `{ threadId, answer: "decline" }` | `()`               | Changed: `block` is gone. `decline` removes the request, blocks the vault with the carried card and writes a blocked notice (FR-009). Accepting stays `chat_request_card` + 060 |
| `chat_requests_set_allowed` | `{ allowed: boolean }`            | `()`               | New (FR-012): writes `chat.requests.allowStrangers`; `false` also deletes every request, without notice and without block                                                       |
| `contacts_block_contact`    | `{ contactId }`                   | `()`               | New (FR-014): every vault of the contact on the block list, a blocked notice per vault; the contact is hidden                                                                   |
| `contacts_block`            | `{ vault }`                       | `()`               | Changed (spec 051): also writes a blocked notice to the vault                                                                                                                   |
| `contacts_blocked`          | `()`                              | `BlockedVault[]`   | Changed: each entry also names the hidden contact (`contactId`, `name`) or the kept card's name, and the direct `threadId` for "Verlauf ansehen" (FR-015, FR-016)               |
| `contacts_unblock`          | `{ vault }`                       | `()`               | Changed (today `set_blocked(false)`): restores the hidden contact or creates one from the kept card, writes an accepted notice (FR-016)                                         |
| `chat_conversations_list`   | unchanged                         | `ChatConversation` | Leaves out conversations of blocked vaults; new fields `peerAnswered`, `peerBlocked`, `blockedByPeer`, `requestLastAt` (data-model.md)                                          |

Actions (catalog, not agent-callable): `chat.request.decline`, `chat.requests.setAllowed`,
`contacts.blockContact`, `contacts.unblock`. `chat.request.block` goes.

Contacts readers (`contacts_list`, `contacts_get`, `contacts_photo`, agent `contacts.search`/`contacts.get`,
MCP, extension contacts API) leave hidden contacts out; `get` of a hidden contact answers `NotFound`
(research R11).
