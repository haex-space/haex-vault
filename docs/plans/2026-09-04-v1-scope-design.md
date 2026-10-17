# Holzi — v1 Scope and Founding Revisions

**Status**: Draft. Written 2026-09-04, same day as `docs/design/founding.md`. Captures scope decisions and revisions made in a scope-sharpening session immediately after the founding capture.

**Relationship to `founding.md`**: this document is normative for v1 scope and for the identity/storage/mobile revisions listed below. Where it and `founding.md` differ, this document wins for v1. Where it is silent, `founding.md` stands. Both documents are pre-spec: field names, event kinds, wire formats, and identity derivations are working proposals, not settled interfaces, until re-expressed in numbered specs (see Section 5 of `founding.md`).

---

## 1. Purpose

The founding document captures the full architectural vision. This document does two things:

- **Draws the v1 line.** Which parts of `founding.md` are in v1, which are post-v1, and why.
- **Revises three founding areas** whose treatment in `founding.md` no longer matches how v1 should work: the identity/pairing model, the storage layer, and the mobile availability model. Each revision is stated here in enough detail to write specs from; deeper detail is spec-phase work.

## 2. v1 scope summary

| Founding element                                                                          | v1 status   | Note                                                                                                                                                                                                                                                                                                                                                         |
| ----------------------------------------------------------------------------------------- | ----------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Tauri application shell (desktop, server, mobile)                                         | **In v1**   | Desktop + server + iOS + Android. Mobile has explicit availability constraints (Section 5).                                                                                                                                                                                                                                                                  |
| Embedded Nostr relay endpoint per device                                                  | **In v1**   | Every registered instance hosts a relay endpoint. Mobile only while foreground.                                                                                                                                                                                                                                                                              |
| iroh peer per device                                                                      | **In v1**   | Every registered instance runs an iroh peer. Mobile only while foreground.                                                                                                                                                                                                                                                                                   |
| Nostr command events (device-to-device)                                                   | **In v1**   | Cross-device command routing over Nostr is v1's core value.                                                                                                                                                                                                                                                                                                  |
| NIP-17 DMs                                                                                | **In v1**   | Second ingress track survives from founding.                                                                                                                                                                                                                                                                                                                 |
| `blob.offer` (iroh content-addressed blob transfer)                                       | **Post-v1** | v1 does not ship blob transfer between devices. `blob.offer` and the ticket-lifecycle spec (`founding.md` §5 item 4) are deferred. Cross-device MCP results that would exceed a size threshold are refused in v1 rather than falling back to blobs. Confirmed post-v1 on 2026-09-07, but explicitly kept in view as the first capability after v1 — see §14. |
| `stream.offer` (real-time voice/video/screen streams)                                     | **Post-v1** | No real-time media in v1. `founding.md` §4 item 11 (stream-session semantics) is entirely post-v1.                                                                                                                                                                                                                                                           |
| MCP server for external clients (host-local)                                              | **In v1**   | IDE/shell/script access is v1. Standard MCP auth per transport. Two auth boundaries to spec (this one plus the cross-device one).                                                                                                                                                                                                                            |
| MCP-to-Nostr adapter (cross-device MCP invocation)                                        | **In v1**   | Follows from cross-device commands + external MCP.                                                                                                                                                                                                                                                                                                           |
| Embedded LLM model runner                                                                 | **In v1**   | Runs on desktop, server, and mobile (small preset on mobile). See Section 6.                                                                                                                                                                                                                                                                                 |
| Provider adapters (Anthropic, OpenAI, OpenRouter, Ollama-HTTP, Generic-OpenAI-compatible) | **In v1**   | Native Rust trait; see Section 6.                                                                                                                                                                                                                                                                                                                            |
| Confirmation-authority signed-release model                                               | **In v1**   | Cross-device confirmation is a killer use case for the mobile-in-v1 decision. Kept as founding describes.                                                                                                                                                                                                                                                    |
| Chat surface in Tauri UI                                                                  | **In v1**   | Primary interaction target.                                                                                                                                                                                                                                                                                                                                  |
| Peer trust records, trust store, epoch-based revocation                                   | **In v1**   | Baseline for any cross-device work. The revised identity model changes _who signs_, not the trust-store shape.                                                                                                                                                                                                                                               |
| Cross-user sharing (`access.grant`/`access.revoke`, guest events)                         | **Post-v1** | `founding.md` §6 future-direction, unchanged.                                                                                                                                                                                                                                                                                                                |

