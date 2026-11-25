# Data Model: Tag management by selection, with merge

No table, column or migration changes. The feature reads and writes the two existing synced tables.

## Tag (`haex_passwords_tags`)

`id`, `name` (unique by `fold`; after trimming at least 1 and at most 64 characters), `color`
(optional), `created_at`.

- **Merge target** keeps `color`. Its name is kept when no result name is supplied; otherwise the
  supplied name is trimmed and validated (non-empty, at most 64 characters, and not already used by
  a tag that remains). The name is changed in the same transaction as the merge. A source name is
  available because the source is deleted.
- **Merge source** row is deleted (with a delete marker).

## Entry-tag link (`haex_passwords_item_tags`)

`id` (derived from item and tag: `item_tag_id(item, tag)`), `item_id`, `tag_id`. An entry has at most
one link per tag.

## Merge (operation, not stored)

Input: `sourceIds`, `targetIds`, and an optional result name. Each ID list is the complete folded-name
display group; the first target ID is the retained target row. Validate both groups and any supplied
name before the first write. In one write transaction:

1. The source and target groups are disjoint and every row exists.
2. For every source ID and each of its links: delete it (own marker); insert
   `item_tag_id(item, retained target)` unless the entry has a link to the target already.
3. Delete every source row (own marker).
4. If a result name was supplied, rename every target-group row to it.

State after: the target group's links are the union of both groups' entries, each entry once; the
source group and all its links are gone. A refusal during validation changes nothing.

## Agent-profile reference (read only)

`haex_agent_profile_grants` rows of group `passwords`: `scope_tags` (JSON array of names, `GrantRow.tags`
in code) and `grant_key` (the tag a new entry gets). A profile "names" a tag when one of these equals the tag's name by `fold`.

## Tag usage (derived, not stored)

`{ login: number, pass: number }` per tag: how many entries of each app carry it, by the cut of
`scopeOf` (`pass` is Wallet's, everything else the password manager's). Computed in the frontend from
the headers the store holds; for a display group of equal names it counts the entries that carry any of
its ids. `login + pass` is the number of entries that carry the tag.
