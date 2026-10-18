# Cross-User Sharing — Deferred Design

**Status**: Deferred. Written 2026-09-07 from a design session. **Not v1 scope.** Captured so the
conclusions are not lost and so the closed-federation work can avoid choices that would block this
later.

**Superseded in part** by [`2026-09-28-sync-architecture.md`](./2026-09-28-sync-architecture.md):
rows are no longer scoped into spaces (spaces carry files only; data is shared per user), the
separate federation identity is replaced by the vault identity, and the object-storage IAM sidecars
are replaced. That document lists what is kept from here.

**Relationship to existing documents**:

- [`docs/design/founding.md`](../design/founding.md) §6 sketched cross-user sharing as a future
  direction (`access.grant` / `access.revoke`, a guest ingress track). This document supersedes that
  sketch where the two differ, and keeps §6's framing everywhere else.
- [`docs/plans/2026-09-04-v1-scope-design.md`](./2026-09-04-v1-scope-design.md) puts cross-user
  sharing post-v1. That line is unchanged. §4 of that document also predicted that a stable
  federation-scope identity would resurface here; it did, and it is this document's largest open
  question (§8).
- Source material is `haex-vault 2.x (Legacy)`: ADR 0002 (shared-space authenticity and confidentiality),
  `CONTEXT.md`, and `docs/plans/2026-08-27-phase4-sharing-content-addressable-iam.md`.

**Sequencing decision**: closed federation first (one operator, multiple paired instances, data and
files). Open federation — sharing across operators — comes after, on top of whatever the closed
federation established.

---

## 1. The question this answers

`haex-vault 2.x` implements shared spaces with MLS (group key agreement) and UCAN (capability
delegation chains). Can holzi have cross-user sharing without adopting either stack?

**Yes.** What MLS and UCAN provide is replaceable with primitives holzi already needs for other
reasons. What is _not_ replaceable is the concepts: there is still a capability system and there is
still group key management, just in a smaller, purpose-built form.

## 2. What is being dropped, and what it costs

**UCAN provides** signed delegation chains (`Read` / `Write` / `Invite` / `Admin`, orthogonal, each
with a `delegatable` flag), verifiable locally against a self-certifying space root, independent of
any leader.

**Replaced by** signed grant and member records living in the space's own CRDT state, carrying
capability flags, merged with the monotonic epoch mechanic already designed for `peer_instances` in
v1-scope §4. Same trust property, no UCAN serialization, no general delegation graph.

**Membership authority and validation.** The space creator's signed genesis record establishes the
first member and its `admin` capability. Afterwards, only a current member whose record carries
`admin` at the space's current epoch may create a grant or revoke a member. Every mutation carries
the `space_id`, subject federation npub, capability set (for a grant), pre-mutation epoch, signer
instance pubkey and a signature over that canonical payload. The receiver verifies the instance
signature and its binding to the signer's current member record before accepting the mutation. A
grant's capabilities MUST be a subset of the signer's current capabilities; an admin cannot grant
itself or another member a capability it does not hold. A revocation cannot grant capabilities and
must name the target and the resulting epoch. An accepted revocation advances the space epoch and
rotates the content key, so a same-epoch or stale grant cannot restore the revoked member. A
receiver rejects an invalid signature, unknown or non-admin signer, stale or future pre-mutation
epoch, or capability escalation before the record enters the CRDT merge; it does not rely on a
later apply-time check to repair an unauthorized merge.

**Cost**: no arbitrary-depth re-delegation. The known cases need none — a grant names a federation
identity (§7), which spans that person's instances without any chain at all. Onward sharing between
_people_ is a separate question (§8 item 4); if it ever needs real depth, that is the point at which
UCAN becomes worth reconsidering.

**MLS provides** epoch keys derived from group state, rotating on membership change, with forward
secrecy and post-compromise security, scaling at O(log N).

**Replaced by** a symmetric 32-byte content key per space epoch, wrapped to each member with
**NIP-44 version 2**. The wrapping uses secp256k1 ECDH (the unhashed 32-byte x-coordinate),
HKDF-SHA256, ChaCha20 and HMAC-SHA256; it is not ChaCha20-Poly1305. Rotation on membership change
means minting a fresh key and re-wrapping to the remaining members.

The plaintext passed to NIP-44 v2 is canonical UTF-8 JSON with exactly these fields:

