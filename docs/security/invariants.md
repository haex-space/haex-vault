# Security Invariants

This is a living checklist of security guarantees haex-vault relies on. Each
entry states the guarantee, which component currently enforces it, and
whether it has a regression test. Where a guarantee is not (yet) enforced,
that is written down as a gap, not glossed over.

Source: adapted from `docs/plans/verbesserungsvorschläge.md` §15 (I1–I10),
renamed to match this codebase's actual components, plus two invariants
(I11, I12) documenting existing behavior that predates this file. §14's
five-question threat-model template is applied to three real boundaries
below.

**Note:** `docs/plans/verbesserungsvorschläge.md` is a local planning
artifact, gitignored (`docs/plans/*`) and not present in this repository —
it will not resolve as a link for anyone browsing the repo or an external
auditor. It is cited here only for provenance (which internal proposal this
list and template were adapted from); every claim in this file is otherwise
self-contained and verified directly against the code cited alongside it.

This file is a reference for future work (in particular CRDT adversarial
testing) — update it whenever an invariant's enforcement mechanism changes,
a regression test is added, or a gap is closed.

## Invariants

### I1 – Server Confidentiality

**Guarantee:** A compromised sync server cannot decrypt application payload
(column values).

**Enforced by:** Column values are encrypted client-side before push and
decrypted client-side after pull. By the time a value reaches the Rust apply
layer it is already plaintext locally — `RemoteColumnChange.decrypted_value`
in `src-tauri/src/crdt/commands/apply/types.rs:29` is documented as "Already
decrypted in frontend", confirming that on the wire / at rest the value must
have been ciphertext. The encryption/decryption itself lives in the TS sync
layer (`src/stores/sync/orchestrator/push.ts`, `src/stores/sync/engine/vaultKey.ts`)
— not re-audited line-by-line in this pass; this entry verifies the
architectural split (Rust apply path never handles ciphertext), not the TS
crypto implementation itself.

**Regression test:** none identified within the scope checked.

---

### I2 – Operation Authenticity

**Guarantee:** Only an authorized device/identity can create a valid change
for its own identity.

**Enforced by:** Per-column Ed25519 signatures binding `space_id +
table_name + row_pks + column_name + hlc_timestamp + author_did +
value_bytes` (`verify_column_sig`, called from
`src-tauri/src/crdt/commands/apply/db.rs:155-191`). Shared-space apply paths
reject missing signatures. Personal-vault (owner-only) sync between the
owner's own devices remains unsigned and instead relies on device/session
authentication under the owner's own account — that layer was not
re-verified in this pass.

**Forged/manipulated HLC timestamps: REJECT.** Because `hlc_timestamp` is
part of the signed Ed25519 preimage (`column_sig/preimage.rs::build_preimage`),
tampering with the HLC on a signed change invalidates the signature and the
apply pipeline rejects the change outright — the design does *not*
accept-and-reorder a change with a forged HLC. Rejection is row-scoped and
has no side effects (no identity-stub row is created for the forged
`author_did`).

**Regression test:** yes — `src-tauri/src/crdt/column_sig/verify_tests.rs`
(signature primitive), plus apply-pipeline-layer regression guards added
alongside this doc: `apply_rejects_change_with_forged_author_did` and
`apply_rejects_change_with_forged_hlc_timestamp` in
`src-tauri/src/crdt/commands/apply/db.rs` (both assert full-pipeline
rejection and the side-effect-free-fail property).

---

### I3 – Extension Table Isolation

**Guarantee:** An extension cannot read or write another extension's
database tables without an explicit, owner-approved permission entry.

**Enforced by:** Default-deny check in `PermissionManager::check_database_permission`
(`src-tauri/src/extension/permissions/manager/check/database.rs`) — an
extension's own tables (`{public_key}__{name}__` prefix) are auto-allowed
(`PermissionChecker::is_auto_allowed_table`); every other table requires a
matching, deny-first-resolved `ExtensionPermission` row. Schema DDL
(CREATE/ALTER/DROP) additionally hard-checks the table-name prefix in
`validate_sql_table_prefix` (`src-tauri/src/extension/database/helpers.rs`).
System tables (`haex_*`, `sqlite_*`) are excluded from wildcard (`*`) grants
(`is_system_table`, `src-tauri/src/extension/permissions/checker.rs:224`).

