# Data Model: Tag management by selection, with merge

No table, column or migration changes. The feature reads and writes the two existing synced tables.

## Tag (`haex_passwords_tags`)

`id`, `name` (unique by `fold`; after trimming at least 1 and at most 64 characters), `color`
(optional), `created_at`.

- **Merge target** keeps `name` and `color`.
- **Merge source** row is deleted (with a delete marker).

## Entry-tag link (`haex_passwords_item_tags`)

`id` (derived from item and tag: `item_tag_id(item, tag)`), `item_id`, `tag_id`. An entry has at most
one link per tag.

## Merge (operation, not stored)

Input: `sourceId`, `targetId`. In one write transaction:

1. `sourceId != targetId`, both rows exist.
2. For each link of the source: delete it (own marker); insert `item_tag_id(item, target)` unless the
   entry has a link to the target already.
3. Delete the source row (own marker).

State after: the target's links are the union of both tags' entries, each entry once; the source and
all its links are gone. A refusal at step 1 changes nothing.

## Agent-profile reference (read only)

`haex_agent_profile_grants` rows of group `passwords`: `scope_tags` (JSON array of names, `GrantRow.tags`
in code) and `grant_key` (the tag a new entry gets). A profile "names" a tag when one of these equals the tag's name by `fold`.

## Tag usage (derived, not stored)

`{ login: number, pass: number }` per tag: how many entries of each app carry it, by the cut of
`scopeOf` (`pass` is Wallet's, everything else the password manager's). Computed in the frontend from
the headers the store holds; for a display group of equal names it counts the entries that carry any of
its ids. `login + pass` is the number of entries that carry the tag.
