# Data Model: Cards and tickets in the password manager (wallet passes)

Migration `0061_passwords_pass` (research R1). All tables are CRDT-tracked by the sync engine automatically (no `_no_sync`),
have a primary key, no UNIQUE constraint and no `WITHOUT ROWID` (034 FR-037). `HOLZI_TRIGGER_VERSION` 39 becomes 40.

## Changed table: `haex_passwords_item_details`

| Column       | Type                            | Note                                                                                                        |
| ------------ | ------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| `kind` (new) | `TEXT NOT NULL DEFAULT 'login'` | `login` or `pass`. Fixed at creation (FR-001). An unknown value is shown as "needs a newer holzi" (FR-042). |

All other columns are unchanged. For a pass, `username`, `password`, `otp_*` and `autofill_aliases` are empty (FR-007);
`title`, `note`, `url`, `icon`, `color` and `expires_at` carry the shared meaning, with `expires_at` as the end (a date).

## New table: `haex_passwords_pass_details` (1:1 with a pass)

| Column       | Type                            | Note                                                                                                                                     |
| ------------ | ------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| `item_id`    | `TEXT PRIMARY KEY`              | References `haex_passwords_item_details(id)` `ON DELETE CASCADE`. One row per pass.                                                      |
| `pass_kind`  | `TEXT NOT NULL DEFAULT 'other'` | `ticket`, `loyalty_card`, `voucher`, `membership`, `boarding_pass`, `other`. An unknown value reads as `other` (FR-004).                 |
| `start_date` | `TEXT`                          | `YYYY-MM-DD`; set when the start is a date alone. Exclusive with `start_at`.                                                             |
| `start_at`   | `TEXT`                          | UTC instant, RFC 3339 with milliseconds.                                                                                                 |
| `start_zone` | `TEXT`                          | IANA name (`Europe/London`) or fixed offset (`+02:00`) of the place. Set exactly when `start_at` is set.                                 |
| `parent_id`  | `TEXT`                          | Id of the entry this pass belongs to. **No foreign key** (R8): the parent may not have arrived by sync yet; cleared explicitly on purge. |
| `source_id`  | `TEXT`                          | `passTypeIdentifier/serialNumber` of an imported pkpass, for the duplicate check (R7). Not a secret, not shown as a field.               |

Index: `idx_haex_passwords_pass_details_parent_id (parent_id)`.

**Rules.** A pass has a `pass_details` row from its creation; a row missing for an entry of kind `pass` is read as
`pass_kind = other` with no start and no parent and repaired at the next write. `start_date` and `start_at` are never both set
(a write that sets one clears the other). A parent that equals the pass's own id, or whose chain leads back to it, is refused at
write (R8); a chain that loops through sync is shown and repairable.

## New table: `haex_passwords_pass_barcodes` (codes)

| Column     | Type                         | Note                                                                                                                  |
| ---------- | ---------------------------- | --------------------------------------------------------------------------------------------------------------------- |
| `id`       | `TEXT PRIMARY KEY`           | UUID (random; no unique rule on content).                                                                             |
| `item_id`  | `TEXT NOT NULL`              | References `haex_passwords_item_details(id)` `ON DELETE CASCADE`.                                                     |
| `format`   | `TEXT NOT NULL`              | `qr`, `code128`, `ean13`, `pdf417`, `aztec`, or any other text kept as received (FR-028: stored as text and flagged). |
| `value`    | `TEXT NOT NULL`              | The code's content. **A secret** (FR-025).                                                                            |
| `label`    | `TEXT`                       | Optional label ("Outbound", "Seat 14C").                                                                              |
| `position` | `INTEGER NOT NULL DEFAULT 0` | Order among the pass's codes.                                                                                         |
| `flag`     | `TEXT`                       | `NULL`, or `invalid_value` (arrived by import or sync and does not fit its format) or `unknown_format`.               |

Index: `idx_haex_passwords_pass_barcodes_item_id (item_id)`.

## Unchanged satellites (serve both types by `item_id`)

`haex_passwords_key_values` (free fields, seat, gate, issuer notes), `haex_passwords_group_items` (folder and trash),
`haex_passwords_item_binaries` (attachments), `haex_passwords_item_snapshots` and `haex_passwords_snapshot_binaries` (history),
`haex_passwords_item_tags`. `haex_passwords_passkeys` and `haex_passwords_passkey_links` stay for logins only; the service
refuses a passkey on a pass (R11).