**Regression test:** not individually re-run in this pass; the
deny-first-precedence matching logic follows this project's general
permission-check-helper convention (pure, unit-testable matcher functions).

---

### I4 – Secret (Password) Access

**Guarantee:** No extension gets raw SQL access to password storage; access
is only through the typed password API.

**Enforced by:** Each `extension_password_*` command calls
`PermissionManager::check_passwords_permission`
(`src-tauri/src/passwords/commands/mod.rs`) — a dedicated permission
resource type, separate from generic DB permissions. Raw SQL access to
`haex_passwords_*` is not auto-allowed and is excluded from wildcard (`*`)
grants (`is_system_table`). The module's own comment states access is
"forbidden by policy" — note this is a strong convention backed by the
wildcard exclusion, not an absolute code-level impossibility: a manifest
could in principle request an exact-name grant for a `haex_passwords_*`
table and have it owner-approved via the permission-prompt UI (not audited
in this pass).

**Regression test:** not identified in this pass.

---

### I5 – Capability Integrity

**Guarantee:** Changing a capability's action, resource, subject, scope, or
expiry invalidates it.

**Enforced by:** UCAN token verification plus the `CapabilitySet`/predicate
model (`src-tauri/src/ucan/capability_set.rs`; chain verification in
`src-tauri/src/ucan/commands.rs`) — any field change breaks the Ed25519
signature over the token, so verification fails. This migrated from an
earlier, coarser `CapabilityLevel` model; the `CapabilityLevel` →
`CapabilitySet` migration (W4) is still in progress, and a follow-on
"Single Authority Plane" authorization plan is blocked on its completion
(see `.claude/decisions.md`, 2026-08-14 entry).

