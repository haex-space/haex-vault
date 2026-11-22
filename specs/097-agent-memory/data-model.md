# Data model: Shared memory of the agents

## Table `haex_agent_memory_pages` (new, synced, vault-wide)

Migration `0069_agent_memory`. Tracked by haex-crdt (no `_no_sync` suffix), no UNIQUE constraint besides the primary key,
no foreign key.

| Column           | Type             | Meaning                                                               |
| ---------------- | ---------------- | --------------------------------------------------------------------- |
| `id`             | TEXT, primary    | UUID v4 of the page                                                   |
| `title`          | TEXT NOT NULL    | one line, at most 80 characters                                       |
| `description`    | TEXT NOT NULL    | one line, at most 200 characters; what the table of contents shows    |
| `body`           | TEXT NOT NULL    | Markdown, at most 10,000 characters; pointers are `[[Title]]`         |
| `author_kind`    | TEXT NOT NULL    | `user` or `model` (a model of the local agent or of an Abo-Agent)     |
| `author_key`     | TEXT             | the subject key of spec 083 (`model:<id>`); `NULL` for the user       |
| `author_name`    | TEXT NOT NULL    | the name the user knew the model by when it wrote; empty for the user |
| `author_device`  | TEXT NOT NULL    | the vault device UUID of the device that wrote                        |
| `created_at`     | INTEGER NOT NULL | milliseconds since the epoch                                          |
| `changed_kind`   | TEXT             | as `author_kind`, for the last change; `NULL` while unchanged         |
| `changed_key`    | TEXT             | as `author_key`                                                       |
| `changed_name`   | TEXT             | as `author_name`                                                      |
| `changed_device` | TEXT             | as `author_device`                                                    |
| `changed_at`     | INTEGER          | time of the last change                                               |

Rules:

- **Title.** Compared without case and surrounding spaces. A write that would give a second page the title of an existing
  one on this device is refused (`title_taken`); a sync can still bring two, and then a title resolves to the one changed
  last and the other is named in replies and on the page.
- **Name kept, not looked up.** `author_name` and `changed_name` are stored, so a page keeps the name of a model that was
  deleted. The device is shown by its current alias in `known_devices`; a device that is gone shows as unknown.
- **Order.** Changed last first by `max(created_at, changed_at)`, then by `id`.
- **Two devices at once.** Changes of different pages never meet; two changes of the same page merge per column by the
  later write, as every tracked table does.
- **Removal.** The row is deleted with the usual deletion marker. Pointers to its title stay in the texts that hold them.
- **Unknown values.** An `author_kind` this build does not know is shown with its stored name and no kind label.

### Validation on every write (FR-003, FR-022)

1. `agent_memory::check::check(field, text)` for title, description and body: `empty` (title, description), `too_long`,
   `line_break` (title, description), `invisible_characters`, `secret`, `instruction`.
2. `title_taken`: a new page, or a renamed one, whose title another page on this device has.
3. `memory_full`: a new page beyond 1,000.
4. `page_too_long`: `memory.add` whose result would pass 10,000 characters.

A change that keeps or reduces a size and every removal are never refused for size.

## Pointers (derived, not stored)

`agent_memory::links::pointers(body) -> Vec<String>`: every `[[…]]` in the body, trimmed, in order, without duplicates;
`[[Title|shown text]]` points to `Title`. The graph is a map `title → (pointers out, pointers in)` built from all pages and
rebuilt when `haex_agent_memory_pages` changes. A pointer whose title has no page is kept and shown as such.

## Columns of `chat_threads` (new)

Added by the same migration; both synced with the thread and deleted with it, neither part of the thread list
(`storage/chat_threads.rs` names its columns) nor of the `ChatThread` binding.

| Column              | Type    | Meaning                                                                                                   |
| ------------------- | ------- | --------------------------------------------------------------------------------------------------------- |
| `memory_toc`        | TEXT    | the table of contents as it was when the first turn started; `NULL` until taken, `''` for an empty memory |
| `memory_foreign_at` | INTEGER | when foreign content was first seen in the conversation; `NULL` while none                                |

Both are written once and never changed afterwards. `memory_toc` is one line per page in the form of the contract's "The
memory block".

## Rights: the group `memory`

`Rights.memory: MemoryLevel` in `src-tauri/src/mcp_server/grants.rs`:

```rust
pub enum MemoryLevel { Off, Read, ReadWrite }   // Default: Off
```

Stored as one row of `haex_agent_profile_grants`: `grant_group = 'memory'`, `grant_key = ''`, `level` one of `off`,
`read`, `read_write`.

| Stored                                    | Profile                        | Resolved                                 |
| ----------------------------------------- | ------------------------------ | ---------------------------------------- |
| no rows at all                            | "Standard"                     | `ReadWrite` (factory rights)             |
| rows, but none of the group `memory`      | "Standard"                     | `ReadWrite` (stored before this feature) |
| rows, but none of the group `memory`      | any other                      | `Off`                                    |
| row with `off`, `read`, `read_write`      | any                            | that level                               |
| row with a level this build does not know | any                            | `Off`                                    |
| anything                                  | resolved for an external agent | `Off` (FR-020)                           |

Scopes: `memory.read` is open at `Read` and `ReadWrite`, `memory.write` at `ReadWrite`.

## Constants (`agent_memory/limits.rs`, `toc.rs`)

| Name                    | Value  | Source                                                     |
| ----------------------- | ------ | ---------------------------------------------------------- |
| `TITLE_MAX_CHARS`       | 80     | research R2                                                |
| `DESCRIPTION_MAX_CHARS` | 200    | research R2                                                |
| `BODY_MAX_CHARS`        | 10,000 | research R2                                                |
| `MAX_PAGES`             | 1,000  | research R2                                                |
| `SHARE`                 | 10     | the block may fill a tenth of the model's context (FR-010) |
| `SEARCH_RESULTS`        | 10     | research R4                                                |
| `NO_START_TOC`          | empty  | models that get no block at the start (FR-027)             |