```json
{
  "content_key": "<base64 of exactly 32 bytes>",
  "epoch": 42,
  "space_id": "<canonical space id>"
}
```

For each member, the wire envelope is canonical UTF-8 JSON with the following fields:
`alg` (`"nip44-v2"`), `space_id`, `epoch`, `sender_pubkey` and `recipient_pubkey` (64 lowercase
hex characters containing the secp256k1 x-only public keys), and `payload`. `payload` is the
standard padded Base64 encoding of the NIP-44 v2 bytes
`version (0x02) || nonce (32 bytes) || ciphertext || mac (32 bytes)`. The sender derives the
conversation key from its private key and the recipient public key; the recipient selects the
matching envelope by `recipient_pubkey` and derives it from the inverse key pair. The member record
signs the canonical envelope together with the subject and epoch, preventing an envelope from
being swapped between spaces, epochs or recipients. NIP-44's random nonce is fresh for every
envelope, and its authenticated payload is verified before the key payload is parsed.

**Cost**: O(N) re-wrap per membership change instead of O(log N), and no post-compromise security.
Note that ADR 0002 already scopes backward secrecy out — a removed member keeps old epoch keys and
can still read historical ciphertext — so the delta against the MLS design is narrower than it
first appears.

**Escape hatch**: NIP-EE is MLS over Nostr. If group scale or PCS later matter, the ecosystem path
exists without rearchitecting the surrounding model.

## 3. Two classes of relay

`founding.md` binds relay, device, and Tauri application one-to-one, and spec 001 refines this to
"the _active_ SQLite instance is the running relay endpoint is the running iroh peer". Sharing adds
a second deployment shape that is **not** a peer:

- **Peer relay** — inside the Tauri application, inside the SQLCipher boundary. Holds the
  passphrase, sees plaintext, has an identity in `peer_instances`, performs CRDT apply, issues and
  honours grants.
- **Buffer relay** — headless, on a server. No passphrase, no instance identity, no apply. Stores
  ciphertext and forwards it. The operator of that machine learns no content.

The one-to-one rule survives, restricted to peers. Buffer relays are not peers.

Nostr gives integrity here for free: every event carries a Schnorr signature over its hash by its
author, so a buffer relay that alters content produces an event every receiver rejects. What
encryption does _not_ prevent is **withholding** — a buffer relay can serve stale state. The
mitigation is the same one `haex-vault 2.x` uses: HLC cursors and compaction anchors.

## 4. Data plane

**Decision: the CRDT delta stream does not travel as Nostr events.**

v1-scope §5 states the three channels are disjoint — "Nostr never carries CRDT deltas". That holds
for shared spaces too. The rejected alternative was mapping each CRDT transaction group onto an
encrypted Nostr event tagged with `space_id`. It is attractive (one transport, one auth model, and
the event signature would replace `haex_column_sigs` for free) but fails on three counts:

1. **Compaction.** Nostr events are immutable and NIP-09 deletion is advisory. A long-lived space
   accumulates millions of column changes with no reliable way to prune. `haex-vault 2.x` needed
   compaction anchors for exactly this reason.
2. **Event size.** Relays cap event size; `haex-vault 2.x` ADR 0001 already deals with maximum CRDT
   transaction size.
3. **Cursors.** Nostr `since` filters are wall-clock; HLC is not.

**Instead**: the `haex-crdt` sync channel gains a space mode. On a server, the sync endpoint runs
alongside the buffer relay, and the roles split cleanly:

- The **relay authenticates** — NIP-42 challenge/response establishes which pubkey is asking.
- The **grant records authorize** — signed membership with capability flags and epoch.
- The **sync endpoint transports** — ciphertext deltas, with HLC cursors and compaction anchors.

Per-change authenticity comes from the `SignatureProvider` trait the extraction plan already
defines. For a shared space, the real provider's `AuthorId` is the signing instance's Nostr
pubkey, and its verification context contains the NIP-42-authenticated `nostr_pubkey`, the space's
current epoch and the federation-signed instance attestations. Before calling
`haex-crdt`'s apply pipeline, the provider MUST require every change author to equal the
NIP-42-authenticated pubkey or to be bound to it by a valid federation-signed attestation for the
current epoch. The attestation must also bind that instance to the member federation identity and
its current capabilities. `on_before_apply` rejects an identity mismatch, stale/invalid
attestation or wrong-space context, and the all-or-nothing apply then writes nothing. This binding
prevents a relay that authenticated one instance from accepting a valid signature from another.

