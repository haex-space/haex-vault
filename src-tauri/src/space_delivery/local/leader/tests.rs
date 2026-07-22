//! Static-source regression guards for the leader module.
//!
//! Historical note: pre-split these tests lived inline in `leader.rs` and
//! used `include_str!("leader.rs")` + `split_once("#[cfg(test)]")` to extract
//! the production text. After the structural split into the `leader/`
//! directory, the production text is sourced by concatenating every
//! production submodule (`mod.rs`, `auth.rs`, `notify.rs`, `claim.rs`,
//! `dispatch.rs`, `util.rs`). None of those files contain `#[cfg(test)]`,
//! so the assertions retain the same shape; only the inputs were stitched.

#[cfg(test)]
fn production_source() -> String {
    let mut s = String::new();
    s.push_str(include_str!("mod.rs"));
    s.push('\n');
    s.push_str(include_str!("auth.rs"));
    s.push('\n');
    s.push_str(include_str!("notify.rs"));
    s.push('\n');
    s.push_str(include_str!("claim.rs"));
    s.push('\n');
    s.push_str(include_str!("dispatch.rs"));
    s.push('\n');
    s.push_str(include_str!("util.rs"));
    s
}

#[cfg(test)]
mod audience_check_tests {
    //! Regression guards for UCAN audience verification.
    //!
    //! Post-T6 reality: the unified `auth_gate::authorize_request` is the
    //! central gate for every authenticated, non-bypass space-delivery
    //! request. Its audience binding (`require_audience` against the
    //! connection-bound DID) is covered by `auth_gate_tests`.
    //!
    //! What this module still pins is the **Announce bypass** path. Announce
    //! cannot rely on the gate (the gate returns `Ok(None)` for it, because
    //! Announce is what populates the cached UCAN the gate later reads), so
    //! `require_ucan_capability` runs inline there. Without the `aud ==
    //! announced peer DID` check inside the helper, a peer P could replay
    //! another member's UCAN through its own QUIC channel and have it
    //! cached — the gate would then trust the cached `validated_ucan` on
    //! subsequent requests and the replay would pass. So the helper's
    //! invariants matter exactly as much as before, just for one caller.
    //!
    //! These tests are static-source assertions because the dispatcher
    //! requires `&mut LeaderState`, a tokio runtime, an `iroh::Endpoint`, a
    //! populated `connected_peers` map, and a SQLite schema with HLC
    //! triggers; building all of that costs more than the linting checks
    //! buy us. Behavioural coverage is deferred to e2e in haex-e2e-tests.
    //!
    //! Unit coverage of the helper itself (`require_audience` accepts /
    //! rejects) lives in `ucan::verify::tests`.

    use super::production_source;

    /// `require_ucan_capability` must take a `peer_did` parameter and call
    /// `require_audience` inside. Removing either would silently restore
    /// the replay window the audience check is meant to close.
    #[test]
    fn require_ucan_capability_takes_peer_did_and_calls_require_audience() {
        let source = production_source();
        let production = source.as_str();

        assert!(
            production.contains("peer_did: &str,"),
            "require_ucan_capability must declare a peer_did parameter"
        );
        assert!(
            production.contains("require_audience(validated, peer_did)"),
            "require_ucan_capability must invoke require_audience with the \
             announced peer DID — without this, a UCAN issued to any other \
             still-active member is accepted as a replay"
        );
    }