## 3. What "v1 done" looks like

Two of the operator's own devices, paired into one federation. From each device the operator can:

- Open a chat with a local or provider model of their choice.
- Send a routed command to the other device (`@laptop, do X`) and get a result back.
- Have an IDE / shell / script on either device talk to holzi via its host-local MCP server.
- Confirm a `require-confirmation` write action initiated on one device from the other (signed-release-event round-trip).

No file transfer between devices. No voice/video. No cross-user sharing.

## 4. Revised identity and pairing model

**Revision history.**

- 2026-09-04 (initial): introduced a federation-root keypair derived from a paper-seed, master-signed attestation ring, and paper-seed-only recovery. Superseded.
- 2026-09-06 (this revision): removed the federation-root/paper-seed layer as over-engineering for v1's closed-federation scope. Identity is per-SQLite-instance, held inside the encrypted database itself. Recovery is a surviving synced instance or a backup of the `.db` file. Pairing and capability slots kept but simplified to CRDT-synced peer records signed by the granting instance's own key. `founding.md` §2 (Identity model) is fully superseded for v1.
- 2026-09-07 (scope note, **not** a v1 revision): cross-user sharing is designed against a
  federation identity keypair that returns post-v1 — one secp256k1 key per person, replicated as
  data inside the encrypted database to instances holding `sharing-authority`, signing instance
  attestations only. See
  [`2026-09-07-cross-user-sharing-deferred-design.md` §7](./2026-09-07-cross-user-sharing-deferred-design.md).
  **v1's identity model below is unchanged**: instance keys only, no federation-scope key. The
  2026-09-06 objection does not carry over, because it turned on paper-seed recovery being theatre
  and the post-v1 key is not an offline artifact — it recovers from a surviving paired instance or a
  `.db` backup like everything else.

The 2026-09-04 model was rejected because a paper-seed only reconstructs signing capability, not the data that lived in the compromised or lost devices. In a closed federation of one operator's own devices with no cross-user sharing (see `founding.md` §6, still post-v1), no external party references the federation as a stable public identity, so the federation-scope root pubkey buys nothing that a surviving encrypted `.db` does not already provide. The paper-seed layer was inherited from the cross-user-sharing future direction and is deferred with it.

**Revision.**

