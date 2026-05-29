-- ---------------------------------------------------------------------------
-- HAND-WRITTEN MIGRATION (do not regenerate with drizzle-kit)
-- ---------------------------------------------------------------------------
-- Add `inviter_relay_url` to `haex_pending_invites`. Without this column the
-- invitee has no way to learn which relay the inviter's leader endpoint is
-- reachable on, because the leader's `haex_space_devices` row only arrives
-- via CRDT sync *after* the invitee has already connected at least once.
-- If that first ClaimInvite has to fall through mDNS / hole-punching (no
-- shared relay), the connection can take a long time or never succeed.
--
-- The column is nullable so existing rows and inviters that don't carry the
-- field stay valid. `acceptLocalInvite` falls back to the receiver-side
-- relay URL when this column is NULL.
-- ---------------------------------------------------------------------------

ALTER TABLE `haex_pending_invites` ADD COLUMN `inviter_relay_url` text;
