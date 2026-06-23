//! Tests for [`super::super::auth_gate::authorize_request`] — the unified
//! pre-dispatch authorisation gate.
//!
//! Covers every stage of the pipeline:
//! - Stage 2a (no peer entry):     `rejects_request_without_prior_announce`
//! - Stage 2b (peer w/o UCAN):     `rejects_request_when_peer_announced_without_ucan`
//! - Stage 3 (expired UCAN):       `rejects_request_with_expired_cached_ucan`
//! - Stage 4 (audience):           `rejects_audience_mismatch`
//! - Stage 5 (capability):         `rejects_missing_capability_for_requested_space`
//! - Stage 5 (SyncPush floor):     `accepts_read_member_sync_push_at_gate_level`
//! - Stage 5 (MLS orthog., upload): `accepts_read_member_mls_upload_key_packages_at_gate_level`
//! - Stage 5 (MLS orthog., ack):    `accepts_read_member_mls_ack_commit_at_gate_level`
//! - Stage 5 (MLS orthog., msg):    `accepts_read_member_mls_send_message_at_gate_level`
//! - Stage 5 (MLS orthog., welc):   `accepts_read_member_mls_send_welcome_at_gate_level`
//! - Stage 6a (revoked):           `rejects_revoked_member`
//! - Stage 6b (DB error):          `surfaces_db_error_from_membership_check_as_explicit_error`
//! - Stage 1 (bypass):             `bypasses_claim_invite_cleanly`
//! - Happy path:                   `accepts_valid_request_from_active_member`
//!
//! Each reject test additionally verifies that the gate writes a `warn` row
//! to `haex_logs` (via `log_to_db`) with `source = Request::op_name`, so the
//! in-app log viewer keeps showing rejected requests; happy-path and bypass
//! tests verify that the gate writes no audit row when nothing is rejected.

#![cfg(test)]

mod audience_mismatch;
mod bypass;
mod capability;
mod expired_token;
mod happy_path;
mod helpers;
mod membership;
mod stage_2_announce;