holzi uses `NoopSignatureProvider` inside the closed federation, where an authenticated iroh
transport carries the trust; it remains valid there because the provider's documented no-op
precondition is that the transport is already authenticated. `NoopSignatureProvider` is not used
for shared-space ingress. This satisfies ADR 0002's requirement that authorization be verifiable
locally and independently of any leader.

**Read gating is enforced, not blind.** The endpoint checks the authenticated pubkey against a
per-space member list rather than serving anyone who knows a `space_id`. Rationale is requirement 3
of the content-addressable IAM plan: enforcement must exist at the storage layer, not only at the
encryption layer, or a removed member can keep listing and hoarding ciphertext against a future key
compromise. The price is that the operator of a buffer relay learns the participant set per space —
the same metadata trade `haex-vault 2.x`'s sync server already makes through whitelist filtering.

The list is a signed **relay-readable authorization projection**, not a second CRDT. After a peer
relay has committed a valid membership merge, an instance holding `sharing-authority` publishes a
projection containing `space_id`, the resulting `epoch`, the canonical list of read-authorized
instance `nostr_pubkey` values, explicit revocation tombstones, a `membership_digest`, `issued_at`,
`expires_at`, and the federation signature. The digest is computed over the canonical member
records and is also carried by the encrypted CRDT membership state. On a grant or revocation, the
peer first commits the membership mutation, then publishes the projection; a revocation publishes
a higher-epoch tombstone before the peer sends new deltas. The federation signer refuses to publish
a projection unless its digest matches the merged CRDT membership, so the projection cannot silently
drift from encrypted state.

The buffer relay verifies only the federation signature, `space_id`, digest, and monotonic epoch;
it stores the latest projection without decrypting or applying CRDT state. It accepts a read only if
the authenticated NIP-42 pubkey is in the projection for that space, the projection epoch is at
least the client's requested minimum epoch, and `issued_at <= now < expires_at` within a bounded
maximum projection lifetime. A higher-epoch projection supersedes every lower-epoch projection;
same-epoch conflicting projections are rejected. Missing, expired, invalid, or revoked projections
fail closed, so delayed publication cannot authorize reads indefinitely. Every member still
re-verifies the signed membership, epoch, digest and `SignatureProvider` identity binding on apply;
the buffer projection is a storage-layer gate, never an authority that can be bypassed or used to
grant access.

## 5. File plane

Files never live in the SQLite database. Two **separate entities**, not two transports for one
artifact — a user entitled to both chooses where to fetch from:

- **Local sharing over iroh.** The file exists only on the owner's device. Served as `blob.offer`
  plus a peer-bound ticket, per `founding.md`. **Not encrypted at the application layer** — iroh's
  transport encryption plus the ticket check carry it, because the recipient is authorized and no
  third party holds the bytes. No third party, no storage cost, available only while the owner is
  online.
- **Encrypted object storage over HTTPS.** External storage is untrusted, so content is encrypted.
  Always available, costs storage.

**Revocation differs between the two, and the UI must show it.** Locally it is immediate and total:
the accept handler refuses, and nothing but what was already fetched exists elsewhere. On object
storage it is bounded by key rotation and IAM, and works forward only.

For the object-storage path, `haex-vault 2.x`'s content-addressable IAM design ports nearly unchanged:

- Content at `content/o/<random-hex>`, encrypted under a fresh per-object DEK.
- A grant is a ~200-byte sidecar under a per-space prefix, carrying the DEK wrapped under the
  grant's KEK, plus the pointer, size, name and plaintext hash.
- IAM scopes each principal to LIST + GET within its own grant prefix, plus bucket-wide GET without
  LIST on the content prefix. Without a sidecar, an object cannot even be enumerated.
- The KEK/DEK split makes multi-space grants free: one physical object, N sidecars.

**The only change holzi needs** is the KEK source. `haex-vault 2.x` resolves it from the MLS group
state (`FileKeySource::SpaceEpoch`); holzi resolves it from the NIP-44-wrapped space epoch key of
§2. Everything else — envelope format, sidecars, opaque object keys, IAM policy shape — is
transport- and identity-agnostic.

## 6. Scoping: what a member can actually see

Membership answers _whether_ a pubkey may subscribe to a space. Three further gates answer _what_
is in it, and all of them apply **at the sender, before encryption**. A member never queries the
owner's database; they receive a stream from which everything else was already excluded.

