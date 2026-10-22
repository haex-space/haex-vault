# References between entries are resolved in the service; the passkey service signs without a counter or a presence claim

Status: accepted; superseded in part by ADR-0015 (user presence and verification)
Date: 2026-10-04

## Decision

Two additions to the password manager (spec 036) extend ADR-0007 ("secrets sit in
the vault database and are protected by grants"). Neither weakens it.

**References between entries.** A value of an entry (username, password,
address, note, the value of a custom field) may contain placeholders in the form
`{$<entry id>:username}`, `{$<entry id>:password}` or `{$<entry id>:extra:<key>}`
(the model is KeePass's field references).

- Placeholders are **text**. Only the Rust service resolves them, when a value is
  shown, copied or used. The resolved value is never stored: not in the entry,
  not in the history (a state keeps the placeholder), not in a list.
- Lists never resolve. For a caller from outside the window, a username or an
  address that contains a placeholder is returned empty (otherwise a password
  reference in a username would carry a secret into a list).
- **A reference grants no access.** A caller from outside the window gets a value
  through a reference only if its grant also covers the **source** entry; otherwise
  the whole field is absent and nothing says that a reference exists.
- Resolution follows chains up to **12 levels**, as KeePass does (its
  `SprEngine.MaxRecursionDepth`). Unlike KeePass it never returns empty text for
  a cycle or a chain that is too deep: the field reports the error, and copying or
  using it fails instead of silently using an empty or literal value. A cycle is
  refused on save.
- The grammar exists once, in Rust. The window asks the service to split a text
  into its parts and to build a placeholder (with the escaping); it has no copy of
  the grammar.

**Passkey service.** The service can create a passkey, confirm a sign-in (sign)
and list passkeys for a caller with grants (extensions, external agents over MCP,
later the external bridge); the window creates and uses none (it lists, renames
and deletes).

- A passkey always belongs to an entry and follows that entry's tags and trash
  state. A passkey without an entry (only possible through foreign
  data) counts as not there and is never created by holzi.
- The private key never leaves the service: no answer, error, log or history
  carries it.
- The origin of a request is checked against the relying party id (the host is
  the id or a subdomain of it, and the id is not a public suffix, not an IP
  address) before anything is signed.
- The signature counter stays **0**. A synced passkey cannot have a reliable
  global counter across devices that confirm offline; sending equal non-zero
  values would look like a clone to a relying party.
- The authenticator data claims neither user presence nor user verification
  (nobody confirms the request in the service). Relying parties that require them
  refuse the answer until a trustworthy, ceremony-bound confirmation exists.
- A passkey can be shown in another entry by a link row (`passkey_links`), never by
  copying the key pair.

## Consequences

- The reference resolver is a second place, next to `reveal.rs`, where secrets
  leave storage; it is a pure module plus a database module tested with planted
  marker values and the access matrix (a caller with a tag grant on the target
  only, trashed sources, lists).
- A relying party that insists on user presence cannot be served by this service
  yet; that is a limit, not an oversight.
- Deleting a source with references asks the user to turn them into values.

See `specs/036-password-redesign/research.md` R3, R4, R5, R6, R8, R9 and
`specs/036-password-redesign/contracts/`.
