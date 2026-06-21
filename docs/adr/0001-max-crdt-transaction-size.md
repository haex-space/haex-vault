# ADR 0001 — Maximum CRDT transaction size

- **Status:** Accepted
- **Date:** 2026-06-21
- **Deciders:** Martin Drechsel

## Context

CRDT changes sync as length-prefixed JSON frames over QUIC (P2P) and as HTTP
bodies (home server). The receiver buffers a frame fully before parsing
(`peer_storage::protocol::read_message` allocates `vec![0u8; len]`), so an
unbounded frame length is a memory-safety / DoS hazard — today guarded by a
hard `MAX_RESPONSE_SIZE` of 10 MB, which simply *rejects* an oversized frame and
wedges the sync loop.

Some user data is genuinely large and **is** CRDT-synced: password-entry file
attachments are stored as blobs in a CRDT table (unlike folder `file_sync`,
which already transfers content out-of-band via `peer_storage`). So "a single
transaction larger than the frame cap" is a real, reachable state, and the 10 MB
value is arbitrary.

We want sync to paginate per source-transaction (one HLC = one transaction) and
apply each transaction atomically. That is only clean if a single transaction
has a known upper bound — otherwise one transaction can exceed any page budget
and we are back to the wedge.

## Decision

**A single CRDT transaction (all changes sharing one HLC timestamp) must not
exceed 100 MB of serialized change data. Payloads larger than that must use file
storage (`file_sync` / `peer_storage`), never CRDT columns.**

Enforcement is a hard guard in Rust, not a convention:

- **Where:** the mandatory CRDT write chokepoint `database::core::execute_with_crdt`
  (every write to a synced `haex_*` table goes through it — see the
  "SQL must use CRDT helpers" rule). This binds core, the password manager, and
  all extensions equally.
- **How:** a per-CALL size check inside `execute_with_crdt`, before the write.
  Measure the serialized byte size of the statement's bind parameters; if it
  exceeds the cap, fail with a typed `TransactionTooLarge { bytes, limit }` error
  and write nothing. This is provably equivalent to a per-transaction byte
  counter here because `execute_with_crdt` parses exactly one statement and wraps
  it in its own `BEGIN…COMMIT`, and rusqlite cannot nest transactions — so one
  call IS one whole transaction (one HLC). No running counter or commit/rollback
  hook is needed. The single-oversized-blob and many-small-rows cases are both
  covered: a multi-row `INSERT ... VALUES (..),(..)` is one statement whose
  params already contain every row, so their sizes sum in the one measurement.
- **UX:** `TransactionTooLarge` maps to a clear, localized message ("this
  attachment exceeds the 100 MB sync limit — store large files via file
  storage") surfaced to the user, instead of a silent sync wedge.

The cap is a deliberate **product limit**, not an internal frame size. 100 MB is
chosen to comfortably cover realistic attachments while keeping a single
transaction bufferable in memory during apply; revisit only if a concrete need
for larger single-transaction payloads appears.

## Consequences

- Per-transaction pagination becomes clean: with a page budget ≥ 100 MB, every
  HLC-group fits wholly inside one page, so pagination never splits a
  transaction.
- The QUIC wire cap is raised **uniformly on all paths** (owner mesh and shared
  spaces alike) to carry a 100 MB transaction: a transaction a user can create
  locally must be shareable into a space, so the cap cannot differ by path.
  Shared-space DoS is bounded by leader rate-limiting (PR #491), not by an
  artificially low frame size.
- Large attachments must round-trip through file storage; the password manager
  and extensions get a clear error rather than a wedge if they exceed the cap.
- Existing rows already larger than the cap (none expected — no production
  users) are not retroactively rejected; the guard is write-time only.

## Alternatives considered

- **No limit at all.** Rejected: an unbounded length prefix can OOM the
  receiver even from a trusted peer, and an unbounded single transaction defeats
  per-transaction pagination.
- **Out-of-band blob transfer for CRDT attachments** (chunked via `peer_storage`,
  CRDT row holds only a hash/pointer). Deferred (YAGNI) — only worth it if
  multi-GB CRDT-synced attachments become a real requirement; the 100 MB cap +
  file storage covers the foreseeable cases.