1. **Table whitelist.** A canonical list of tables permitted to cross a space channel at all
   (`SPACE_SCOPED_CRDT_TABLES` in `haex-vault 2.x`, Rust as source of truth). Vault-private tables never
   leave the device regardless of any grant. This is the structural guard against exfiltration.
2. **Row register.** Within a whitelisted table, only rows explicitly registered into the space are
   emitted — an M:N mapping, a push declaration by the owner ("row R belongs to space X").
3. **Per file.** One sidecar per content object, or one `blob.offer` per file bound to one
   recipient and one hash.

Capability kind per member (read, write, invite, admin) is a flag on the member record, following
the pattern `pairing-authority` and `confirmation-authority` already establish in v1-scope §4.

## 7. Federation identity

**Decided 2026-09-07.** A grant is written against a key, but a person has several instances, and
v1 deliberately has no identity spanning them. Sharing introduces one: **a single secp256k1 keypair
per person, the federation identity.**

It is a native Nostr identity — presentable as an npub and usable directly in NIP-42 and NIP-44 —
not a new format.

- **What it signs**: statements binding instances to the person ("instance P is mine"), and their
  revocations. It does not sign daily traffic; instances keep signing with their own keys.
- **Where it lives**: a row in the encrypted database, replicated by `haex-crdt` only to instances
  holding a `sharing-authority` capability, wrapped to those instances' pubkeys with NIP-44. The
  temporary install on an untrusted host from v1-scope §12 never receives it.
- **How a grant resolves**: the granter names the recipient's federation npub. The sync endpoint's
  read gate (§4) admits an instance pubkey when a valid federation-signed attestation binds it to a
  member npub at the current epoch. The recipient adds or retires devices by publishing further
  attestations and revocations under the same key; the granter never has to act.

**DIDs were considered and rejected.** In `haex-vault 2.x` they are load-bearing because UCAN's issuer
and audience fields _are_ DIDs. Without UCAN they stop paying for themselves: a `did:key` over the
same secp256k1 key is the npub with an extra encoding to keep consistent, and the customary Ed25519
`did:key` would introduce a second keypair that NIP-42 and NIP-44 cannot verify natively — working
against the decision that Nostr carries authentication and authorization. Note also that
`haex-vault 2.x`'s owner DID spans devices because a device-attestation layer sits beneath it, not
because it is a DID; the spanning comes from the attestations, which is exactly what is kept here.

**Why this is not the federation root v1-scope §4 removed.** That decision on 2026-09-06 turned on
the paper seed being false recovery — a paper key restores signing ability, not data. This key is
data: it lives in the encrypted database and replicates with everything else, so recovery is a
surviving paired instance or a `.db` backup, which is v1's recovery story already. No paper seed, no
hardware token, no NIP-46, and therefore no custody question.

**Costs.**

- The key exists on every `sharing-authority` instance, so its compromise radius is wider than a
  single instance key. The response has the shape of v1-scope §4's federation-wide reset — roll the
  federation key, re-attest instances — with one part v1's reset does not have to solve: every
  counterparty holding a grant must learn the new npub out of band.
- One more key type to manage, with its own rotation story.

This supersedes an earlier proposal in this document for a depth-one delegation chain, in which the
granter invited one instance and that instance named its siblings. The federation identity does the
same work without leaking the recipient's device count to the granter, and without a representative
instance whose loss forces a re-invite.

## 8. Open questions

1. **Federation-identity mechanics.** The identity itself is decided (§7); three sub-questions are
   not. Where instance attestations are published so a counterparty can fetch them — in the space,
   as Nostr events under the federation npub, or both. Their lifetime and renewal cadence, since
   `founding.md` §2's short-lived attestations assumed a master key that was present to renew them.
   And how a counterparty learns a rotated federation npub, which is the one recovery path that
   cannot be solved inside the encrypted database.
2. **Metadata exposure at the buffer relay.** §4 accepts that the operator learns the participant
   set. Revisit if that is too high a price.
3. **Compaction across a multi-operator space.** `haex-vault 2.x` elects a leader per mode; who advances
   the anchor when the space spans two federations is unresolved.
4. **Re-delegation.** Whether a member may share onward to a third person at all, and if so under
   what limit. Nothing in §7 provides it: a federation identity spans one person's instances, not a
   chain between people.
5. **Rotation cost.** O(N) re-wrap per membership change bounds practical space size. The bound
   should be measured before it becomes a surprise.
