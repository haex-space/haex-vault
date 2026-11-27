# Data Model: Delegates reach the web only through HaexVault's agent browser

Nothing is stored. The one entity is a constant per vendor in `src-tauri/src/adapters/cli_delegate/web_block.rs`.

## Web block of a vendor

| Field              | Meaning                                                                                                                         |
| ------------------ | ------------------------------------------------------------------------------------------------------------------------------- |
| `vendor`           | `DelegateVendor` (Claude, Codex, Abacus).                                                                                       |
| `args`             | The arguments every start gets, in every autonomy mode ([contracts/web-block.md](contracts/web-block.md)).                      |
| `confirm`          | What the program's start-up report must show: absent tool names (Claude), off features and `web_search` (Codex), none (Abacus). |
| `checked_versions` | The program versions the block was checked against ([quickstart.md](quickstart.md) §4).                                         |

**Rules**

- `args` are the same for every autonomy mode and every program version (spec FR-003, FR-006).
- Every version a vendor lists in the shipped `catalog.json` is in `checked_versions` (a test fails otherwise).
- A start whose report shows a blocked tool, a connected `claude-in-chrome` server, a `web_search` other than `disabled`
  or a blocked feature enabled ends with `AdapterError::Unavailable` and a reason naming the vendor and what was found;
  no other backend is tried (spec FR-007).

**State**: none. The block is computed per start; a change of the person's settings never touches it.