    /// Every UCAN-gated request handler must source `peer_did` from the
    /// connection-bound `verified_did` of the quic_did_auth handshake (Phase
    /// 2, see plan §4.1). Prior to Phase 2 the DID was looked up from the
    /// `connected_peers` map populated by Announce — that worked only when
    /// every handler ran after Announce and effectively meant "trust whatever
    /// Announce claimed", which is itself unsafe before the handshake binds
    /// the DID. After C7 every handler binds directly via
    /// `verified_did.to_string()`.
    ///
    /// **T6 update.** The pre-T6 invariant — "every UCAN-gated arm calls
    /// `require_ucan_capability(…, peer_did, …)`" — is gone. Sync arms now
    /// trust the unified `auth_gate` (which performs the same audience +
    /// capability + active-membership checks once per request). Only the
    /// Announce arm still calls the helper inline, because Announce is the
    /// bypass that *populates* the cached UCAN the gate later reads from.
    /// We keep the `verified_did.to_string()` guard below to pin that no
    /// regression brings back `require_peer_did(state, peer_endpoint_id)`.
    #[test]
    fn every_require_ucan_capability_call_passes_a_peer_did() {
        let source = production_source();
        let production = source.as_str();

        // Every handler that needs a DID for capability/buffer keys/audit
        // logs now derives it from the connection-bound verified_did. The
        // legacy `require_peer_did(state, peer_endpoint_id)` lookup against
        // `connected_peers` is gone — keeping it would defeat the Phase 2
        // promise that handlers no longer depend on Announce having run
        // first.
        let legacy_lookups = production
            .matches("require_peer_did(state, peer_endpoint_id)")
            .count();
        assert_eq!(
            legacy_lookups, 0,
            "no production handler should look up the peer DID via \
             require_peer_did any more — it must come from verified_did. \
             Found {legacy_lookups} legacy call sites."
        );

        let verified_bindings = production.matches("verified_did.to_string()").count();
        assert!(
            verified_bindings >= 4,
            "expected at least 4 `verified_did.to_string()` bindings inside \
             request handlers (covering MLS request envelope DIDs + the \
             three UCAN-audience-checked handlers); found {verified_bindings}"
        );
    }
}

#[cfg(test)]
mod auth_gate_wireup_tests {
    //! Regression guards for the T5 wire-up: `handle_delivery_request` must
    //! invoke `auth_gate::authorize_request` **before** the `match request`
    //! dispatch, so every non-bypass request is authorised at one choke point.
    //!
    //! ## Deviation from the plan (Phase 4 Task 4.2)
    //!
    //! The plan prescribed a behavioural integration test
    //! (`unannounced_mls_upload_is_rejected_at_dispatcher`) built against a
    //! `build_test_leader_state("SPACE")` helper. We deviated and shipped the
    //! three source-text assertions below instead. Rationale:
    //!
    //! - **Fixture cost is real.** `LeaderState` carries an `AppHandle`, an
    //!   iroh `Endpoint`, an MLS provider, an HLC, a tokio runtime, plus a
    //!   SQLite schema with HLC triggers. The existing
    //!   `audience_check_tests` and `claim_invite_did_binding_tests` modules
    //!   hit the same wall and resolved it the same way — source-text only.
    //!   We follow that precedent.
    //! - **Behavioural coverage already exists at the gate level.**
    //!   `auth_gate_tests::rejects_request_without_prior_announce` (and
    //!   sibling rejection-path tests) drive the gate against an in-memory
    //!   DB. Those tests prove the gate works. The source-text assertions
    //!   here prove the *wire-up*: the dispatcher actually calls the gate,
    //!   and on `Err` it returns the response before reaching the match.
    //! - **End-to-end coverage lives in `haex-e2e-tests`.** Real-network
    //!   negative paths (un-announced peer, revoked member, etc.) run there.
    //!
    //! Net: gate behaviour is exercised against an in-memory DB;
    //! dispatcher-to-gate wiring is pinned via static-source assertions;
    //! the full path is covered e2e. The plan's `build_test_leader_state`
    //! helper was not worth its weight given that triangulation.
    //!
    //! T6 has landed: the gate outcome is now `gate_ucan` (no prefix
    //! underscore) and every non-bypass arm reads its `ValidatedUcan` from
    //! the gate via `gate_ucan.as_ref().expect(...)`. SyncPush passes the
    //! gate UCAN into `authorize_inbound_sync_push` for downstream origin
    //! attribution; SyncPull keeps it for the success-path audit log;
    //! RequestRejoin and SubmitExternalCommit bind it to `_gate_ucan`
    //! solely so a future wire-up regression would panic loudly.

