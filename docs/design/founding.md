# HaexVault — Founding Design

**Status**: Draft. Founding design captured 2026-09-04.

**Revised by**:

- [`docs/plans/2026-09-04-v1-scope-design.md`](../plans/2026-09-04-v1-scope-design.md). That document draws the v1 scope line and revises the identity/pairing model, the storage layer (introducing `haex-crdt`), and the mobile availability model. Where the two documents disagree for v1, the plans document wins. **§4 of that plan carries a 2026-09-06 revision that fully supersedes §2's Identity model in this founding doc** (Master key + master-signed device attestations → per-SQLite-instance keypairs stored inside the encrypted database; paper-seed dropped; recovery via surviving synced instance or `.db` backup).
- [`specs/001-frontend-onboarding/spec.md`](../../specs/001-frontend-onboarding/spec.md). First speckit spec. Refines §2.2 "Device equals relay equals Tauri application" to "the _active_ SQLite instance equals the running Nostr relay endpoint equals the running iroh peer" — the Tauri application is a container that may hold multiple `.db` files on disk with exactly one active at a time. Rationale in [`specs/001-frontend-onboarding/research.md → Storage-file model`](../../specs/001-frontend-onboarding/research.md).

**What haex-vault is**: a portable, isolated personal-agent app. Runs as a Tauri application on any device the operator owns. All LLM credentials, model choice, and history live inside the app. LLM interactions happen inside haex-vault; the host system's LLM configuration (if any) is unrelated. External access to haex-vault is only via MCP.

