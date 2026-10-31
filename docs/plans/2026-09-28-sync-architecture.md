# Sync Architecture — Own Devices, Spaces, Data Shares, Blind Relay

**Status**: Design, written 2026-09-27/28 from an operator brainstorm session. Not yet a spec. The
spec cut in §14 is a proposal; every requirement here has to be re-expressed as numbered spec
requirements before code moves.

**Relationship to existing documents**:

- [`2026-09-07-cross-user-sharing-deferred-design.md`](./2026-09-07-cross-user-sharing-deferred-design.md)
  — **superseded in part**. Kept: dropping MLS (§2), NIP-44 v2 key envelopes (§2), the peer relay /
  buffer relay split (§3), "Nostr never carries CRDT deltas" (§4), the relay-readable signed
  membership projection (§4), the table whitelist as exfiltration guard (§6). Replaced: rows scoped
  into spaces through an M:N row register (§6 item 2), the separate per-person federation identity
  (§7; the vault identity plays that role, §4 below), and the HTTPS object-storage IAM sidecars (§5;
  replaced by §12 below).
- [`2026-09-04-v1-scope-design.md`](./2026-09-04-v1-scope-design.md) — §4 (vault = SQLite =
  identity, per-device Nostr/iroh keys) is the base this document builds on. Two lines there move:
  `blob.offer`/file transfer stops being post-v1 for the own-device case (§7), and §5's "iroh never
  carries state" no longer holds, because the `haex-crdt` sync channel runs over iroh (§6). The
  addendum at the end of v1-scope already named this tension.
- Reference implementations (read for lessons, not ported):
  - `haex-vault 2.x (Legacy)`: repository `https://github.com/haex-space/haex-vault-legacy`, revision
    `8dce379d94e18fcd42c3b73686a06f984ca3f574`.
  - `haex-sync-server`: repository `https://github.com/haex-space/haex-sync-server`, revision
    `e20abeb26991d187fe1a4a8f1bc69029e5b12177`.
  - `haex-crdt` as pinned by holzi: repository `https://github.com/haexmas/haex-crdt`, revision
    `ed230d2c3f58c1b10710b6025ea0ce6c20b8d009`.

---

## 1. Goals

1. **Data sync** of the SQLite vault between the devices running the same vault.
2. **File sync** between the devices running the same vault.
3. Both over **iroh**, secured via **Nostr** keys (identity, authentication, key wrapping).
4. A **blind relay**: an untrusted, multi-tenant server through which vaults of many unrelated users
   sync. It never sees plaintext.
5. **Sharing between different users**, reliable and secure:
   - **Files** through **spaces**, which behave like network folders with folder-wide rights.
   - **Data** (SQLite) through **data shares**, with rights granted individually per user.
   - Content on any foreign server is always encrypted and decryptable only by authorized users.

Non-goals for v1: hiding the participant set from the relay (metadata unlinkability), post-compromise
security, forward secrecy against removed members for content they could already decrypt,
re-delegation between people.

## 2. Operator decisions