    use super::production_source;

    /// `handle_delivery_request` must call `auth_gate::authorize_request`
    /// before the `match request` dispatch. Without this single choke point
    /// the per-arm checks remain the only line of defence and the MLS-related
    /// arms (which had no inline UCAN check pre-T5) stay un-gated.
    #[test]
    fn handle_delivery_request_invokes_gate_before_match() {
        let source = production_source();
        let production = source.as_str();

        let fn_marker = "pub(crate) async fn handle_delivery_request(";
        let fn_start = production
            .find(fn_marker)
            .expect("handle_delivery_request must exist");
        let body = &production[fn_start..];
        let gate_call_pos = body.find("auth_gate::authorize_request(").expect(
            "handle_delivery_request must call auth_gate::authorize_request — \
                 without this every per-arm check stays the only line of defence \
                 and the MLS arms remain un-gated. See plan T5 §4.1.",
        );
        let match_pos = body
            .find("match request {")
            .expect("handle_delivery_request must contain `match request {`");

        assert!(
            gate_call_pos < match_pos,
            "auth_gate::authorize_request must be invoked BEFORE the `match \
             request` dispatch — gating after the match defeats the choke \
             point. See plan T5 §4.1."
        );
    }

    /// The gate-rejection arm in `handle_delivery_request` must `return` the
    /// `Response::Error` it receives, never fall through to the match. The
    /// `?` operator is impossible here because the fn returns `Response`,
    /// not `Result`, so the explicit `return response` pattern is the only
    /// safe shape.
    #[test]
    fn handle_delivery_request_returns_gate_rejection() {
        let source = production_source();
        let production = source.as_str();

        let fn_marker = "pub(crate) async fn handle_delivery_request(";
        let fn_start = production
            .find(fn_marker)
            .expect("handle_delivery_request must exist");
        let body = &production[fn_start..];
        let gate_call = body
            .find("auth_gate::authorize_request(")
            .expect("gate call missing");
        let match_pos = body.find("match request {").expect("match missing");
        let between = &body[gate_call..match_pos];

        assert!(
            between.contains("Err(response) => return response"),
            "expected gate-Err arm to be exactly \
             `Err(response) => return response` so the dispatcher \
             short-circuits before the match. A loose `return …(response)` \
             could silently wrap, log, or mutate the rejection; we pin the \
             exact shape. Found gate→match slice:\n{}",
            &between[..between.len().min(200)]
        );
    }

    /// Paranoid guard, **not load-bearing**: the compiler already catches
    /// a rename of `LeaderState::connected_peers` or `LeaderState::db`
    /// because the gate call-site in `handle_delivery_request` reads
    /// `&state.connected_peers` / `&state.db` directly. This test only
    /// matters for the narrow case where a future refactor introduces a
    /// builder/getter that *re-exports the same identifier with different
    /// semantics* — e.g. swapping the field for an `Arc<Mutex<…>>` wrapper
    /// behind the same name. Three lines, zero runtime cost; kept for the
    /// signal value to future readers.
    #[test]
    fn leader_state_exposes_fields_the_gate_consumes() {
        let source = production_source();
        let production = source.as_str();

        assert!(
            production.contains("pub connected_peers: Arc<RwLock<HashMap<String, ConnectedPeer>>>"),
            "LeaderState.connected_peers must remain the typed handle the \
             gate reads from"
        );
        assert!(
            production.contains("pub db: DbConnection"),
            "LeaderState.db must remain the typed handle the gate reads from"
        );
    }
}