**Origin**: This design emerged from a brainstorming session that briefly considered building the same feature set inside haex-hive. The scope shift (from haex-hive's harness-configurator role to a full agent runtime with signaling and transport planes) was too large, so haex-vault lives as its own project. haex-hive stays as it is (harness plane per its Scope Realignment). Where useful, haex-vault may consume haex-hive-distributed configuration for the harness that runs inside it, but that is a downstream integration question, not a coupling.

**Purpose of this document**: hold the founding architecture in one place so the first implementation slices have a normative reference. Field names, event kinds, wire formats, and identity derivations are working proposals, not settled interfaces.

---

## 0. Normative status

This is a design record. Every requirement below has to be re-expressed as a numbered spec before any code moves. HaexVault adopted the full speckit workflow on 2026-09-04; `specs/001-frontend-onboarding/` is its first numbered spec. Field names, event kinds, wire formats and identity derivations are working proposals, not settled interfaces.

## 1. What haex-vault is, and what it is not

HaexVault is:

- A **Tauri application** that runs on desktop and server (mobile later, see open questions).
- **Isolated from the host**: LLM credentials and model configuration live inside haex-vault; the operator can open haex-vault on any device, including one without any LLM installed on the host, and use it normally.
- A **Nostr relay endpoint** and an **iroh peer** in one process, forming (with the operator's other haex-vault installations) a **closed federation** of the operator's own devices.
- An **MCP server** for external clients: the only way for anything outside haex-vault to reach into it. Standard MCP auth applies.
- **Local-first**: local model available when the device has the capacity; provider model when the operator explicitly chooses it; no automatic model routing.

HaexVault is not:

- A coding harness. If the operator wants a coding harness inside an IDE, that lives in the IDE's own tool space (typically distributed via haex-hive).
- A bridge into IDEs. External integration is by having the external tool speak MCP into haex-vault; haex-vault does not ship IDE plugins or custom session tokens at v1.
- A general execution plane for third parties. Cross-user sharing is a future direction (Section 6), not a v1 feature.

## 2. Architecture summary

### Two planes, disjoint by intent

- **Nostr** carries identity, presence, discovery, capability advertisement, commands, LLM prompts and text responses (including chunked text-token streams), DMs, and control-plane events (including iroh handshake tickets).
- **iroh** carries bulk bytes in two shapes over the same peer connection: content-addressed blobs for persistent artifacts (user-facing files, generated media saved as images, video, audio) via iroh-blobs' BAO tree (chunked, resumable, verifiable); and ephemeral QUIC streams for real-time media (voice call, video call, screen share, live playback of a large file whose bytes are not being captured as a blob).

The split is by traffic class, not by a runtime bitrate check. LLM text-token streams are always on Nostr: each chunk is a small semantic event that benefits from audit-friendly ingress checks, throughput is trivial (a few KB/s per session), no measurement needed. Real-time media sessions (voice, video, screen, live large-file playback) are always on iroh: Nostr's per-event JSON envelope, base64 encoding, and per-event policy check are the wrong tool for that traffic profile, and public relays would drop the connection under it. A session's transport is fixed at offer time by the offer kind and does not migrate between Nostr and iroh at runtime; a session that changes character (an audio call gaining a video track, for example) opens a new offer.

### Device equals relay equals Tauri application

Each installation is one process that hosts:

- The Tauri UI.
- The agent runtime, including local and provider model adapters (local LLM, OpenAI/Codex, Claude, further providers, chosen explicitly by the user per request).
- An embedded Nostr relay endpoint.
- An iroh endpoint.
- A local encrypted database.
- A permission/policy layer.
- An **MCP server** exposed to external clients on the host.

"Relay" and "device" are the same trust boundary. There is no wire protocol between the app and the relay; policy checks are local function calls before an event is written to the relay database.

### Local-first, user-chosen models

- A local model is available whenever the device has the capacity to host one.
- The user selects model and reasoning tier explicitly. No automatic model routing overrides that choice.
- Relay and network faults must not block local work.

### Two-track ingress policy

The relay treats two classes of events differently:

- **Command events** (device-to-device: MCP calls, presence refresh, ticket handshake, status queries, policy updates). Accepted only from pubkeys in the master-attested device registry, with NIP-42 auth. Egress only to the relay endpoints of the operator's other devices. Since device equals relay one-to-one, the home-relay identity for a target device is the relay URL announced in that device's most recent valid presence event, signed by its attested `nostr_pubkey`; stale or revoked presence entries are excluded from egress. This is the closed federation.
- **DM events** (NIP-17 gift-wrapped, `kind:1059`). The outer wrapper carries a per-event random pubkey by design, so the operator blocklist is applied against the actual sender pubkey after gift-wrap decryption and sender validation, not against the outer pubkey. Wrapper events are admitted subject to explicit size, rate, and retention limits set on the relay. NIP-42 authenticates the local client publishing wrappers to the operator's own relays. Egress to whatever public relay set the user chose. This is the open channel.

Both handlers live in the same relay instance, dispatched by event kind.

### Identity model

- **Master key**: held offline (hardware token, a Nostr signer such as NIP-46, or paper backup). Not used for daily traffic.
- **Device keys**: one per installation. Each device holds its own Nostr keypair AND an iroh NodeId; both are bound in the same master-signed attestation event. Attestations are short-lived (working default: 7 days) and renewed while the master is present. Revocation is a single event that invalidates both endpoint identities.
- The attestation event kind is a haex-vault-specific kind, not NIP-26 (deprecated).

### Trust store covers both endpoints

Ingress checks share one table: `(alias, nostr_pubkey, iroh_node_id, valid_until, capabilities, epoch)`. Nostr ingress validates against `nostr_pubkey`, iroh accept-handler validates against `iroh_node_id`, and both check the current epoch and reject decisions that would be taken on a stale epoch value. Revocation bumps the epoch atomically for both endpoints; iroh transfers in flight against a stale epoch are cancelled by the accept-handler on the next chunk exchange. Any drift between the two trust states is structurally impossible. The precise epoch propagation, revocation ordering, and in-flight cancellation contract is spec-phase work.

### iroh authorization piggybacks Nostr

Two iroh session shapes exist, authorized identically by a signed offer event on Nostr plus a peer-bound iroh ticket:

- **Blob offer** (`blob.offer`): announces a content-addressed transfer. Carries the Blake3 hash, filename, MIME, size, and a short-lived iroh ticket.
- **Stream offer** (`stream.offer`): announces an ephemeral QUIC stream for real-time media. Carries a stream kind (voice, video, screen, generic), codec parameters, direction (unidirectional or bidirectional), an expected duration hint, and a short-lived iroh ticket. No content hash, since the payload is generated in real time.

Authorization is per iroh QUIC stream, not per underlying iroh connection. iroh multiplexes multiple streams onto a single connection between two peers, and every new stream carries its own ticket check at accept time. On each new-stream accept:

- The ticket must be currently issued, unexpired, bound to the intended recipient's `nostr_pubkey` and `iroh_node_id`, and tied to the event ID of the announcing offer. A ticket presented from any other peer identity is rejected, so a leaked ticket cannot authorize a session by a third party even before its single-use consumption.
- Ticket consumption is atomic and durable, keyed by the pair (`ticket`, offer event ID). Concurrent accepts race exactly one to success; every other concurrent accept fails. The ticket is consumed at the start of the handshake (fail-closed): a failed handshake does not release the ticket, the offer must re-issue.

Single-use therefore means "one accepted stream per ticket": one blob download session, or one `stream.offer` session. Session lifetime after the accept (ordinary teardown, heartbeat behavior on transient loss, and behavior on iroh endpoint reconnection or migration) is spec-phase work and listed in Section 4, item 11. No separate authorization layer sits on top of iroh.

### MCP as capability schema

- Local tools are exposed by a local MCP server per device (haex-vault's own tool space).
- A compact capability summary (tool names, resource URIs, version) is published in the presence event; the full MCP schema is fetched over the direct connection once a session is opened.
- Remote MCP invocation between the operator's own haex-vault devices is translated by an adapter into Nostr command events. The receiving device's adapter dispatches the tool call against its local MCP server and returns the result over Nostr.
- For results above a size threshold, or of a file-artifact type, the response event carries only metadata plus a `blob.offer`; the bytes travel via iroh. The calling LLM sees a normal MCP result with a resource URI.

### External MCP access

HaexVault exposes an MCP server on the host so that external tools (a shell, an IDE with an MCP-capable AI assistant, a script) can reach it as any MCP server. Authentication follows the MCP standard for the chosen transport (stdio, HTTP, Unix socket). Custom bridge tools, session tokens tied to particular IDEs, or per-environment policy tuples are not part of v1; if a class of external caller needs sharper policy than MCP's own mechanisms provide, that is a future direction (Section 6).

### Confirmation for write actions

Policy may mark any capability class as `require-confirmation`. The relay holds such intents until it receives a signed release event from a device whose current attestation carries a `confirmation-authority` capability, master-attested and rotatable or revocable through the same attestation flow. The release event names the exact intent ID it authorizes; receivers verify the attestation binding and the intent scope before accepting the release, then forward the intent.

### Explicit device targeting

Any cross-device command must name a target device by alias. Aliases are assigned when a device is added to the trusted network: the master-signed attestation binds the alias to the initial `nostr_pubkey` and `iroh_node_id` pair. In the chat surface, aliases are typed with an `@`-mention convention (e.g. `@laptop-home`); the LLM sees the mention as a routed instruction, and the same explicit-target skill governs resolution and confirmation regardless of whether the target was typed as `@alias`, chosen from a picker, or supplied by another tool. A skill enforces this: the resolved target (alias plus `nostr_pubkey` plus `iroh_node_id`, both stable across sessions and both drawn from the current device attestation) is shown to the operator before dispatch. The signed intent payload carries `target_nostr_pubkey`, `target_iroh_node_id`, and the attestation epoch that was current at resolution time; the receiver validates all three against its own current attestation before executing. An alias rebinding between confirmation and dispatch therefore cannot silently route an authorized command to a different device: the alias is used for display and lookup only, never as the authorization binding. Alias uniqueness within the operator's device registry is enforced by the master signing at most one active attestation per alias, and rebinding requires a fresh master-signed attestation with a new epoch. When the command carries a file transfer, the ticket fingerprint of the accompanying `blob.offer` is shown alongside; pure control commands have no ticket at this stage. Stale presence is surfaced.

## 3. What Nostr does that iroh does not, and vice versa

Recorded here so future readers do not relitigate:

- Nostr provides asynchronous publish to an offline recipient (relay stores events), identity-based discovery (a pubkey reaches all its posts), a cross-client public data model, censorship resistance through relay pluralism, zero infrastructure to publish, and Lightning-native payments.
- iroh provides two efficient direct-P2P transports over the same QUIC connection with hole-punching: content-addressed blob transfer (chunked, resumable, verifiable via BAO tree) for persistent artifacts, and ephemeral QUIC streams for real-time media. Both inherit the same peer-identity binding.

The split in Section 2 is chosen so each layer does what it is good at. No layer is asked to do the other's job.

## 4. What this document does not decide

Recorded here so the follow-up spec work knows its scope:

1. Event kind numbering for command events, presence events, `blob.offer`, `stream.offer`, attestation, revocation, confirmation. Working proposals only; numbers not fixed.
2. Wire format of the device attestation event (fields, signature scheme, replay protection). Replay protection is normative for every state-changing event kind (attestation, revocation, command, MCP call, confirmation release, `blob.offer`, `stream.offer`), covering event or intent ID, expiry, sender-and-target binding, durable deduplication or monotonic sequence checks, and explicit idempotency rules. The attestation spec fixes the mechanism once; the ingress, blob, MCP, and runtime specs reuse it.
3. Policy language for the ingress ACL (per-tool, per-argument, per-device-pair).
4. Relay implementation choice (nostr-rs-relay embedded, strfry embedded, or a purpose-built minimal relay). Trade-offs not yet weighed.
5. Encrypted-at-rest storage choice (SQLCipher, Sled + AGE, or other).
6. Master-key custody options (which of NIP-46, hardware token, or paper backup are supported at v1).
7. Mobile scope for v1. Mobile is an eventual target; whether v1 ships mobile at all, or only desktop and server, is unresolved.
8. Multi-device routing when several devices offer the same capability. The explicit-target skill covers UX; how presence events express load hints and "prefer for capability X" flags is unresolved.
9. Comparison with existing tools (Buzz, OpenHands, Remote Control, ACP-based bridges) as normal design due diligence, once the spec work starts. Background research, not a project-approval precondition.
10. Sequencing: which of `attestation and ingress policy`, `ping round-trip`, `iroh blob roundtrip`, `MCP adapter and LLM` lands first as the initial implementation slice.
11. Stream session semantics for `stream.offer`, unresolved and normative in the stream spec:
    - Session identifier and its relationship to the announcing offer's event ID.
    - Ticket lifetime relative to session lifetime: strict single-use per stream, or session-scoped so a normal iroh QUIC connection migration continues the same authorized session without a fresh ticket.
    - Heartbeat interval, missed-heartbeat grace period, and behavior during transient network loss, so dead sessions are removed without terminating valid ones under short outages.
    - Reconnection semantics on iroh endpoint address change: same session continues under the existing authorization, or fresh authorization is required.
    - Codec negotiation and bandwidth adaptation within a session.
    - Optional in-session blob capture (for example, saving a video call to a persistent blob mid-stream).

## 5. Follow-up work

Ordered by the earliest thing that must exist for the rest to have a normative home.

1. **Project structure and spec workflow for haex-vault.** Resolved on 2026-09-04: haex-vault uses the full speckit workflow; `specs/001-frontend-onboarding/` is the first normative slice. Future specs continue to be captured, reviewed, and versioned there.
2. **Spec for the `peer_instances` record and peer registry.** The smallest normative slice, no transport yet.
3. **Spec for the two-track ingress policy** with concrete event kinds and the relay policy engine surface.
4. **Spec for the `blob.offer` announcement, ticket lifecycle, and iroh accept-handler.**
5. **Spec for the MCP-to-Nostr adapter** (cross-device MCP invocation between the operator's own haex-vault devices) and remote capability advertisement.
6. **Spec for the local MCP server** exposed to external clients on the host, and its auth boundary.
7. **Spec for the Tauri application shell**: LLM adapters, credential storage, model routing UI, chat surface.
8. **Comparison with existing tools** (Buzz, OpenHands, ACP-based bridges) as normal due diligence, once the first implementation slice is underway.

Mobile scope, encryption-at-rest choice, and provider-model adapters are further follow-ups whose priority is set once the specs above are in flight.

## 6. Future direction beyond v1

Cross-user file and data sharing is a deliberate future extension. In v1 the trust model is a closed federation of one operator's own devices. A later version extends it to a **half-open sharing model**: an operator (Alice) can grant read (or richer) rights on a specific resource to another operator (Bob), either targeted to Bob's master pubkey or as a limited public share, for a defined period.

Sketched (not settled, not part of v1 scope):

- Two new event kinds signed by the resource owner: `access.grant` (names the resource, the grantee master pubkey, the rights, an expiry, optional device restriction) and `access.revoke` (references the grant event id, effective immediately).
- A third ingress track on the relay alongside command events and DMs: **guest events** (`access.request`, `blob.request`, `stream.request`) accepted only from pubkeys with a currently valid grant on the referenced resource, per a `guest_grants` table on the granter's device.
- Cross-operator attestation trust: the granter's device accepts the grantee's master-signed device attestations as proof of the grantee-device-to-grantee-master binding, but does not otherwise inherit trust from the foreign fleet. Pure signature verification, no persistent trust state.
- Grants are delivered to the grantee via NIP-17 DM plus published directly to the grantee's home relay if reachable; the grantee's UI shows a "granted access to R until D" entry.
- Existing `blob.offer` and `stream.offer` primitives are reused unchanged: the ticket binds to the grantee device's `nostr_pubkey` and `iroh_node_id` per the same rules that today bind to own-fleet identities.
- Revocation piggybacks the trust-store epoch mechanic: a revoke event flips the grant's epoch, in-flight iroh sessions tied to the grant are cancelled by the accept-handler on the next chunk exchange. The grantee does not have to observe the revoke for it to take effect; the granter's device simply stops honoring requests.

Open sub-questions for the future spec:

- Whether grants can reference dynamic resource sets (e.g. `/shared/*`) or only enumerated resources.
- Public share limits (per-grantee quotas, per-resource rate limits, aggregate egress budgets, abuse response).
- Discovery and change notification: does the grantee poll the resource, or does the granter push updates through the same DM channel that delivered the grant.
- Whether recursive delegation (Bob re-grants to Charlie) is allowed at all, and if so under what constraints.

Deeper integration with external environments (IDE plugins, session-token-authenticated bridge tools, environment-scoped policy tuples that let `laptop:vscode` and `laptop:shell` carry different rights) is a second future direction. In v1, external tools reach haex-vault through its plain MCP server with standard MCP auth; anything richer is out of scope until the v1 surface is stable.

This section is a scope pointer, not design input for the v1 specs listed in Section 5.