| #   | Decision                                                                                                                                                                                                                                                                                              | Date       |
| --- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------- |
| D1  | A Nostr pubkey identifies one **device**. The same vault runs on several devices at once, each with its own Nostr identity; the **vault identity** is the same on all of them.                                                                                                                        | 2026-09-27 |
| D2  | **Spaces are network folders for files only.** `read` = read all files in the space; `write` = add new files and modify existing ones.                                                                                                                                                                | 2026-09-27 |
| D3  | **SQLite data is not shared via spaces.** Read/write on data is granted individually per user.                                                                                                                                                                                                        | 2026-09-27 |
| D4  | Data shares exist for **single entries and whole collections**. Extensions declare what is shareable through **one uniform schema** the core can check for every extension.                                                                                                                           | 2026-09-27 |
| D5  | An extension may only declare shareables over **its own tables** (prefix `{public_key}__{extension_name}__`).                                                                                                                                                                                         | 2026-09-28 |
| D6  | The **creator of a space or share is its only admin**: invites members, assigns, changes and revokes rights, removes members.                                                                                                                                                                         | 2026-09-27 |
| D7  | Only the admin invites. A recipient cannot re-share. Downloading content and sharing it again in their own space is out of scope to prevent.                                                                                                                                                          | 2026-09-27 |
| D8  | ~~Superseded by D27.~~ The **vault identity private key lives on every device** of the vault (inside the SQLCipher database). A lost or stolen device is locked out by rotating the vault identity.                                                                                                   | 2026-09-28 |
| D9  | For v1 the relay guarantee is **"the operator sees no content"**. Per-space pseudonyms are not v1.                                                                                                                                                                                                    | 2026-09-28 |
| D10 | Concurrent edits of the same file produce a **conflict copy**.                                                                                                                                                                                                                                        | 2026-09-28 |
| D11 | **The relay is untrusted.** It never holds a user's own S3 credentials.                                                                                                                                                                                                                               | 2026-09-28 |
| D12 | **Both storage backends in v1**: storage provided by the public relay (A) and the user's own S3 (B).                                                                                                                                                                                                  | 2026-09-28 |
| D13 | **No MLS.** Its ordered epoch chain does not fit an order-free CRDT (§3.3).                                                                                                                                                                                                                           | 2026-09-27 |
| D14 | Vault copies made before sync keep **one shared vault identity**, derived from the common placeholder.                                                                                                                                                                                                | 2026-09-28 |
| D15 | Until the own relay exists, presence, NAT traversal and invitations use **preset public Nostr and iroh relays**, changeable in settings.                                                                                                                                                              | 2026-09-28 |
| D16 | Synced files live in a **user-chosen folder of the file system** on each device (own folders and spaces).                                                                                                                                                                                             | 2026-09-28 |
| D17 | A scope lives on **exactly one home relay** in v1; further relays only help with NAT traversal.                                                                                                                                                                                                       | 2026-09-28 |
| D18 | Delete: `write` deletes own entries or files, `delete` is needed for everyone else's (§11).                                                                                                                                                                                                           | 2026-09-28 |
| D19 | Revocation acts **forward only**, with a receipt-time check against the newest known member list and an immediate relay block; the remaining window is accepted (§11).                                                                                                                                | 2026-09-28 |
| D20 | Own S3 in v1: **RustFS and AWS S3**, verified and tested. MinIO is dropped (no longer open source); R2, B2 and Hetzner follow after verification.                                                                                                                                                     | 2026-09-28 |
| D21 | Only the **admin** of a scope uploads compaction snapshots; in the vault scope any own device.                                                                                                                                                                                                        | 2026-09-28 |
| D22 | **Invites also create a new key generation**; key generations and member-list versions stay one counter.                                                                                                                                                                                              | 2026-09-28 |
| D23 | **No admin hand-over in v1.** (Rotation part superseded by D26.) On rotation holzi closes the scopes the vault administers and leaves the others, signed with the old identity; nothing depends on members' consent.                                                                                  | 2026-09-28 |
| D24 | On storage backend A **the relay transfers the encrypted objects itself**; there are no presigned links.                                                                                                                                                                                              | 2026-09-28 |
| D25 | S3 providers that fail the setup check **cannot be connected**; no fallback path.                                                                                                                                                                                                                     | 2026-09-28 |
| D26 | **No identity rotation in v1.** A lost device is no problem while a copy or the relay exists and the passphrase holds. Replaces the rotation parts of D23; the admin role stays non-transferable.                                                                                                     | 2026-09-28 |
| D27 | **Main and linked devices**: only main devices hold the vault identity private key and may add or remove devices; several are possible; a signed device list replaces attestations; linking is preferred over copying; transferring the vault key when linking is opt-in with a warning. Replaces D8. | 2026-09-28 |
| D28 | Space and share keys are wrapped to **every device** on the member vault's device list.                                                                                                                                                                                                               | 2026-09-28 |
| D29 | **Any device** of the admin vault may manage spaces and shares; only device management needs a main device.                                                                                                                                                                                           | 2026-09-28 |
| D30 | Only the vault identity private key is direct-only; S3 credentials and content keys are ordinary, recoverable vault data.                                                                                                                                                                             | 2026-09-28 |
| D31 | Optional **recovery package** on the relay: recovery key never leaves the device, possession proof plus second factor (TOTP or e-mail link).                                                                                                                                                          | 2026-09-28 |
| D32 | The relay syncs **SQLite data only**; files go only through optional operator storage (backend A). A **password manager** ([spec 034](../../specs/034-password-manager/spec.md)) holds secrets such as S3 credentials.                                                                                | 2026-09-28 |
| D33 | **Invites are links the admin creates and hands over**, never a message to a bare vault identity: one-time secret, expiry, withdrawable; the admin confirms the acceptance. Messages without a valid open secret are dropped silently.                                                                | 2026-10-01 |

D33 is superseded in one point by [ADR-0014](../adr/0014-invitations-from-the-address-book.md)
(spec 051): an invitation may also be delivered to a contact whose business card holzi has verified.

## 3. What exists today, and what the references teach

### 3.1 holzi

- No sync transport: no iroh, Nostr, secp256k1 or MLS dependency in `src-tauri/Cargo.toml` or
  `package.json`.
- `haex-crdt` is used for local tracking only; no `scan_*` or `apply_remote_changes` call exists.
  `NoopSignatureProvider` is configured in `src-tauri/src/instances/vault_config.rs`.
- `src-tauri/src/identity/bootstrap.rs` mints a **placeholder** `vault_identity` keypair (random
  bytes shaped like secp256k1 keys); real key generation was explicitly deferred to the sync slice.
- `vault_identity` has no `_no_sync` suffix, so its private key replicates with the vault. Under D27
  it must reach only main devices, so this storage has to change.
- `known_devices` (`src-tauri/src/storage/known_devices.rs`) maps installation UUIDs to vault-device
  UUIDs; the vault-device UUID is the HLC node id.

### 3.2 haex-crdt (pinned revision)

- Column-level LWW on uhlc HLCs, metadata columns inline in each business table; deletes are hard
  deletes plus a synced `haex_deleted_rows` log; `_no_sync` tables and columns are excluded.
- Scanner (`src/crdt/scanner/mod.rs`) and apply (`src/crdt/apply/`) are **transport-agnostic**.
- Hooks this design relies on:
  - `ScanFilters` — `origin_node`, `row_pks` allow-list and `column_eq` for sender-side scoping.
  - `ApplyPolicy::prepare_row` (`src/crdt/apply/policy.rs`) for receiver-side admission.
  - `SignatureProvider` (`src/signature.rs`) with `sign_column`, `verify_column` and
    `on_before_apply`; the `sig` JSON is opaque.