#[cfg(test)]
mod claim_invite_did_binding_tests {
    //! Red regression for the ClaimInvite DID-spoofing vector (Phase 2 of
    //! `docs/plans/2026-06-01-quic-did-auth-primitiv.md`).
    //!
    //! ## The bug these guards lock down
    //!
    //! Today `handle_claim_invite` lifts the claimant's DID directly from
    //! the request payload (`Request::ClaimInvite { did, .. }`) and passes
    //! it to `invite_tokens::validate_invite` as `claimer_did`. The
    //! iroh-QUIC connection only binds the remote `endpoint_id`; nothing
    //! ties the claimant's payload-`did` to the connection cryptographically.
    //! Per §1.2/§4.2 of the plan, this enables two distinct attacks:
    //!
    //! - **Targeted-Invite spoofing (§4.2 scenario 1):** a token has
    //!   `target_did = Alice`; any peer who knows the token can send
    //!   `ClaimInvite { did: "Alice", … }` from their own endpoint and
    //!   becomes "Alice" inside the MLS group.
    //! - **Public-Invite identity spoofing (§4.2 scenario 2):** a token
    //!   has `target_did = None`; any peer can pick a fresh DID (or borrow
    //!   a known one) and have a UCAN minted for that DID.
    //!
    //! `invite_tokens::validate_invite` itself is fine — it correctly
    //! rejects `claimer_did` ≠ `target_did`. The vulnerability is the
    //! *call site*: it has no way to know the connection-verified DID
    //! until the Phase 2 wiring lands.
    //!
    //! ## Why source-text assertions, not full behavioural tests
    //!
    //! Same reason `audience_check_tests` above is source-text-only:
    //! `handle_claim_invite` requires `&LeaderState`, which needs an
    //! `iroh::Endpoint`, an MLS provider, a tokio runtime, and a SQLite
    //! schema with HLC triggers — building all that for a unit test
    //! costs more than these assertions buy us. Real behavioural T5+T6
    //! cases (per §4.4) live in the `haex-e2e-tests` companion PR
    //! (`invitations/targeted-invite-did-mismatch`,
    //! `invitations/public-invite-foreign-did`).
    //!
    //! ## TDD discipline
    //!
    //! These tests are `#[ignore]`d while the rest of the Phase 2 commits
    //! land in sequence so the suite stays green on every commit — the
    //! `#[ignore]` attribute is removed in commit C5
    //! (`feat(space_delivery): bind ClaimInvite to verified DID`) which
    //! is also the commit that makes them pass.

    use super::production_source;

    /// T5 (positive): the connection-verified DID flows into
    /// `handle_claim_invite`. Without a `verified_did` parameter the call
    /// site has nothing but the payload `did` to validate against — which
    /// is exactly the bug.
    #[test]
    fn handle_claim_invite_takes_verified_did_parameter() {
        let source = production_source();
        let production = source.as_str();

        assert!(
            production.contains("pub async fn handle_claim_invite(")
                && production.contains("verified_did: &str"),
            "handle_claim_invite must accept the connection-verified DID as \
             a parameter so the claim is gated by the cryptographically \
             bound peer identity rather than the client-supplied payload \
             `did` field. See plan §4.2 scenarios 1+2."
        );
    }

    /// T6 (negative): `validate_invite` must be invoked with the
    /// connection-verified DID, not with the payload `did` field. The
    /// guard pins the exact argument used at the call site — without it,
    /// any peer can spoof `Request::ClaimInvite::did` and pass a token
    /// validation gated only on a string match against `target_did`.
    #[test]
    fn handle_claim_invite_validates_against_verified_did_not_payload_did() {
        let source = production_source();
        let production = source.as_str();

        // The validate_invite call sits inside handle_claim_invite. After
        // the C5 fix the fourth positional argument (`claimer_did`) is
        // sourced from the connection-bound `verified_did`, never from
        // the request payload.
        let bytes = production.as_bytes();
        let call_marker = b"invite_tokens::validate_invite(";
        let mut found_correct = false;
        let mut idx = 0;
        while let Some(pos) = bytes
            .windows(call_marker.len())
            .skip(idx)
            .position(|w| w == call_marker)
        {
            let abs = idx + pos;
            idx = abs + call_marker.len();
            // Scan the next ~400 bytes — enough for the multiline call
            // expression to include the claimer argument.
            let end = (abs + 400).min(bytes.len());
            let window = std::str::from_utf8(&bytes[abs..end]).unwrap_or("");
            if window.contains("verified_did") && !window.contains("&did,\n") {
                found_correct = true;
                break;
            }
        }

        assert!(
            found_correct,
            "invite_tokens::validate_invite(…) inside handle_claim_invite must \
             pass `verified_did` (the connection-bound DID), not the payload \
             `did` field. Today the call site uses `&did` (payload-supplied), \
             which lets a peer claim any token by spoofing the `did` field. \
             See plan §4.2 scenarios 1+2 and §5.5 commit 5."
        );
    }
}