6. **Event kinds and wire formats** for grants, revocations, key envelopes and invitations. All
   spec-phase work, as founding.md §4 item 1 already records.

## 9. What this means for the closed federation

Nothing here blocks the closed-federation work, but three things are worth keeping in view while
building it:

- The `SignatureProvider` seam must stay real. Starting with `NoopSignatureProvider` is fine; wiring
  around the trait is not.
- The table whitelist and row register are the exfiltration guard. Introducing them late means
  auditing every table that already exists.
- Capability flags on peer records are the same mechanism sharing needs. Keeping them general costs
  nothing now.

One live tension: this session's stated priority was a closed federation in which a person exchanges
**data and files** between their own devices. v1-scope currently places `blob.offer` post-v1 and
refuses oversized cross-device MCP results rather than falling back to blobs. "Fetch my photos from
my phone" needs blob transfer. Either that v1 line moves, or the closed federation ships without
files. That decision belongs in the v1 scope document, not here.

## 10. Addendum (2026-09-17): concrete drivers from a follow-up session

Written from a session that started by comparing holzi to OpenClaw 2.0 and to
[`haex-space/haex-sync-server`](https://github.com/haex-space/haex-sync-server). No architectural
decision here changes; this section adds concrete use cases and sharpens one open question already
listed in §8.

**Buffer relay, concretely — "Holzi Relay".** The operator's stated target: a multi-tenant,
self-hosted sync server where multiple unrelated users' vaults sync through one deployment, with
none able to read another's data, including the operator of that deployment. This is exactly the
buffer relay of §3. Useful sibling reference: `haex-sync-server` provides a concrete implementation
of the general client-encrypted relay shape for HaexHub on a different stack (Supabase/Postgres
policies and authenticated API routes rather than holzi's NIP-42 plus signed membership
projections). The server stores ciphertext and encrypted key material, but does not hold the
decryption key. This is evidence that the general relay shape is implementable, not validation of
holzi's different Nostr authorization model. The Nostr-based mechanism in §3-§4 is holzi's own
design, not a port of that implementation.

**The metadata caveat needs to be stated plainly, not just listed as open.** §8 item 2 already flags
"metadata exposure at the buffer relay" as open, but softly. Restated directly because a plain
reading of "nobody can read another user's data, not even the operator" oversells the design as it
stands: that guarantee holds for _content_ — ciphertext stays ciphertext. It does not hold for _the
participant set_ — the buffer relay operator sees which pubkeys sync into which space, the same
kind of trade `haex-sync-server` makes (its operator can see which `user_id` owns which encrypted
rows, among other sync metadata, even without their content). True metadata-level unlinkability —
hiding who talks to whom, not just what they say — is a materially harder problem
(mixnet/onion-routing class) and is not something this design attempts without a specific driver
for it.

**Cross-user sharing, concretely — calendar and todo/shopping-list entries.** The row-register
mechanism in §6 item 2 was designed abstractly ("row R belongs to space X"); the concrete driver
surfaced in this session is sharing individual calendar entries or todo/shopping-list items with
another operator, with read or write rights, without exposing the rest of that table. This is
what the table whitelist plus row register can provide for inclusion granularity; the read/write
rights remain capabilities on the space member record, not per-row ACLs. No design change is
proposed here, just confirmation that the granularity can match the use case. Two things worth
flagging against this specific driver:

- **Sequencing dependency, not a design gap.** holzi has no calendar, todo, or shopping-list
  application yet. The current application scope covers chat/models/providers and the surrounding
  Tauri, relay, iroh, MCP, and device-lifecycle surfaces, but none of those modules provides rows
  for these use cases. The sharing mechanism is table-agnostic — a future app module would need to
  add its tables to Holzi's eventual equivalent of `SPACE_SCOPED_CRDT_TABLES`, implement the
  sender-side filtering, and build a row-registration UI. There is nothing to register until those
  app modules exist. Not a blocker to this design; a build-order note.
- **Re-delegation (§8 item 4) is the open question this driver makes concrete.** A shared family
  calendar plausibly wants re-sharing (share with a partner, partner re-shares with the kids); a
  shared todo list with a coworker plausibly does not. §7's federation identity, as designed,
  provides none — a grant reaches one person's own devices, never a chain to a second person. Worth
  deciding in the context of the app that needs it, rather than adding a global answer to this
  deferred design now.
