-- W4 PR-3 Task 8b: `haex_ucan_tokens.capability` -> `capabilities`.
--
-- The column changes meaning as well as name: it used to be a decomposed
-- hierarchical string (`space/write`), one row per single capability grant.
-- It now stores a serialized `CapabilitySet` (canonical-sorted JSON array of
-- `{cap, delegatable}` entries) — symmetric with the wire form introduced by
-- Tasks 2/4/8 for the UCAN payload's `cap` map value.
--
-- Existing rows CANNOT be parsed by the new code (they carry a bare `space/x`
-- string, not JSON). UCAN tokens are always re-requestable via P2P
-- Announce/Claim, so we drop the cache instead of writing a data-migration
-- path that would silently corrupt sets on legacy rows.
DELETE FROM haex_ucan_tokens;
--> statement-breakpoint
ALTER TABLE haex_ucan_tokens RENAME COLUMN capability TO capabilities;