## Header shape (`ItemHeader`, ts-rs binding `src/types/bindings/ItemHeader.ts`)

New fields, all derived by one extra `LEFT JOIN pass_details` in `items.rs` `load_headers`; **no code in any of them**:

| Field                               | For  | Note                                                           |
| ----------------------------------- | ---- | -------------------------------------------------------------- |
| `kind`                              | all  | `login` or `pass`; an unknown value is passed through as text. |
| `passKind`                          | pass | as above.                                                      |
| `startDate`, `startAt`, `startZone` | pass | as stored.                                                     |
| `parentId`                          | pass | as stored.                                                     |

`AgentHeader` and the agent tool result add `kind`, `passKind`, the three start fields and `expiresAt` for a pass (FR-027a).
The extension summary adds `type`. The existing five fields and the rules of 034 stay.

## Detail shape (single-entry read)

`ItemDetail` for a pass is the header plus note, free fields and attachments as today. **Codes are not in it.** A separate
single-entry command returns the codes (`passwords_pass_codes`, contracts), so that no list, header, log, error or tab history
can carry a value, and `passwords_pass_code_svg` returns the drawing of one code.

## History stand (`SnapshotData`, version 2)

A stand of a pass adds an optional block, `#[serde(default)]`:

```text
pass: { kind, passKind, startDate | startAt+startZone, codes: [{ format, value, label, position }] }
```

- `version` becomes 2 for new stands; a stand **without** a version reads as version 1 (research R2); a stand without the
  block reads as a login.
- Not in a stand: the parent link and the folder (restoring never moves or re-links), and nothing about passkeys.
- `changed_fields` names `kindOfPass`, `start`, `codes`; `SnapshotView` carries code formats and labels, and `reveal` returns one
  value on request.

## Entities and relations

```text
item_details (kind: login | pass)
  ├── 1:1  pass_details (kind = pass)      ── parent_id ─► item_details.id (soft link, query)
  ├── 1:n  pass_barcodes
  ├── 1:n  key_values, item_binaries, item_snapshots, item_tags
  ├── 1:1  group_items (folder or trash)
  └── 1:n  passkeys (kind = login only)
```

## State and lifecycle

| Event                 | Effect                                                                                                                   |
| --------------------- | ------------------------------------------------------------------------------------------------------------------------ |
| Create a pass         | One transaction writes `item_details` (kind `pass`), `pass_details`, codes, free fields, tags, folder and a first stand. |
| Edit                  | Same token (`updated_at`) as 034; one stand when anything in the stand changed.                                          |
| Change the type       | Not possible (FR-001); the user creates a new entry.                                                                     |
| Trash a pass          | As 034 (folder `trash`); its children keep their link and show "(in trash)" for it (client side).                        |
| Trash a parent        | Nothing changes in its passes; they show "(in trash)".                                                                   |
| Purge a pass          | `purge_item` deletes codes, detail row and the rest, children first; passes that pointed at it get `parent_id = NULL`.   |
| Remote delete         | Applied without foreign keys, so the same explicit list as purge covers it.                                              |
| Parent not synced yet | The pass shows "(not available)" until the entry arrives.                                                                |
| Loop from two devices | Detected on the client by walking parents with a visited set; every pass on the loop is flagged; "Clear link" repairs.   |
| Unknown `kind`        | Listed with a "needs a newer holzi" notice, not edited, not written over.                                                |

## Derived, not stored

- **Linked section** of an entry: `headers where parentId = id`.
- **Expired**: `expiresAt` before the device's local day (034 FR-008), for both types.
- **Sort key by date**: the start instant (or the date at 00:00 of the device's zone), else the end; upcoming nearest first,
  then past most recent first, then none (FR-021).
- **Folder names for search**: the names on the entry's folder path (R10).
- **Proposal of place** (FR-019): ticket-like and dated gives `Tickets` or `Tickets/<provider>` and the tag `Ticket`, else the
  root; computed by `pass/place.rs`, never stored, and no rule moves an existing entry.

## Validation rules (from the spec)

- A code typed by hand is checked against its format including check digits and refused with a reason; a code from import or
  sync is kept, flagged and repairable (FR-003).
- An end before the start is allowed (edge case); the pass is expired by its end.
- A value read by camera is stored as text and never opened (FR-034).
- The size limit of attachments stays 25 MiB (034 FR-020); the ZIP limits of an import are in R6.