- **Defect this design must fix**: `ColumnChange.device_id` is filled with the _scanning_ store's
  device, not the original writer's. Over more than one hop (A → B → C, or via the relay) authorship
  is mislabeled. The true origin is only in the HLC node suffix, and a node id says nothing about
  which vault authored a change in a shared context (§5.4).

### 3.3 haex-vault 2.x: why MLS + CRDT was cumbersome

haex-vault 2.x ran one openmls group per space (a leaf per device), exported a per-epoch key, and synced
that key table through the CRDT. The friction is structural, not incidental:

- **Two channels with different consistency models.** MLS needs a total order of commits; CRDT
  membership rows merge order-free. A receiver that has not yet applied an ADD sees the same state as
  "member removed", so peers can accept or reject the same commit differently. The code marks this
  `KNOWN DIVERGENCE RISK` (`src-tauri/src/mls/authorization.rs`).
- Epoch gaps detected by matching error strings, leader-held commit buffers, rejoin loops, stale
  group state that permanently locks a device out, and duplicate signature keys on re-invite.
- openmls has no role model; admin/invite policy had to be layered on with extra signatures, and
  external-commit joiners cannot carry the proof-of-possession.
- A leaf per device made multi-device fragile (removing a member by DID removed only one leaf).
- A missing epoch key stalls the whole pull, because the HLC group cannot be applied partially.

**Lesson**: keep key management inside the CRDT's own consistency model. Key epochs become ordinary
CRDT rows that may exist concurrently (§5.2); wrap to the _vault_, not the device (§5.3).

haex-vault 2.x patterns worth keeping: the `{public_key}__{extension_name}__` table prefix
(`src-tauri/src/extension/utils.rs`), the space-table whitelist `SPACE_SCOPED_CRDT_TABLES`
(`src-tauri/src/crdt/scanner.rs`), per-object DEKs with chunked AEAD and opaque object names
(`src-tauri/src/file_sync/crypto/envelope.rs`), and ADR 0002's decision that removed members keep
historical read access
(`docs/adr/0002-shared-space-authenticity-and-confidentiality.md`).

### 3.4 haex-sync-server: shape yes, data model no

The **shape** is right: multi-tenant, stores ciphertext, co-hosts an iroh relay. The **data model**
is not what holzi wants:

- One row per cell with `table_name`, `row_pks`, `column_name`, `hlc_timestamp`, `device_id`,
  author and epoch in cleartext (`src/db/schema.ts`); only the value is encrypted. The server sees
  the app schema, activity per device, and natural-key PKs.
- Server-side LWW merge in `POST /sync/push` (`src/routes/sync.ts`) — the server must understand
  the change format.
- Registration requires an email address; the encrypted key backup is recoverable by email OTP.
- UCAN authorization (`src/middleware/ucanAuth.ts`) has no revocation list: a removed member keeps
  access until the UCAN expires.
- Blob storage (`src/routes/storage.ts`) is one bucket per user, not per space, and the server holds
  full MinIO credentials.

## 4. Identity model

**Revised 2026-09-28 (D26–D30); replaces D8 and the rotation parts of D23.**

| Key                                   | Scope                 | Held by                           | Used for                                                                                                                       |
| ------------------------------------- | --------------------- | --------------------------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| **Device key** (secp256k1, Nostr)     | one device (instance) | that device only, never leaves it | the device's own identity: signing changes, lists and file-index entries, NIP-42 auth at relays, receiving key envelopes (D28) |
| **iroh endpoint key** (ed25519)       | one device            | that device only                  | QUIC transport; bound to the device key by a signed statement                                                                  |
| **Vault identity** (secp256k1, Nostr) | one vault             | **main devices only** (D27)       | the vault's stable public address (grants, member lists, invitations); signs the device list                                   |

- **Main devices and linked devices (D27)**: a vault runs on several devices, each an instance with
  its own device key. A **main device** (Hauptgerät) holds the vault identity private key and may add
  and remove devices; there may be several. The first instance is a main device. A **linked device**
  (verknüpftes Gerät) reads and writes all vault data and may manage spaces and shares (D29), but
  cannot add or remove devices.
