# ADR 0025: Entry types share one identity table and add a detail table

- **Status**: accepted
- **Date**: 2026-11-12
- **Spec**: [092-wallet-passes](../../specs/092-wallet-passes/spec.md), research R1 and R12

## Context

The password manager (spec 034) stored one kind of entry, the login. Spec 092 adds cards and tickets
(passes) and names more types as follow-ups (payment card, identity, note, SSH key). Every satellite
of an entry (free fields, folder and trash, tags, attachments, history, grants) is keyed by the item
id, and the sync, the access rules (spec 021) and the window all reach entries through it.

## Decision

1. An entry type is a `kind` on `haex_passwords_item_details` (`login` for every entry that existed
   before) plus a 1:1 detail table keyed by the item id (`haex_passwords_pass_details` for a pass).
   Data that belongs to a type and has many rows (the codes of a pass) sits in a table of its own.
2. Every satellite keyed by the item id serves every type unchanged. A new type costs a detail table,
   a view and an import mapping, not a copy of six satellites.
3. The type is fixed at creation. Every reader that serves a credential, an autofill, a secret or a
   passkey asks for logins and is refused a pass; a reader that wants another type says so.
4. A new type ships with a migration (a `kind` value alone is not enough). The sync handshake of spec
   024 (FR-029) refuses a peer whose migration count or trigger version differs, and an old build
   cannot open a vault with a migration it does not know, so no old device ever receives an entry it
   cannot show. The window's "needs a newer holzi" notice for an unknown `kind` is a second layer, not
   the guard.
5. The tables that hand-list the satellites (`trash::purge_item`, `copy.rs`) name every new table, since
   the sync applies a remote delete without foreign keys.

## Consequences

- Folders, tags, trash, history, attachments, grants and sync work for a pass without new code.
- The login-specific columns stay on the shared table and are empty for a pass; moving them into a
  `login_details` table is a separate cleanup.
- A second table with copies of the six satellites was rejected: it would double every satellite
  query and every rule of spec 034.