**Regression test:** yes, extensively — MLS/UCAN membership-authorization
phases 1–3 shipped with e2e attack-spec coverage (PR #778/#780/#781/#783).
Phase 4 and an external-commit proof-of-possession gap remain open.

---

### I6 – MLS Integrity

**Guarantee:** An authorization proof is bound to the exact MLS state/commit
it was issued against.

**Enforced by:** `src-tauri/src/mls/commit_bind.rs` plus
`src-tauri/src/mls/authorization.rs`. A historical receiver-side gate gap
(divergence between the authorization check and the actual synced
capability source) was fixed via UCAN-on-commit binding (PR #782).

**Regression test:** yes — bind-replay and forged-UCAN e2e attack specs
pass (haex-e2e-tests companion repo).

---

### I7 – Replay Resistance

**Guarantee:** A previously-accepted operation cannot be accepted again as a
new one (e.g. a pruned/old delete cannot resurrect a row or be replayed).

**Enforced by:** HLC-ordered comparison gates —
`should_propagate_delete` / `delete_shadows_insert` in
`src-tauri/src/crdt/commands/apply/delete_propagation.rs:173-190` reject a
delete/insert that is not strictly newer than the target row's current HLC.
Shared-space delete-log additionally uses a per-space compaction anchor
(`haex_space_compaction_anchors`) so a stale peer cannot reintroduce a row
whose delete-signal has already been pruned
(`src-tauri/src/crdt/scanner.rs:50-54`).

**Regression test:** yes —
`shared_space_delete_log_apply_preserves_resurrection_bug_free` and related
tests in `delete_propagation.rs`, plus apply-pipeline-layer guards added
alongside this doc:
`apply_is_idempotent_when_identical_unsigned_change_set_is_applied_twice`,
`apply_is_idempotent_when_signed_change_is_replayed`, and
`apply_v10_then_receiving_stale_v7_does_not_roll_back` in
`src-tauri/src/crdt/commands/apply/db.rs`.

---

### I8 – Sync Safety

**Guarantee:** A malformed remote operation never produces an unauthorized
local state.

**Enforced by:** Fail-closed handling throughout the apply path —
`is_safe_identifier` (`src-tauri/src/crdt/trigger.rs:170`) rejects unsafe
table/column identifiers before any SQL is built; the delete-propagation
register-gate lookups fail closed on DB error (skip the entry rather than
authorize a delete) (`delete_propagation.rs:511-565`); inbound peer changes
to non-whitelisted tables are rejected unless the exact row is individually
registered for that space
(`src-tauri/src/space_delivery/local/inbound_sync/validate.rs:73`).

**Regression test:** yes — `test_is_space_scoped_table_whitelist`
(`src-tauri/src/crdt/scanner_tests.rs:328`) plus the delete-propagation
fail-closed/register-gate tests. The apply-pipeline-layer replay/idempotence
tests added alongside this doc
(`apply_is_idempotent_when_identical_unsigned_change_set_is_applied_twice`,
`apply_is_idempotent_when_signed_change_is_replayed`,
`apply_v10_then_receiving_stale_v7_does_not_roll_back` in
`src-tauri/src/crdt/commands/apply/db.rs`) double as
"malformed/adversarial remote op → no unauthorized local state" guards for
this invariant.

---

### I9 – Lock State

**Guarantee (as originally proposed):** Vault locked → no secret operation
possible.

**Status: not currently enforced — this is a gap, not an implemented
guarantee.** This codebase has `VaultLock`
(`src-tauri/src/database/vault_lock.rs`), but that is a cross-*process*
advisory file lock preventing two OS processes from opening the same vault
DB concurrently (corruption prevention) — it is not a security "locked but
mounted" session state. No auto-lock, re-lock, or inactivity-timeout
mechanism was found in `src-tauri/src` or `src`. Today a vault is binary:
closed (encrypted, unopenable without the password) or open (fully
decrypted, all operations available) — there is no intermediate "locked"
state to gate secret operations against.

**Enforced by:** N/A.

**Regression test:** none — do not claim this invariant holds until a
distinct lock state exists.

---

### I10 – Extension Command Surface

*(Replaces the original "Extension Trust Boundary" I10, which referenced a
"secret extension" tier tied to the HaexPass extension. HaexPass was removed
from the project — PR #51 closed — so that tier no longer exists. Replaced
with the actual mechanism that gates what any extension can reach at all.)*

**Guarantee:** A webview extension can only invoke the specific Tauri
commands granted to `ext_*` webviews; owner-only commands can never be
invoked directly from an extension's webview.

**Enforced by:** `src-tauri/permissions/extension-commands.toml` —
`allow-extension-commands` is a positive allowlist bound to
`webviews: ["ext_*"]`. Owner/admin commands (permission granting,
session-permission management, extension-limit changes) exist only in
`allow-app-commands` (the main window's capability) and are never listed in
`allow-extension-commands`. Extension-facing commands additionally resolve
identity server-side via `resolve_extension_id` (window label, or iframe
public-key+name bound to the calling MessagePort — never a client-supplied
`extension_id` parameter), and owner-only commands are further gated by
`require_main_window()` as defense-in-depth.

**Regression test:** yes —
`src/tests/extensions/permission-command-allowlist.test.ts` asserts
owner-only commands are excluded from the extension allowlist. Historical
incident: PR #512 found `grant_session_permission` /
`resolve_permission_prompt` briefly reachable from extensions — a self-grant
sandbox escape — fixed by moving them to `allow-app-commands` only, plus the
`require_main_window()` gate.

---

### I11 – Delete-Propagation Positive-Register-Gate *(existing, not new work)*

**Guarantee:** An absent register entry never produces a business-row
DELETE.

**Enforced by:** `propagate_shared_space_deleted_rows_to_target_tables` in
`src-tauri/src/crdt/commands/apply/delete_propagation.rs:496-566`. Three-way
gate on the `(table, row, space)` register lookup:

- registered in the target space → positive authorization, delete proceeds;
- not registered anywhere → no-op (race with a local unshare — per ADR 0002
  §6.5, unshare must not hard-delete rows);
- registered in a *different* space → no-op (suspected `NotSharedInSpace`
  forgery; both the business row and the other space's register entry are
  left untouched).

All three register/count lookups fail closed: a DB error skips the entry,
never authorizes a delete.

**Regression test:** yes —
`shared_space_delete_log_apply_is_idempotent_on_race_with_local_unshare`,
`shared_space_delete_log_apply_rejects_when_row_not_shared_in_that_space`,
`shared_space_delete_log_apply_only_removes_matching_space_register_entry`
(all in `delete_propagation.rs`).

---

### I12 – haex_logs / Vault-Scoped-Table Confinement *(existing, not new work)*

**Guarantee:** `haex_logs` (and other vault-scoped system tables) are never
synced to peers of a shared space — only to the owner's own sync server and
the owner's own other devices.

**Enforced by:** `SPACE_SCOPED_CRDT_TABLES`, a default-deny whitelist
(`src-tauri/src/crdt/scanner.rs:37-55`) — only tables explicitly listed
there are eligible for space-delivery; `haex_logs` is not among them.
Enforced on both sides of the wire: the outbound scanner only ever walks
whitelisted tables for space-delivery, and inbound peer changes to
non-whitelisted tables are rejected unless the exact row is individually
registered for that space (`is_space_scoped_table`, checked in
`src-tauri/src/space_delivery/local/inbound_sync/validate.rs:73`).

**Regression test:** yes — `test_is_space_scoped_table_whitelist`
(`src-tauri/src/crdt/scanner_tests.rs:328`) asserts vault-private tables
(e.g. `haex_identities`, `haex_vault_settings`, `haex_ucan_tokens`) resolve
to non-space-scoped. `haex_logs` itself is not named in that specific
assertion, but is covered by the same default-deny mechanism (absence from
the whitelist means `is_space_scoped_table` returns `false`).

---

## Threat Model: Three Boundaries

Applying the five-question template from
`docs/plans/verbesserungsvorschläge.md` §14 to three real boundaries in this
codebase.

### Boundary 1: Compromised Sync Server

**Attacker.** The operator/administrator of the sync server, or anyone who
compromises it — full read/write access to whatever the server stores, and
full control over what it forwards.

**What can they see?** Metadata: `table_name`, `row_pks` (JSON-encoded
primary key values), `column_name`, `hlc_timestamp`, and the per-column
`author_did` + Ed25519 signature (all present as separate wire fields in
`RemoteColumnChange` / the column-sig verification inputs — confirmed in
`src-tauri/src/crdt/commands/apply/types.rs` and `db.rs:155-191`), plus
request timing and sizes (inherent to any HTTP/Supabase transport).

**What can they modify?** They can drop, delay, replay, or reorder synced
changes — the server is a relay/store, not a participant in the CRDT
computation.

**What cryptographic property prevents the attack?** AEAD encryption of
column values (confidentiality — the Rust apply layer never sees anything
but already-decrypted local values, see I1); per-column Ed25519 signatures
binding table/row/column/HLC/author (integrity — undetected tampering or
forging a change under another identity is rejected, see I2); HLC-ordered
anti-resurrection and anti-replay checks at apply time (see I7).

**What can they still do?** Availability attacks (withhold data
indefinitely); traffic analysis (table identifiers, HLC timestamps,
`row_pks`, and per-column author DIDs are visible even though the value
itself is opaque — this leaks *which* tables/rows change, *how often*, and
lets an observer pseudonymously correlate a device/identity's activity
across changes via its DID); general metadata leakage. This matches the
plan document's original §14 draft, with one concrete refinement confirmed
in code: the per-column author DID is also visible wire metadata, not just
generic "CRDT metadata."

---

### Boundary 2: Compromised/Malicious Extension

**Attacker.** A normal-tier extension, already installed and running as an
`ext_*` webview, whose code is fully attacker-controlled. Assumes the Tauri
capability system and per-call permission checks execute as written (this
boundary is about what the *design* permits, not about bugs in the
enforcement code itself).

**What can they see?** Its own database tables (auto-allowed); anything
explicitly owner-granted per its manifest (other tables, filesystem paths,
web/mail/password scopes); the extension command surface listed in
`allow-extension-commands` (`src-tauri/permissions/extension-commands.toml`)
— this is the entire reachable Tauri IPC surface; nothing outside this list
is dispatchable from an extension webview at all (see I10).

**What can they modify?** Its own tables/files; only permission-granted
*and* owner-approved resources beyond that (default-deny —
`PermissionManager::check_database_permission` /
`check_filesystem_permission`, see I3).

**What cryptographic/structural property prevents escalation?** Extension
identity is resolved server-side (`resolve_extension_id` — window label, or
iframe public-key+name bound to the calling MessagePort — never a
client-supplied `extension_id` string), so one extension cannot spoof
another extension's identity to reach its resources or its logs. Owner-only
commands (permission granting, session-permission management,
extension-limit changes) are absent from `allow-extension-commands`
entirely, and are additionally gated by `require_main_window()`.

**What can they still do (residual risk, stated honestly)?**

- The DB/filesystem permission model is a deny-first allowlist, not a hard
  technical wall for every resource: an extension manifest could in
  principle request an exact-name permission targeting another extension's
  table, or (per `is_system_table`) even a system table, and if the *owner*
  approves that prompt, access is granted. The barrier here is user consent
  and prompt-dialog design (not audited in this pass), not cryptographic
  impossibility.
- Once granted broad permissions it actually asked for and got (e.g.
  `web_fetch`, filesystem access), an extension can exfiltrate whatever it
  is allowed to read — inherent to any capability-based extension model, by
  design.
- The allowlist in `extension-commands.toml` is a manually-maintained list.
  PR #512 found a genuine self-grant sandbox escape (owner-only commands
  briefly reachable from `allow-extension-commands`); the automated test
  `permission-command-allowlist.test.ts` is the regression guard against a
  repeat, not a structural impossibility.

---

### Boundary 3: Local Untrusted Process → External Bridge

**Attacker.** An arbitrary local process on the same machine — not the
legitimate browser extension, not a remote network attacker — e.g. malware,
another locally-installed application, or an unrelated compromised browser
extension, that can open a TCP connection to the external bridge's
WebSocket port on `localhost`.

**What can they see?** The server's ephemeral X25519 public key, handed out
to anyone who connects (`ServerKeyPair`,
`src-tauri/src/external_bridge/server/connection.rs`); with that, they can
complete a Diffie-Hellman handshake and open what looks like an
authenticated session using *any* `client_id` string they choose, since
`client_id` is a bare, self-declared field in the handshake envelope
(`EncryptedEnvelope.client_id`,
`src-tauri/src/external_bridge/crypto.rs:107`) — not cryptographically bound
to the X25519 key material used for that connection.

**What can they modify?** They can send a handshake claiming any
`client_id`, `client_name`, and requested extensions/permissions. First
contact for a brand-new `client_id` does require a human-in-the-loop
approval (`PendingAuthorization`, shown with name/public-key/requested
scopes in an owner-facing dialog,
`src-tauri/src/external_bridge/authorization.rs:78-93`) — this is a real,
verified mitigation for a *first* connection.

**What cryptographic property prevents the attack?** Partial only.
AES-256-GCM with ephemeral X25519 DH gives confidentiality and forward
secrecy for a session's *content* once a shared secret is derived. But
server-side authorization keys off the `client_id` string, not off any bound
key material: `SQL_IS_CLIENT_KNOWN` resolves as bare `WHERE client_id = ?1`,
while `SQL_IS_AUTHORIZED` (`src-tauri/src/external_bridge/authorization.rs:100-103`)
adds `AND extension_id = ?2` and `SQL_IS_CLIENT_AUTHORIZED_FOR_EXTENSION`
(`authorization.rs:225-231`) joins on the target extension's `public_key`/`name`
as well — but in both cases that extra condition narrows *which extension* the
client may reach, not *whether the connecting process is who it claims to
be*. None of these queries check that the connecting process's own public
key matches the public key recorded for that `client_id` at authorization
time.

**What can they still do? (KNOWN GAP — write this down honestly, do not
mark it solved)**

`"Localhost != trusted"` is **not** fully enforced by this bridge today:

1. Once a `client_id` has been authorized (e.g. for the legitimate browser
   extension), any other local process that later connects and declares the
   *same* `client_id` string is treated as the same authorized client —
   `check_client_authorized` / `check_client_authorized_for_extension` never
   re-verify a bound public key. This is exactly the gap
   `docs/plans/verbesserungsvorschläge.md` §3.1 calls out ("client_id →
   permission lookup" instead of "client_id → long-term identity → proof of
   possession → session key") — confirmed still present in the current
   code, not yet remediated.
2. No Origin/Host header validation was found in the WebSocket accept path
   (`tokio_tungstenite::accept_async` is called with no custom header
   callback in `server/connection.rs`), so the §3.8 "Origin-/Browser
   Boundary" concern (malicious webpage → localhost WebSocket, DNS
   rebinding) is also unaddressed as far as this bridge's own code goes.
3. Concretely, an attacker that can connect locally and learn or guess a
   previously-authorized `client_id` (e.g. by reading it out of the
   legitimate extension's local storage — a plausible capability for
   anything already running as the same OS user) can impersonate that
   client's session and issue authorized requests without ever proving
   possession of the original client's key material. What it cannot do
   without further work: talk its way past the first-contact owner-approval
   dialog for a *new* `client_id`, or read another concurrent session's
   ciphertext without deriving that session's own shared secret.

This boundary should be treated as an open finding, not a documented-away
risk — it is the strongest candidate for follow-up hardening work in this
codebase's threat model.
