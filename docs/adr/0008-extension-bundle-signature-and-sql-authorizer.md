# Extension bundles carry a per-file signature, and extension SQL passes two independent checks

Status: accepted
Date: 2026-10-02

## Decision

holzi hosts haextensions (spec 017, building on ADR-0004). Two decisions reach
beyond holzi into the vault-sdk and haex-crdt and are recorded here.

**1. Bundle format `haextension-bundle/2`.** An extension bundle (`.xt`, a zip)
carries `haextension/manifest.json` as byte-exact canonical JSON (RFC 8785) and
a separate `haextension/signature.json` that lists the path, size and SHA-256 of
every other entry. The Ed25519 signature covers the domain prefix
`"haextension-bundle/2\n"` followed by the canonical form of that file without
its `signature` field. holzi verifies the list against the archive and the
signature against the manifest's public key before installation and before
every start. Bundles in the haex-vault 2.x (Legacy) format, which signs only the
concatenated file contents, are rejected with a hint to re-sign them with the
`haex` tool of the vault-sdk. The archive rules (paths, duplicates, sizes,
compression ratio) are in `specs/017-extension-host/contracts/bundle-format.md`.

**2. SQL from extensions is checked twice, by layers that do not depend on
each other.**

- A pre-check in Rust, ported from haex-vault 2.x and corrected while porting,
  parses the statement, rejects anything but one read or write, collects every
  table it touches (including `WITH`, `EXISTS` and subqueries in every clause)
  and classifies it as own, another extension's or core. It knows the needed
  permissions before execution, so holzi can ask the user without holding the
  database lock.
- The SQLite authorizer, set by haex-crdt only around the extension's own
  statement (`Database::write_guarded` / `read_guarded`), allows reads and
  writes only of the extension's own tables and of tables of other extensions it
  holds a grant for, functions only from an allowlist, and inside triggers only
  haex-crdt's own `z_dirty_*` triggers. It sees what SQLite actually resolves,
  so a form the parser misses is still stopped.
- Both layers use an **allowlist**: own tables and granted extension tables.
  Core tables (holzi's own, haex-crdt's, SQLite's) are never reachable by SQL
  from an extension, and no grant — remembered or temporary — can change that.
  Core data is reached only through typed host functions, such as the password
  functions of spec 034.

**The caller is the frame, never the payload.** An extension's identity comes
from the frame session that holzi's own frontend opens for a tab; nothing an
extension sends selects or changes it (ADR-0004).

## Rationale

The analysis of haex-vault 2.x (spec 017, research R6, R2) found that its signature
lets content move between files and files be renamed or added without the key,
and that its AST-only SQL check misses `WITH`, `EXISTS`, schema qualifiers and
migration statements. holzi supersedes haex-vault 2.x, so compatibility with a weak
format is not worth keeping. A parser alone has to know every place SQL can
name a table; the authorizer closes that class of gaps for about 150 lines of
code, and the bypass corpus of the spec must be rejected by each layer alone.

## Consequences

- The vault-sdk's `haex` tool writes and verifies the new format (major
  version 4); existing bundles must be rebuilt.
- haex-crdt gets `write_guarded` / `read_guarded`, trigger setup after DDL
  inside the transaction, a schema mode with foreign keys off and a rebuild that
  keeps the CRDT metadata. holzi pins the new revision by full SHA.
- Every delivery of spec 017 brings its cases of the bypass corpus.
- ADR-0005 stays reserved for spec 021; ADR-0007 is spec 034.

## Considered alternatives

- **Keep haex-vault 2.x's signature format**: existing bundles keep working, but
  the format is manipulable.
- **Only the AST check (haex-vault 2.x's approach)**: one missed position is enough
  for a breach.
- **Only the authorizer**: no permissions known in advance, so no prompt; errors
  would only appear during execution.