#[cfg(test)]
mod claim_invite_credential_binding_tests {
    //! W0 Part A regression guard: the ClaimInvite path must feed the
    //! connection-verified DID into `MlsManager::add_member` as the
    //! `expected_did` the KeyPackage credential is checked against.
    //!
    //! `add_member` rejects a KeyPackage whose BasicCredential DID ≠
    //! `expected_did` — the rejection itself is proven behaviourally by
    //! `mls::manager::tests::add_member_rejects_credential_did_mismatch`.
    //! That defence only protects the claim path if the leader passes the
    //! *cryptographically verified* DID (not a payload-supplied or otherwise
    //! attacker-influenced value) as `expected_did`. Were the wiring wrong, a
    //! peer could claim with a KeyPackage naming any DID and the credential
    //! check would rubber-stamp it.
    //!
    //! Source-text assertion, matching the precedent set by
    //! `claim_invite_did_binding_tests` above: `handle_claim_invite` needs a
    //! full `LeaderState` (iroh endpoint, MLS provider, HLC, tokio, SQLite +
    //! triggers) to run, so the end-to-end rejection is covered in
    //! `haex-e2e-tests`; here we pin the wiring that connects the credential
    //! check to the connection-bound identity.

    use super::production_source;

    /// The `did` bound inside `handle_claim_invite` must derive from the
    /// connection-verified DID, and that same `did` must be the `expected_did`
    /// argument passed to `mls::blocking::add_member`.
    #[test]
    fn claim_invite_passes_verified_did_as_add_member_expected_did() {
        let source = production_source();
        let production = source.as_str();

        let fn_start = production
            .find("pub async fn handle_claim_invite(")
            .expect("handle_claim_invite must exist");
        let body = &production[fn_start..];

        // The claimant DID used downstream is the connection-bound one.
        assert!(
            body.contains("let did: String = verified_did.to_string();"),
            "handle_claim_invite must bind the claimant `did` from \
             `verified_did` (the quic_did_auth connection identity), not from \
             the request payload. See W0 plan §Part A."
        );

        // That verified DID must be the value add_member checks the KeyPackage
        // credential against. Scan only the add_member(...) call expression so
        // unrelated `did.clone()` uses elsewhere in the fn can't mask a
        // regression here.
        let add_member_pos = body
            .find("blocking::add_member(")
            .expect("handle_claim_invite must call mls::blocking::add_member");
        let after = &body[add_member_pos..];
        let call_end = after.find(".await").unwrap_or(after.len().min(400));
        let call = &after[..call_end];
        assert!(
            call.contains("did.clone()"),
            "the add_member(...) call in handle_claim_invite must pass the \
             connection-verified `did` as its `expected_did` argument, so the \
             KeyPackage credential is checked against the cryptographically \
             bound identity. Without this, W0 Part A's credential-DID check is \
             fed an unverified value on the claim path. Found call:\n{call}"
        );
    }
}