- **Device list** (Geräteliste): signed with the vault identity by a main device; lists every current
  device (public device key, role, network id, and the device name encrypted for the vault's own devices only) with a generation. It follows the member-list
  rules (§10.3): higher generation replaces lower, same generation → smallest hash wins, fail
  closed. Relay, own devices and member devices accept a device only if it is on the vault's current
  device list. It replaces per-device attestations.
- **Linking** is the preferred way to add a device: the new instance joins via code or QR from a main
  device, which adds it to the device list and wraps the vault's content-key generations to its
  device key. holzi asks whether to also transfer the vault identity private key (making it a main
  device); the default is no, with the warning that a compromised main device plus a known
  passphrase is a total loss. The key travels only over the direct link. Copying the SQLite file
  stays possible as a secondary way; the copy gets a new device key and syncs only once a main device
  lists it (a copy of a main device can list itself).
- **Removing** a device (main devices only): a new device list without it, plus a new generation of
  the vault's own content key wrapped to the remaining devices. From then on nobody delivers anything
  to it and it cannot decrypt new changes; data already on it stays. Removing a main device only
  works against an honest device.
- **No identity rotation in v1 (D26)**: a lost device is no problem as long as a copy or the relay
  exists and the passphrase holds. The admin role of a space or share is never transferable (D23).
- Presence and addressing: each device publishes its current iroh `NodeAddr` as an encrypted Nostr
  event for its own vault's devices. This replaces iroh's default pkarr/DNS discovery, so no
  third-party discovery infrastructure is required.

## 5. Common building blocks

All three planes (own devices, spaces, data shares) use the same primitives.

### 5.1 Sealed batch

The unit that travels over iroh and through the relay:

```
SealedBatch {
  scope_id,        // vault id, space id or share id (opaque on the relay)
  key_id,          // which content key encrypted this batch
  ciphertext,      // XChaCha20-Poly1305 over the canonical payload below
}
payload = {
  author_device_npub, author_vault_npub,
  changes: [ColumnChange with per-column sig],   // one or more complete HLC groups
}
```

- Batches never split an HLC transaction group (`paginate_changes` already guarantees this).
- Application-level encryption is used **also on direct iroh links** between own devices. Transport
  encryption would suffice there, but one format means any node — a third own device or the relay —
  can store and forward a batch without seeing plaintext, and there is only one code path.

### 5.2 Key epochs as CRDT rows

```
scope_keys(scope_id, key_id, epoch, created_by_vault, created_at_hlc,
           envelopes: { recipient_vault_npub -> NIP-44 v2 envelope })
```

- A rotation inserts a new row; it does not replace a chain link. Two concurrent rotations produce
  two valid keys — no fork to resolve.
- Every ciphertext names its `key_id`. A receiver decrypts with whatever key it holds; it never needs
  a globally agreed "current" epoch to read.
- A sender encrypts with the deterministically highest key (`epoch`, then `key_id`) it knows,
  **excluding any generation that is wrapped to a vault the sender knows to be removed**. Without
  this exclusion, two concurrent rotations on the admin's own devices (one of them a removal) could
  make the "highest" key one the removed member still holds.
- **Every membership change creates a new generation**, invites and right changes included, not
  only removals. The member list of a generation is therefore fixed once written, which is what the
  forward-only validity rule in §11 needs: an invitee's writes are valid under the first generation
  that lists them.
- **New members also receive envelopes for the older generations**, so they can read content that
  existed before they joined (the same history decision as haex-vault 2.x ADR 0002).
- Only the scope's admin writes `scope_keys` for spaces and data shares (D6), so concurrent rotations
  can only come from the admin vault's own devices. The vault-internal scope gets a new generation
  whenever a device is removed (D27), wrapped to the remaining devices.
- Envelope format: as specified in the deferred design §2 (canonical JSON `{content_key, epoch,
scope_id}` in NIP-44 v2, envelope bound to scope, epoch and recipient).

### 5.3 Wrap to every device of the member vault (D28)

Grants and member lists name vaults, but envelopes are addressed to **each device** on the member
vault's current device list. A member vault also stores the keys it receives in its own vault data,
so a device linked later gets existing keys through the vault's own sync; later generations are
wrapped to it directly. The admin therefore learns how many devices a member vault has (accepted).
Rotation costs O(devices of the member vaults). Admin actions may come from any device of the admin
vault (D29); the signature is the device's, and it is valid while that device is on the admin
vault's device list.

### 5.4 Authorship

- Every change carries `author_device_npub` and `author_vault_npub` inside the sealed payload; the
  per-column signature is made with the device key over `(scope_id, table, pks, column, hlc,
author_vault_npub, value)`.
- Receivers verify signature → device list (device ∈ vault) → capability of that vault in that
  scope. This is the real `SignatureProvider` for shared scopes; `NoopSignatureProvider` remains
  valid only for the vault-internal scope over an authenticated iroh link.
- `haex-crdt` must stop stamping the scanning device as `device_id` for relayed changes (§3.2). The
  upstream fix belongs in `haexmas/haex-crdt`.
- **`created_by`**: every shareable table (data shares) and every file-index entry (spaces) carries
  an immutable `created_by` vault npub, set on insert by the core and never writable afterwards.
  Receivers reject any change to it. It backs the "delete own entries" rule (§11).

### 5.5 Cursors

- Between peers: every device numbers the changes it originates **without gaps** (a per-origin
  sequence number next to the HLC, which stays for conflict resolution). Progress is, per origin
  device, the highest number up to which all changes are present. A highest-HLC vector is not enough:
  a newer change arriving before an older one over another path would hide the gap. Gaps are
  requested explicitly; a second, different change under an already used (origin, number) is
  rejected as forgery. `ScanFilters.origin_node` supports scanning per origin.
- At the relay: a **server-assigned monotonic sequence number** per mailbox (§10.2). Nostr's
  wall-clock `since` is not used.

## 6. Plane 1: own-device data sync over iroh

- **Scope**: the whole vault except `_no_sync` tables. No permission checks between own devices;
  trust comes from the vault identity.
- **Transport**: an iroh ALPN, working name `holzi-sync/1`. Handshake: both sides present their
  device key and prove possession of it; the peer accepts only devices on its own vault's current
  device list.
- **Exchange**: swap version vectors, stream the missing sealed batches in both directions, apply
  through `apply_remote_changes`.
- **Discovery**: the encrypted presence event (§4). The relay also runs an **iroh relay** for NAT
  traversal, as haex-sync-server already co-hosts one.
- **Direct-only data (D30)**: only the vault identity private key is direct-only. It goes to a new
  main device only by explicit choice over the direct link, and otherwise only inside the encrypted
  recovery package (§10.5). Device keys never leave their device at all. Everything else, including
  S3 credentials in the password manager and unwrapped content keys, is ordinary vault data: it
  syncs, also through the vault's own mailbox, and is recoverable.
- **Atomicity**: a sealed batch is applied all-or-nothing; one invalid change rejects the whole batch,
  so HLC groups stay intact. A compaction snapshot is checked per complete HLC transaction group:
  an invalid change rejects its whole group, other groups are kept. A partial group is never applied.
- **One-device vaults** publish no presence and open no direct links, but their device still listens
  for presence addressed to its vault identity, and they may use relay mailboxes and talk to members
  of spaces and shares. A fresh copy knows the source device from the copied file, so it publishes
  presence; the source device learns of it that way.
- **Via the relay**: the vault-internal scope also has a relay mailbox (§10), encrypted under the vault
  key, so two own devices that are never online at the same time still converge.

## 7. Own-device file sync

Own-device file sync is a space with exactly one member, the vault itself. It uses the same
machinery as §8, so there is no second implementation. A user can mark folders to sync across
devices; the file index lives in the vault-internal scope (not in a scope of its own), so it
travels with ordinary plane-1 sync.

## 8. Plane 2: spaces (network folders for files)

> **Superseded (2026-10-21)** for files by
> [`2026-10-21-file-access-and-sharing-design.md`](./2026-10-21-file-access-and-sharing-design.md)
> and spec 056: files are shared live with single contacts, without spaces, group keys or stored
> ciphertext. Kept here as the basis spec 028 still refers to.

### 8.1 Model

- A space is a file tree plus a member list: `{vault_npub → capabilities}`, written only by the
  admin (D6).
- **File index**: a small CRDT scoped to the space. One entry per file:

  ```
  space_files(space_id, file_id, path, size, mtime, plaintext_blake3,
              object_id, dek, created_by, modified_by, deleted)
  ```

  The entry is encrypted under the space key like every other batch. The file's **DEK lives in the
  entry**; no sidecars are needed on the storage side.

- **Objects**: file content encrypted in chunks under the per-file DEK (the `HXFE` envelope pattern
  from haex-vault 2.x). `object_id` is the hash of the ciphertext, so storage cannot swap content
  undetected. **Objects are immutable**: modifying a file writes a new object and points the entry
  at it; the old object is garbage-collected by an admin device once no index entry references it.
  Storage never sees an overwrite.
- **Mailbox vs. object storage**: a space's mailbox (file index, member list) lives on the admin's
  relay when one is configured, independent of where objects are stored. "Direct transfer only"
  means objects travel only between devices. With no relay at all, the space syncs only while
  members are online together.
- **Direct links between member vaults**: devices of different member vaults may connect directly
  for one scope. The handshake proves a device key that is on the device list of a vault on that
  scope's current member list, and the link carries only that scope's data. Discovery uses presence addressed to
  the scope's member vaults or the relay's signalling.
- **Transfer**: objects travel as iroh-blobs (BAO-verified, resumable) between peers, or through a
  storage backend (§12). A device fetches an object from whichever source has it.

### 8.2 Rights

| Capability | Meaning in a space                                                     | Enforced by                                                   |
| ---------- | ---------------------------------------------------------------------- | ------------------------------------------------------------- |
| `read`     | read all files                                                         | cryptography (holds the space key) + storage gate             |
| `write`    | add files, modify any file, delete **own** files (`created_by` = self) | receivers (signature + capability) + storage gate for uploads |
| `delete`   | delete any file                                                        | receivers + storage gate                                      |

Delete semantics are the proposal in §11 and still need operator confirmation.

### 8.3 Conflicts

Two concurrent modifications of the same `file_id` (neither HLC dominates the other's base) produce a
**conflict copy** (D10): the losing version becomes a new entry `<name> (Konflikt <device> <time>).<ext>`
with its own `file_id`; in spaces the device name is prefixed with the member's display name. The
exact format is plan work. No silent LWW loss of file content.

### 8.4 Membership changes

- **Invite** (D33): the admin creates an **invite link** (space id and name, admin vault identity,
  capability, a 128-bit one-time secret, expiry, relay hints and an admin-vault signature over
  those fields; no keys) and hands it over out of band. The recipient verifies that signature
  before seeing the preview; a forged or modified link is rejected locally and sends nothing.
  Any admin device may initiate link creation; if it is a linked device without the vault private
  key, a main device signs the payload through the own-vault sync path.
  Accepting sends a signed acceptance
  carrying the secret and the invitee's device list to the admin vault as a Nostr DM (NIP-17). An
  admin device drops every message without a valid, still open secret without display, storage or
  reply, so knowing a vault identity buys no way to send invitations. Only after the admin has
  confirmed the invitee's identity does an admin device add `{vault_npub → caps}`, create a new key
  generation (§5.2), wrap it and all older generations to the invitee, and publish the new signed
  member list (§10.3). Pending invitees are never on the list, so the relay never authorizes them,
  and a declined or discarded invitation leaves no key behind. Discarding sends nothing; open links
  are bounded by the links the admin created.
- **Change rights / remove**: the admin updates the member list, publishes it with a higher epoch,
  then **rotates the space key** and wraps it to the remaining members.
- **What revocation means**: the storage gate refuses immediately; new files are unreadable to the
  removed member; files it already had keys for stay readable to it. Optional full re-encryption is
  a later feature.

## 9. Plane 3: data shares (SQLite, per user)

### 9.1 Shareable declaration (uniform schema)

Extensions declare what can be shared; the core computes and enforces everything else. The extension
never touches sync, keys or ACLs.

```yaml
# extension manifest
shareable:
  - type: calendar.calendar # a collection
    root: { table: calendars, pk: id }
    label: name # shown in the generic share dialog
    includes:
      - { table: events, fk: calendar_id, as: calendar.event }
  - type: calendar.event # a single entry
    root: { table: events, pk: id }
    label: title
    includes:
      - { table: attendees, fk: event_id }
      - { table: reminders, fk: event_id }
```

Table names above are logical; the core resolves them to the extension's prefixed physical tables.

**Validation at install and update (D5)** — the extension is rejected, not just the declaration, if:

- any `root` or `includes` table lies outside the extension's own prefix
  `{public_key}__{extension_name}__`;
- any FK edge points outside that prefix (no reaching into core tables or other extensions' tables);
- a table is not CRDT-tracked, or is a `_no_sync` table.

### 9.2 Share model

```
shares(share_id, owner_vault, type, root_pks, created_at)
share_grants(share_id, grantee_vault, capabilities, epoch)   -- written only by the owner (D6)
scope_keys(...)                                              -- §5.2, per share
```

- **Membership of a row in a share** is derived: the root row plus every row reachable through the
  declared FK edges. A new event in a shared calendar is automatically part of the share; there is no
  row register.
- **Sender-side guard**: only declared tables and only rows reachable from the root leave the vault
  for that share (`ScanFilters` with `column_eq` / `row_pks`).
- **Receiver-side guard** (`ApplyPolicy::prepare_row`): an incoming insert or update is admitted only
  if its table belongs to the share type's extension, it is reachable from the share root, and the
  author vault holds the needed capability (§11). Deletes are checked against the capability rules
  only. When the owner moves an entry out of the shared root, recipients remove their copy without
  sending a delete back, unless another share still covers the entry. A new row from a grantee (an item added to a shared shopping
  list) must carry an FK into the share.
- **Where received data lands**: in the recipient's copy of the same extension tables, tagged with
  the originating `share_id` in a core-owned mapping. It then syncs to the recipient's own devices
  through plane 1.
- **The ACL has a single writer** (the owner vault). Concurrent ACL edits can only come from the
  owner's own devices and resolve by ordinary LWW. No concurrent admins, no forks.

### 9.3 Overlapping shares

With D4, one row can be inside two shares (an event shared individually and through its calendar). A
write is validated against the share it arrived through. Forwarding it into the other share is done
by the owner vault, so it is delayed until an owner device is online. Accepted for v1. The owner
re-issues the forwarded change as a new change signed by an owner device in the second share's
scope; its original author may not be a member there. The original author is kept for display only,
and the row's `created_by` is unchanged.

## 10. Blind relay ("Holzi Relay")

### 10.1 Role

A headless, multi-tenant server. It is **untrusted** (D11): it can delete or withhold, but it can
neither read nor forge. It is not a peer: no vault, no passphrase, no apply.

It bundles three services:

1. **Mailboxes** for sealed batches of **SQLite data only**, one per scope (vault-internal, space,
   data share). Served over the same iroh ALPN family, so the relay is a "blind peer" that speaks
   the sync protocol but only stores and forwards. Files never go into a mailbox (D32).
2. An **iroh relay** for NAT traversal.
3. Optionally a **Nostr relay** for presence, invites and signaling.
4. Optionally **storage backend A**: S3-compatible storage run by the relay operator for space
   files. The relay service checks access and streams each encrypted object from that storage
   (D24); every object exists once. With the user's own S3 (backend B) the relay is not involved in
   files.
5. Optionally **recovery packages** (§10.5).

### 10.2 Mailboxes

- Append-only; each accepted batch gets a monotonic sequence number; clients pull `after <seq>`.
- **No server-side merge.** Table names, columns, PKs and HLCs stay inside the ciphertext — the
  opposite of haex-sync-server's per-cell rows.
- **Compaction is client-driven**: the scope's admin (any own device for the vault scope, D21) uploads an encrypted snapshot of the scope
  at a sequence number; the relay then drops older batches. The snapshot carries the original
  per-column signatures (haex-crdt keeps them in `haex_column_sigs`), so a snapshot writer cannot
  forge other members' data. Receivers verify snapshots exactly like batches.
- No accounts, no email. An identity is a key. Quota is per vault npub; admission is by operator
  invite code in v1, with payment (Cashu/Lightning) as a later option.

### 10.3 Authorization at the relay: signed member lists, not UCAN

The relay must decide who may read from and write to a mailbox. It does this with a **signed member
list** that the admin uploads (the deferred design's §4 projection):

```
MemberList { scope_id, epoch, grants: [{vault_npub, caps}], issued_at, expires_at, admin_sig }
```

- A request authenticates with the **device key** (NIP-42 challenge). The relay keeps the latest
  device list of every vault it serves and checks the admin-device signature, that the device is on
  the device list of a vault in the current member list, and that the capability fits (`read` for pulls, `write` for pushes).
- A higher epoch replaces a lower one; missing, expired or invalid lists fail closed.
- **Same-generation lists**: two admin devices can publish different valid lists with the same
  generation. Relay and receivers apply one rule: the list with the lexicographically smallest hash
  wins, and the relay replaces a stored list of the same generation only by one with a smaller hash.
  An admin device that sees such a conflict publishes generation + 1, merging both edits.
- **Revocation is immediate**: the admin uploads a new list. No revocation list is needed, unlike
  UCAN tokens that stay valid until expiry.
- **Why not UCAN**: UCAN's value is verifiable _delegation chains_. With D6/D7 every chain has exactly
  one link (admin → member), and a one-link UCAN is a signed grant. Holding the current list at the
  relay gives immediate revocation, and it uses the same secp256k1 keys as Nostr instead of a second
  `did:key` Ed25519 key system. Functionally it is still a capability system.
- **The relay gate is coarse and only defense in depth.** It can only check read vs. write. The fine
  rules (`created_by`, delete-own, FK into share) are enforced by every receiving client after
  decryption, so a relay that lets something through still cannot get it applied.

### 10.4 What the relay sees (D9)

Sees: IP addresses, timing, sizes, scope ids, the member list per scope (vault npubs + capabilities),
device npubs through the device lists. Does not see: any content, table or column names, PKs, HLCs, file
names, file sizes beyond ciphertext length. Batches may be padded to size buckets. Per-scope
pseudonyms are a post-v1 option.

### 10.5 Recovery package (D31)

Optional and opt-in. holzi generates a high-entropy **recovery key**, shows it once as code or QR,
and the user keeps it offline. From it the device derives an encryption key for the **recovery
package** and an authentication keypair. The relay stores only the public authentication key (and a
lookup id derived from it), the encrypted package and the second-factor setup. It never sees the
recovery key, the encryption key or the private authentication key.

- Content: the vault identity private key, the vault's content-key generations, the list of relays.
  A main device keeps it current (re-encrypts and uploads on every new generation).
- Retrieval needs both a **possession proof** (the device signs a relay challenge with the private
  authentication key; nothing secret is sent) and a **second factor**: TOTP by default, or a link to
  a stored e-mail address, which the user opts into knowing the relay then holds that address (the
  only exception to D9). Rate limit and lockout after failed attempts.
- Decryption happens only on the device. holzi then restores the vault from its mailbox (snapshot
  plus batches); the user sets a new passphrase; the restored instance is a main device and
  publishes a new device list.
- Storing the recovery key encrypted with another person is a later spec and always user-initiated.

## 11. Capabilities

| Capability | Data share                                        | Space                                            |
| ---------- | ------------------------------------------------- | ------------------------------------------------ |
| `read`     | read all rows of the share                        | read all files                                   |
| `write`    | create rows, modify any row, delete **own** rows  | add files, modify any file, delete **own** files |
| `delete`   | delete any row of the share                       | delete any file                                  |
| admin      | implicit for the creator only; not grantable (D6) | same                                             |

- "Own" means `created_by` equals the author's vault (§5.4).
- The levels are nested: `read` ⊂ `write` ⊂ `delete`, identically for spaces and data shares.
- **Where it is managed**: the settings category „Föderation“ (spec 023) gets the sub-views
  „Geräte“, „Relays“, „Ordner“, „Spaces“ and „Datenfreigaben“; a space's storage backend is set in
  its detail view.
- **Decided 2026-09-28 (D18)**: this three-flag split (delete-own inside `write`,
  separate `delete`). The operator named three options on 2026-09-27; this is the recommended one.
- **Revocation vs. concurrent writes — decided 2026-09-28 (D19)**: a change encrypted under key epoch _e_ is
  valid if its author vault held the needed capability in the member list of epoch _e_. Revocation
  therefore acts forward only, bounded by the cutoff below; a write made concurrently with the
  removal survives only on devices that applied it before they learned of the removal. The alternative, keeping a batch log per scope and recomputing affected cells on
  membership change, is correct but much heavier, and was not chosen.
- **Revocation cutoff**: a member list that removes a vault or lowers its rights carries, for each
  device of that vault, the highest per-origin sequence number (§5.5) the admin had applied. Every
  device that knows the list rejects the affected vault's changes beyond that cutoff that need the
  withdrawn right, whatever their timestamp. Backdating does not help, because the numbers below the
  cutoff are already taken. The relay gate stops new uploads at once.
- **Accepted window (D19)**: a change a device applied before it learned of the revocation stays
  until the next authorized write to the same cell overwrites it. The guarantee is therefore "every
  device that knows the revocation rejects everything beyond the cutoff", not "no post-revocation
  change is ever visible anywhere".

## 12. Storage backends for spaces (D12)

The client side is identical for both backends: same immutable, opaque, encrypted objects addressed
by ciphertext hash from a signed file index. Only the way a device obtains access differs.

### A. Storage provided by the public relay

- The relay operator runs the storage (S3-compatible). The relay _is_ the storage provider, so its
  access is unavoidable and harmless: it holds only ciphertext, and forgery is detected.
- Access (D24): the relay transfers the encrypted objects itself between its storage and the
  requesting device, over its own endpoint and the same protocol family as the mailboxes. There are
  no presigned URLs. The relay checks the current member list on every request, so a removal takes
  effect at once, running transfers included. It still sees only ciphertext; file bandwidth goes
  through the operator's server.
- Delete: the relay records the uploading vault per object and allows `DELETE` for the uploader or a
  `delete` holder.

### B. The user's own S3

- **The relay never receives the credentials** (D11).
- **One bucket per space**, with **two scoped tokens**: read-only and read-write. The admin creates
  them in their vault and wraps them with NIP-44 to each member vault separately according to
  capability. They are not merely encrypted under the space key, since then every reader could
  decrypt the read-write token.
- Devices access S3 directly with those tokens; the relay is not involved for files.
- Revocation: in one operation the admin device first revokes or rotates the affected token at the
  provider, then publishes the new member list, redistributes the token and rotates the space key.
  If the provider revocation fails, the list is still published, the admin sees a persistent
  warning, and holzi retries until the provider confirms. The setup check requires the provider to
  reject a revoked token within 5 minutes.
- Members never delete objects themselves; garbage collection runs on an admin device with the
  admin credentials.
- **Limit**: S3 cannot enforce "delete own files only"; a read-write token can delete any object.
  File-index tombstones remain client-checked. Physically deleted objects are recovered through
  **bucket versioning**, which the setup flow MUST enable.
- RustFS and AWS S3 are the v1 providers from D20 and have been verified and tested against the
  setup criteria. The remaining candidates named in the design session — Cloudflare R2, Backblaze
  B2 and Hetzner Object Storage — are not v1 providers until that verification is done. Recovery
  also needs a read-write token that cannot delete old versions or disable versioning, which narrows
  the list further. A provider that fails any criterion of the setup check cannot be connected; there
  is no fallback path (D25).

## 13. Threat model

| Property                 | Secured by                                                                                                   | Relay / storage can                                                                                  |
| ------------------------ | ------------------------------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------- |
| Confidentiality          | encryption; keys only in members' vaults                                                                     | nothing                                                                                              |
| Integrity / authenticity | per-chunk AEAD, ciphertext-hash object ids, per-column device signatures, device lists, signed member lists  | nothing — forgery is rejected by receivers                                                           |
| Unsolicited invitations  | no invitation without an admin-created link; messages without a valid open secret are dropped silently (D33) | n/a (cannot read messages)                                                                           |
| Authorization            | receivers enforce all rules; relay gate is additional                                                        | let an unauthorized request through (still not applied) or refuse a valid one                        |
| Availability             | redundancy: direct iroh sync, several own devices, snapshots                                                 | delete, withhold, serve stale state — detected by version vectors and sequence gaps, not preventable |
| Removed member           | key rotation, relay gate, token rotation                                                                     | — ; the removed member keeps what it could already decrypt (accepted, as in haex-vault 2.x ADR 0002) |
| Stolen device            | SQLCipher passphrase; removal from the device list (D27)                                                     | —                                                                                                    |

## 14. Proposed spec cut

Spec numbers 017–021 are reserved for agent control; the next free numbers start at 024.

| Spec | Scope                                                                                                                                                                                                      | Depends on                                 |
| ---- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------ |
| 024  | Real secp256k1 vault identity and device keys, main and linked devices, device list, linking, iroh endpoint, presence via Nostr, `holzi-sync/1` between own devices (plane 1), authorship fix in haex-crdt | —                                          |
| 025  | Removed; replaced by spec 061 (sync rules). Was: own-device file sync, file index, objects, conflict copies                                                                                                | 024                                        |
| 026  | Blind relay: mailboxes, sequence cursors, client snapshots, signed member lists, iroh relay, storage backend A                                                                                             | 024                                        |
| 027  | Spaces: member lists, invites, capabilities, key epochs, revocation, rotation                                                                                                                              | 025, 026                                   |
| 028  | Data shares: shareable declaration schema, prefix validation, closure computation, sender/receiver guards                                                                                                  | 024, 026, extension table storage in holzi |
| 029  | Storage backend B: own S3, scoped tokens, versioning                                                                                                                                                       | 027, 030                                   |
| 034  | Password manager: integral secret store (e.g. S3 credentials), usable by extensions with permission, synced and recoverable like other vault data                                                          | 024                                        |

Structural items that belong in 024 even though only the vault-internal scope exists then: the real
`SignatureProvider` seam, the sealed-batch format with `scope_id`/`key_id`, the allow-list that keeps
vault secrets on plane 1, and `created_by` support in the core.

## 15. Open questions

1. ~~**Delete semantics**~~ — decided (D18).
2. ~~**Revocation vs. concurrent writes**~~ — decided (D19).
3. **Admin loss**: if the admin vault is lost entirely, the space or share is frozen (content stays,
   membership cannot change). Is a "transfer admin" feature needed, and when?
4. ~~**Vault identity rotation flow**~~ — dropped: no rotation in v1 (D26).
5. ~~**Snapshot authority**~~ — decided (D21).
6. **Relay discovery and trust configuration**: how a vault picks relays. Several relays per scope
   are not v1 (D17).
7. **Wire formats and event kinds**: sealed batch, member list, device list, presence, invite DM, recovery package.
8. **Rotation cost**: O(devices of the member vaults) per membership change — measure before large spaces appear.
9. **Mobile**: plane 1 while the app is foreground-only (v1-scope availability classes).
10. **Extension storage in holzi**: data shares (028) need extension-owned, prefixed CRDT tables,
    which holzi does not have yet.
11. ~~**Existing vault copies**~~ — decided (D14).
12. ~~**Presence and NAT servers before the own relay**~~ — decided (D15).
13. **Additional S3 providers**: RustFS and AWS S3 are the verified v1 providers (D20); whether
    Cloudflare R2, Backblaze B2 or Hetzner Object Storage meet the same criteria remains to verify.