- **Vault = SQLite = identity.** Each SQLite database is an independent vault. At Genesis, holzi generates a NIP-01-compatible `secp256k1` keypair (the **vault identity**) and stores it in a singleton `vault_identity` row inside the DB. Every replica of the same vault carries this keypair (it comes along with any `cp`); replicas authenticate a copied vault by proof-of-possession of that keypair. The direct **Öffnen** flow therefore needs no attestation, pairing record, or `peer_instances` entry: the copied vault identity is the ingress credential. Per-device Nostr and iroh endpoint keys are generated per replica (see `known_devices` below) and are independent of the vault identity — they change per install; the vault identity does not. Keys never leave the encrypted database.
- **No federation-root, no paper-seed.** There is no persistent federation-scope keypair in v1. "The federation" is the mutual-trust graph formed by CRDT-synced peer records; it has no cryptographic root. The only secret the operator handles explicitly is the SQLCipher passphrase per database — the same one they use to unlock the app. Recovery is by a surviving synced instance or by copying the `.db` to a new install. A receiver verifies a copied-vault connection by checking a challenge signature against its current `vault_identity` public key. A vault-wide identity rotation invalidates every copy carrying the old key; replicas must then be re-established from a snapshot with the new identity. This is the revocation mechanism for direct copied-vault ingress; the separate Verbinden flow uses the peer-registry and pairing rules below. All-devices-lost with no backup means data-lost; that is an accepted risk that the paper-seed did not actually mitigate (it would have restored an empty federation identity).
- **Symmetric pairing.** Any instance whose `peer_instances` record carries the `pairing-authority` capability may pair a new instance. A parent with only `confirmation-authority` cannot pair. Pairing runs by short-lived one-time token (QR code or copy/paste string) issued by the parent instance and scanned by the joiner. The token carries a one-time nonce, an expiry, and a contact hint (Nostr relay URL + parent's `nostr_pubkey`). The joiner opens a Nostr connection to the contact hint and sends a signed canonical pairing transcript naming both parties' `nostr_pubkey`, `iroh_node_id`, requested alias, and the token nonce. The parent verifies the transcript, that the token is unused and unexpired, that its own record still carries `pairing-authority` at the current federation epoch, and that the granted capability subset is a subset of what the parent holds. Both instances then write a mutual `peer_instances` CRDT record naming the other, signed by the writer's own instance keypair, with the capabilities the parent explicitly granted.
- **CRDT-synced peer registry.** The federation state (peer_instances registry, revocation-epoch table, capability changes, chat/session history) lives in the encrypted CRDT store described in Section 5. Every peer holds a full copy; there is no capability-scoped signing material to distribute separately, because instances only ever sign with their own keypair (already in the database from creation). Alias assignment is unique within a federation: concurrent peer records proposing the same alias are detected by the alias plus the writer identities, and the canonical winner is the lexicographically smallest record ID (with the full ID as the deterministic tie-breaker). The losing record remains recorded, the alias remains unroutable while any conflict exists, and commands addressed to it fail with an explicit alias-conflict result until a subsequent write assigns a free alias.
- **Trust store shape (unchanged intent).** For the separate Verbinden flow, ingress checks share one table shape: `(alias, nostr_pubkey, iroh_node_id, valid_until, capabilities, epoch)`. Nostr ingress validates against `nostr_pubkey`, iroh accept-handler validates against `iroh_node_id`, both check the current epoch and reject stale-epoch decisions. Records live in `peer_instances`. The direct copied-vault path does not require that table: it authenticates the vault identity first, and identity rotation is the revocation boundary. The only shape change from the 2026-09-04 revision is that each paired-peer record is signed by the granting _instance's_ keypair, not by a federation-root; verifiers check "the granter's own record was current at the granting epoch and carried `pairing-authority`".
- **Revocation.** Any instance carrying `pairing-authority` at the current epoch may publish a revocation record naming the target's `nostr_pubkey` + `iroh_node_id` + a monotonically-increasing federation epoch. Revocations merge as an add-wins union keyed by each record's unique identity; the effective peer trust is derived after merging. Revocation epochs merge monotonically: the merged federation epoch is always the maximum observed epoch and can never decrease. A revocation at epoch `R` suppresses every matching peer grant at epoch `R` or lower, so a concurrent grant and revocation for the same target at the same epoch resolves to **revoked**. A later grant may restore trust only at an epoch strictly greater than the latest matching revocation and only when its signer is authorized at that epoch. Peer records and capability grants whose epoch is below the current merged epoch are rejected, so delayed CRDT state cannot restore a revoked peer. The peer-registry spec MUST include a concurrent merge test: two replicas start from the same state, independently merge a grant and a revocation for the same target at epoch `R`, exchange both records, and converge on revoked effective trust while retaining both records in the add-wins log.
- **Capability slots.** `pairing-authority` and `confirmation-authority` are per-peer capability flags, delegable and revocable through the same peer-registry flow. Delegation is non-escalating: a granter may only grant capabilities it currently holds, so `pairing-authority` cannot grant `confirmation-authority`. The Genesis instance (first-run on a brand-new SQLite) holds both by default. A protected write requires a current `confirmation-authority` peer record signing the release. The two slots are independent: a parent may pair a joiner with any subset of its own capabilities, including neither. Concrete motivation for the "neither" case is a temporary install on an untrusted host (a laptop in an internet cafe, a shared workstation), which participates in chat and reads federation state but cannot invite further peers or approve protected writes. Wiping that temporary instance's local state on session end is tracked as _Ephemeral Session Mode_ in Section 12.
- **Compromise model.** Attacker without the SQLCipher passphrase: at-rest data (including instance keys) remains unusable. Response: any surviving peer with `pairing-authority` publishes a revocation record for the compromised peer, epoch-bumped. Fast. Attacker with both the `.db` file and the passphrase: attacker can operate that peer authentically until revoked. If the compromised peer had `pairing-authority`, attacker-driven rogue pairings may outrun individual revocations; the operator escalates to federation-wide reset (below). If the compromised peer had neither capability, blast-radius is bounded to that peer's role (chat participant only).
- **Federation-wide reset (panic button).** Response to a compromise the operator cannot outrun with individual revocations. The vault identity is rotated on a surviving trusted instance (new keypair written to the singleton `vault_identity` row); every replica must then be re-established from a snapshot carrying the new identity. Per-device endpoint keys in `known_devices` are re-minted per install as part of the re-establish. All existing `peer_instances` records are declared invalid at a cutoff epoch communicated out-of-band. State to preserve (chat history, skills, memory) is exported from a trusted `.db` snapshot before reset and re-imported into the freshly-paired federation.
- **Peer-record authorization flow.** What used to be a separately-signed `attestation` event under a federation-root is now a `peer_instances` CRDT write signed by the granter's own instance keypair. Signature verification path: (1) the granter's own peer record was in the merged trust store at the write's epoch, (2) the granter carried `pairing-authority` at that epoch, (3) the granted capability set is a subset of the granter's own. All three checks are local to any receiver's copy of `peer_instances`.

**Explicitly out of the revision.** NIP-46 remote-signer support and hardware-token custody are not in v1. If cross-user sharing (`founding.md` §6) returns post-v1, a persistent federation-scope keypair distinct from per-instance keys may be reintroduced then, because "Alice's federation" needs a stable public identifier that Bob's clients can reference. Whether the paper-seed model returns in that context is a post-v1 design question.

## 5. Storage, at-rest encryption, and CRDT sync via `haex-crdt`

`founding.md` §4 items 5 and 8 (encrypted-at-rest choice, multi-device routing) are resolved by consuming an extracted library.

- **`haex-crdt`.** The SQLite + CRDT-sync layer currently living inside `haex-vault 2.x (Legacy)` is extracted into a standalone Rust crate named `haex-crdt`. Both `haex-vault 2.x` and `holzi` consume it as a Rust library dependency. Holzi pins [`haexmas/haex-crdt` at `1c069ef0ea19143af2748f40fc41cba05c94dbe1` (`Cargo.toml`, package 0.4.0)](https://github.com/haexmas/haex-crdt/blob/1c069ef0ea19143af2748f40fc41cba05c94dbe1/Cargo.toml). The crate owns the scanner and apply APIs, while holzi owns the v1 sync transport that wires them together.
- **At-rest encryption is inherited.** `holzi` does not choose its own at-rest scheme; it uses whatever `haex-crdt` provides (working assumption: SQLCipher, to be confirmed during extraction). If `haex-crdt` changes its scheme, `holzi` moves with it.
- **What lives in `haex-crdt`-synced state.** Federation-scope encrypted state that must converge across paired replicas of the same vault: the vault identity keypair (`vault_identity`, singleton), the device registry (`known_devices` installation UUID as immutable row identity, device UUID, alias, timestamps, and endpoint metadata), and later — with sync — `peer_instances` / revocation-epoch / capability grants, chat/session history, skills/memory (post-v1 scope but reserved). The installation UUID file remains local to the host, while its value is carried as the `known_devices` row's primary-key identity (`row_pks`) when that row is synchronized.
- **What does not live in `haex-crdt` at all.** Ephemeral runtime state (open iroh sessions, current relay connections), local-only preferences (local model file paths, active-provider selection), and the installation UUID **file** at `<AppLocalData>/installation-id` (outside every vault; the file itself is never sent over the wire). Its value is stored as the `known_devices` primary-key identity and is carried in that row's CRDT `row_pks`.
- **Device identity for HLC (`DatabaseBootstrap`).** Decided 2026-09-07, refined 2026-09-08. Two UUIDs, layered:
  - **Installation UUID** — one random UUID per holzi installation on a host, stored in
    `<AppLocalData>/installation-id`, shared by every vault opened by that installation, never sent
    over the wire. Its sole purpose is the local lookup key for the next UUID.
  - **Vault-device UUID** — one per (vault × installation), stored as a row in `known_devices`
    keyed by the installation UUID. This is the HLC node id for this replica. Two vaults opened
    by the same installation get independent random vault-device UUIDs; two installations opening
    the same vault (via a copied `.db`) get independent random vault-device UUIDs. The installation
    UUID is the immutable row identity and is carried in CRDT `row_pks`; it is not a mutable column
    update.

  On every open, `DatabaseBootstrap` reads the installation UUID → looks up
  `known_devices` → returns the row's UUID or (on first open of this vault by this install) mints
  a fresh vault-device UUID and inserts the consumer-owned row atomically. The crate then uses
  that returned UUID for HLC initialization. The pinned 0.4.0 revision provides this hook; no
  adoption API, attestation, or rekey is needed.

  The one case that MUST be prevented is the same database opened twice on the same host, which the
  `fs2` file lock inherited from `haex-vault 2.x`'s `vault_lock.rs` already covers
  (`DatabaseError::VaultAlreadyOpenElsewhere`). Duplicating the installation UUID file itself (VM
  clone, `rsync -a` of the entire app data directory) breaks the vault-device-UUID uniqueness
  invariant; detection is the sync layer's responsibility (see the command contract).

- **Three cross-device channels, disjoint by role.** With `haex-crdt` in play, holzi has three cross-device channels rather than the two in `founding.md`:
  1. **Nostr** — semantic events: presence, capability advertisement, commands, LLM prompts/responses, DMs, control-plane events, confirmation intents and releases.
  2. **iroh** — bulk bytes: `blob.offer` (post-v1) and `stream.offer` (post-v1). No v1 traffic on this channel except peer keep-alive.
  3. **`haex-crdt` sync** — federation state deltas: `peer_instances` records, revocation-epoch changes, capability changes, chat/session state.
     These channels do not share ownership. `haex-crdt` is authoritative for chat/session records and their stable message IDs; Nostr events are transport-only envelopes for prompts, responses, and control messages, and are not a second chat history. A received event is applied to the CRDT record keyed by its message ID exactly once; replays and duplicate deliveries are ignored. Nostr never carries CRDT deltas; `haex-crdt` never carries a command intent; iroh never carries state. holzi owns the v1 CRDT sync transport and wires it to `haex-crdt`'s scanner and apply APIs; the crate supplies no transport in this contract. A future consumer may choose a different transport without changing the crate's storage API.

## 6. Provider layer

Native Rust `LlmAdapter` trait with five implementations, hardcoded in v1. No Python sidecar, no plugin surface.

1. **Anthropic** — direct Messages-API adapter. Native support for prompt caching and extended-thinking as those Anthropic features stabilize.
2. **OpenAI** — direct Chat-Completions adapter for canonical OpenAI endpoints.
3. **OpenRouter** — direct adapter, exposes the OpenRouter model catalog behind one API key.
4. **Ollama-HTTP** — adapter for an externally running Ollama on the host or on another operator device.
5. **Generic-OpenAI-compatible** — user supplies a base URL and optional API key. Escape hatch for LiteLLM-as-proxy, LM Studio, Azure-OpenAI, custom vLLM/TGI deployments, and Ollama's OpenAI-compat endpoint.

Plus **embedded runner** as a first-class local backend, not an adapter row: on desktop and server holzi ships a runner (candidate stacks: llama.cpp / candle / mistral.rs, chosen at implementation time based on mobile-portability) and a default small model. On mobile the runner ships with a smaller preset (Phi-3-mini-class or Qwen-tiny-class); battery/storage cost is accepted, iOS App Store size restrictions are respected. Model choice at runtime is always explicit; no automatic routing between local and provider adapters.

## 7. Mobile as full peer with foreground-only availability

`founding.md` §4 item 7 (mobile scope for v1) is answered: iOS and Android are in v1. `founding.md`'s "device equals relay equals Tauri application" one-to-one identity is preserved on mobile, with a documented availability constraint.

- **Full-peer while foreground.** When the mobile app is in the foreground, the mobile device hosts its Nostr relay endpoint and its iroh peer exactly as desktop does. It accepts inbound Nostr command events from the operator's other devices, serves NIP-17 wrappers, participates in `haex-crdt` sync.
- **Unreachable while backgrounded.** When the mobile OS backgrounds and suspends the app, the relay endpoint and iroh peer are unreachable. This is _not_ an error state and must not surface as one. Other devices in the federation observe the mobile device as unreachable and continue.
- **Presence events carry availability class.** Each device's presence event declares an availability class — working proposals: `always-on` (typical desktop or server), `foreground-only` (typical mobile). Availability class is normative for sender behavior: a command targeted at a currently-unreachable `foreground-only` device does not wait on long relay timeouts; the sender returns `queued` immediately while retaining the command intent.
- **Delivery semantics for mobile targets.** Command intents addressed to a currently-unreachable `foreground-only` device are persisted at the sender's home relay with status `queued`, a TTL, and a stable intent ID. The same intent ID is reused for retries, so the target executes an intent at most once. Unreachable targets enter `queued`; when the intent is delivered after a fresh presence event, it moves to `accepted` and may then become `completed` or `failed`; if its TTL elapses while queued, it moves to `expired`. A sender can query every status by intent ID. Exact TTL and the presence-event kind carrying availability class are spec-phase work.
- **iroh sessions to mobile.** iroh streams to a mobile peer terminate on background; there is no session resumption across the sleep/wake boundary in v1. This is aligned with `stream.offer` being post-v1: v1 has almost no iroh traffic to a mobile peer to speak of.
- **Push notifications are not in v1.** Waking a mobile app via APNs/FCM to receive a delayed command is post-v1. In v1, the operator opens the mobile app to check queued intents (like most messengers already require for practical delivery on iOS anyway).

## 8. Confirmation-authority

`require-confirmation` and the signed-release-event model from `founding.md` are in v1, unchanged in shape. Restated for completeness:

- Any capability class may be marked `require-confirmation` in policy.
- The relay holds intents in that class until it receives a signed release event.
- The release event is signed by an instance whose current `peer_instances` record carries the `confirmation-authority` capability. That capability is delegable and revocable through the peer-registry flow (Section 4 above; per the 2026-09-06 revision, records are signed by the granting instance's own key, not by a federation-root).
- The release event names the exact intent ID.
- Receivers verify the release-signer's `peer_instances` binding, the current epoch, and the intent scope before accepting the release; then the intent forwards.

The killer v1 use case: an operator confirms from their phone a `require-confirmation` action initiated on their laptop.

## 9. Relay implementation choice

`founding.md` §4 item 4 is not fully decided; a directional choice is captured.

- Direction: **embedded Rust relay with SQLite backend**, participating in the broader Nostr ecosystem's kinds and NIPs rather than a minimalist purpose-built one. Rationale: the operator wants to leverage Nostr ecosystem tooling and standards over time; standing on an existing Rust relay reduces boilerplate and Nostr-spec-drift.
- Candidate libraries: `nostr-rs-relay` (github.com/scsibug/nostr-rs-relay), `rnostr` (github.com/rnostr/rnostr), or building on top of `nostr-sdk` (github.com/rust-nostr/nostr). Exact choice is a spike task inside the first implementation slice, not a design-time decision.
- The relay must be embeddable in-process on desktop, server, iOS, and Android. If no candidate meets that bar, the spike proposes either a fork or a minimal purpose-built path, and this document's directional choice is revisited.

## 10. Dependency map

Two parallel workstreams gate holzi v1's first implementation slice:

- **haex-hive schema migration** (in progress, in `~/Projekte/haex-hive/`). Until the migration lands, holzi's declared `com.github.haexmas.atoms.graphify-first-authoring` atom does not take effect. Blocks _tooling_, not code — holzi can proceed on design/spec work without it.
- ~~**`haex-crdt` extraction from `haex-vault 2.x`**~~ — **cleared**. Holzi consumes the pinned 0.4.0 revision; no local checkout, branch, or future release assumption is part of this dependency.

Both are outside this repository. Neither is a holzi task. The haex-hive migration remains a
tooling dependency only; it does not delay holzi's implementation timeline now that `haex-crdt`
is cleared.

## 11. First implementation slice — walking-skeleton sketch

A concrete first slice that can start while the haex-hive migration is still pending. Not a spec; a sketch of what to bring up first.

**Target.** Two desktop devices, paired into a fresh federation, exchanging a ping-pong command.

**Steps in the slice.**

1. Holzi Tauri app boots on device A. First-run wizard creates a fresh SQLite (SQLCipher passphrase set by the operator), runs the unsigned pre-HLC bootstrap to generate the vault identity keypair (a secp256k1 keypair for Proof-of-Possession auth between replicas) and store it in the singleton `vault_identity` row, mints a vault-device UUID for device A and inserts it into `known_devices` keyed by device A's installation UUID, then writes the Genesis self-record into `peer_instances` with both `pairing-authority` and `confirmation-authority` (Genesis default). The installation UUID remains in the local sidecar file; when the row syncs, it is carried as the row's primary-key identity.
2. Device A's embedded Nostr relay endpoint comes up bound to the instance's `nostr_pubkey`; its iroh peer comes up bound to the instance's NodeId.
3. Device A's embedded Nostr relay endpoint comes up, announces a Presence event with `always-on` availability class.
4. Holzi boots on device B. First-run wizard offers "join existing federation." Device A displays a short-lived pairing QR (containing a one-time token and a Nostr contact hint). Device B scans it.
5. Device B generates its own Nostr/iroh identities and signs the canonical pairing transcript. Device A verifies and signs the same transcript; both write mutually identifying `peer_instances` records, signed by their respective instance keys, and `haex-crdt` synchronizes those peer records between the two devices.
6. Device B's relay endpoint comes up, publishes its Presence.
7. From Device A's UI, the operator sends a `ping` command targeting `@device-b`. The relay resolves the alias only through the target's mutually signed `peer_instances` record, checks its federation-epoch binding, and routes the command to Device B; Device B echoes a `pong` result.

**Explicitly out of this slice.** LLM chat, external MCP server, MCP-to-Nostr adapter, confirmation-authority flow, embedded model runner, provider adapters, mobile.

Those come as follow-on slices, each with its own spec.

## 12. What remains open

Not decided by this document; still spec-phase work.

- **From `founding.md` §4, still open:** item 1 (event-kind numbering), item 2 (peer-record wire format + replay protection — reshaped from the old `attestation` event per Section 4's 2026-09-06 revision), item 3 (ingress ACL policy language), item 9 (comparison due-diligence). Item 6 (master-key-custody options) is dropped for v1 — the SQLCipher passphrase is the only operator-managed secret; a broader custody story returns only if cross-user sharing reintroduces a federation-scope keypair post-v1.
- **From this document, still open:** presence-event kind and availability-class wire format; sender queue TTL for `foreground-only` targets; embedded-runner stack selection (llama.cpp vs candle vs mistral.rs); exact `haex-crdt` crate boundaries and its own transport.
- **Ephemeral Session Mode.** Temporary installs on untrusted hosts (an internet cafe laptop, a shared workstation) need a session-scoped lifecycle. Regardless of whether the eventual implementation uses an attested empty-capability device or an unattested guest peer, the parent issues a short-lived session lease with an explicit expiry bound to the temporary device or guest-session identity. On a normal session end, the parent publishes a revocation targeting that lease or attestation before the temporary device wipes its local haex-crdt store, per-device keypairs, and cached federation state. If the parent cannot reach every peer, the lease expiry is fail-closed; peers stop accepting the session identity when the expiry passes. A crash, forced termination, or power loss skips the clean-end revocation but has the same bounded-expiry outcome, and the operator can issue an explicit revocation from any surviving authority after recovery. Peers validate both revocation and expiry, so a copied device keypair cannot remain accepted beyond the session lease or a subsequent epoch bump. Open sub-questions are the exact lease/heartbeat wire format and whether the guest or empty-capability model ships in v1; the remote cleanup and fail-closed lifetime are requirements for either model.
- ~~**haex-crdt extraction plan**~~ — resolved. The crate shipped; see [`2026-09-04-haex-crdt-extraction-plan.md`](./2026-09-04-haex-crdt-extraction-plan.md) for the post-hoc status header and what the delivered API actually looks like.

## 13. Next actions for holzi (this repo)

Once this document is committed:

1. ~~Wait on haex-hive migration and `haex-crdt` extraction.~~ `haex-crdt` cleared 2026-09-06; the haex-hive migration blocks tooling only.
2. Start the "Project structure and spec workflow" question (`founding.md` §5 item 1): whether to adopt speckit as haex-hive does, adopt it lightly, or use a different approach. This can begin before the two blockers clear.

## 14. Follow-up decisions (post-drafting)

- **Speckit adopted** (2026-09-04): decision on §13 item 2 resolved in favor of full speckit initialization. `specify init --here --ai claude --offline` was run; `.specify/`, `.claude/skills/speckit-*`, and `CLAUDE.md` (speckit block) are in place.
- **First spec written**: [`specs/001-frontend-onboarding/`](../../specs/001-frontend-onboarding/) covers the Landing / Anlegen / Öffnen / Verbinden / Unlock surface. It also revises `founding.md` §2.2 to permit multiple `.db` files per install with exactly one active — see that spec's [`research.md → Storage-file model`](../../specs/001-frontend-onboarding/research.md) for the rationale. This revision does not change v1 scope; it changes how the walking-skeleton is reached (per §11) and how `<AppLocalData>/instances/` is laid out on disk.
- **Cross-user sharing deferred, design captured** (2026-09-07):
  [`2026-09-07-cross-user-sharing-deferred-design.md`](./2026-09-07-cross-user-sharing-deferred-design.md)
  records how shared spaces would work without MLS or UCAN. Not v1. Sequencing decided: closed
  federation first, open federation after.
- **`blob.offer` confirmed post-v1** (2026-09-07): the §2 line stands, but blob transfer is now the
  named next capability after v1 rather than an open question. Exchanging files between the
  operator's own devices ("fetch my photos from my phone") is a headline use case, so v1 design
  work must not foreclose it.
- **Plane roles sharpened** (2026-09-07): Nostr carries authentication, authorization and
  orchestration; iroh carries data and file transfer. This tightens `founding.md` §2's two-plane
  split. It sits awkwardly with §5's "iroh never carries state" should the `haex-crdt` sync channel
  later run over iroh — which the v1 contract resolves by making holzi the owner of the sync
  transport and using `haex-crdt` only through its scanner/apply APIs. The concrete holzi transport
  remains a sync-channel spec decision.