#[cfg(test)]
mod dispatch_variant_exhaustiveness_tests {
    //! Compile-time guard: every `Request` variant must have a dispatch arm.
    //!
    //! The body of `handle_delivery_request` is a `match request { … }` over
    //! the protocol's `Request` enum. If a new variant lands without a
    //! corresponding arm, the production match would catch it (the compiler
    //! enforces exhaustiveness on non-`#[non_exhaustive]` enums) — *but* the
    //! production file already participates in dozens of test-helpers and the
    //! signal-to-noise of a fresh dev seeing a new variant fail in the middle
    //! of a long compile session is poor. This module is a tight,
    //! purpose-built compile-time canary: 16 named arms, one location, easy
    //! to spot in CI output.
    //!
    //! The `_exhaustive` fn is `#[allow(dead_code)]` because the test body
    //! never *calls* it — its only job is to be type-checked. If a variant
    //! is removed from the enum (or renamed), this fn fails to compile; if a
    //! variant is added, the match below stops being exhaustive and *also*
    //! fails to compile. Either way, the next dispatcher author sees the
    //! signal here before they ship.
    //!
    //! Static-source assertions (above) cover *order* (gate before match);
    //! this one covers *coverage* (every variant routed somewhere).
    //! Source-text alone can't pin coverage because a variant could be
    //! renamed in both the enum and one stale match arm while a *new* variant
    //! is forgotten — the strings would match, the compile would fail.

    use crate::space_delivery::local::protocol::Request;

    #[allow(dead_code)]
    fn _exhaustive(r: &Request) {
        // Mirror the production dispatcher's variant set. The compiler
        // refuses to compile this match if `Request` gains a variant we
        // didn't list — that is the entire test.
        match r {
            Request::MlsUploadKeyPackages { .. } => {}
            Request::MlsFetchKeyPackage { .. } => {}
            Request::MlsSendMessage { .. } => {}
            Request::MlsFetchMessages { .. } => {}
            Request::MlsSendWelcome { .. } => {}
            Request::MlsFetchWelcomes { .. } => {}
            Request::MlsAckCommit { .. } => {}
            Request::MlsKeyPackageCount { .. } => {}
            Request::RequestRejoin { .. } => {}
            Request::SubmitExternalCommit { .. } => {}
            Request::SyncPush { .. } => {}
            Request::SyncPull { .. } => {}
            Request::SyncPullColumns { .. } => {}
            Request::Announce { .. } => {}
            Request::ClaimInvite { .. } => {}
            Request::PushInvite { .. } => {}
        }
    }

    /// Cross-reference: every Request variant the protocol exposes must have
    /// a dispatch arm string somewhere in `dispatch.rs`. This catches the
    /// inverse failure mode of `_exhaustive` above — if the enum stops
    /// listing a variant but the dispatcher still has an arm for it (or vice
    /// versa), the static-source assertion in `auth_gate_wireup_tests` would
    /// pass but coverage would be skew. We pin the count + a per-variant
    /// substring presence check.
    #[test]
    fn dispatch_rs_contains_an_arm_for_every_request_variant() {
        let dispatch = include_str!("dispatch.rs");

        // The variant arms are written as `Request::Foo { … }` (or
        // `req @ Request::Foo { … }` for ClaimInvite). Searching for the
        // qualified prefix is robust against the destructuring shape.
        let variants = [
            "Request::MlsUploadKeyPackages",
            "Request::MlsFetchKeyPackage",
            "Request::MlsSendMessage",
            "Request::MlsFetchMessages",
            "Request::MlsSendWelcome",
            "Request::MlsFetchWelcomes",
            "Request::MlsAckCommit",
            "Request::MlsKeyPackageCount",
            "Request::RequestRejoin",
            "Request::SubmitExternalCommit",
            "Request::SyncPush",
            "Request::SyncPull",
            "Request::SyncPullColumns",
            "Request::Announce",
            "Request::ClaimInvite",
            "Request::PushInvite",
        ];

        for v in variants {
            assert!(
                dispatch.contains(v),
                "dispatch.rs is missing an arm for {v} — either the variant \
                 was removed from the enum (then update this test) or the \
                 dispatcher dropped the arm (then restore it). Either way, \
                 the compile-time match in `_exhaustive` above and the \
                 production match in `handle_delivery_request` MUST agree."
            );
        }
    }
}
