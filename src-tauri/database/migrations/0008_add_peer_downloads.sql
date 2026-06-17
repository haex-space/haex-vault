-- ---------------------------------------------------------------------------
-- HAND-WRITTEN MIGRATION (do not regenerate with drizzle-kit)
-- ---------------------------------------------------------------------------
-- Creates haex_peer_downloads_no_sync — local registry of files previously
-- downloaded from peers, used to skip redundant re-downloads when the user
-- re-clicks a file in the file browser.
--
-- Lookup key: (endpoint_id, remote_path) — "have I downloaded THIS file
-- from THIS peer before?". Two peers exposing files with the same name are
-- tracked independently, so a coincidental name collision can never trigger
-- a wrong dedup hit.
--
-- Match check at lookup time (composed in Rust, not pure SQL):
--   - size matches the FileEntry the peer just sent
--   - modified matches (NULL == NULL counts as a match — some platforms
--     don't expose mtime)
--   - the local target still exists with the recorded size (filesystem
--     stat on desktop, android_fs.get_len on Android with the stored
--     content URI)
-- Any mismatch drops the row and re-downloads.
--
-- Why `_no_sync`:
--   local_path is per-device (a filesystem path on desktop, an Android
--   MediaStore content URI on mobile). CRDT-syncing it to other devices
--   would be meaningless.
-- ---------------------------------------------------------------------------

CREATE TABLE `haex_peer_downloads_no_sync` (
  `endpoint_id` text NOT NULL,
  `remote_path` text NOT NULL,
  `size` integer NOT NULL,
  `modified` integer,
  `local_path` text NOT NULL,
  `downloaded_at` text NOT NULL,
  PRIMARY KEY (`endpoint_id`, `remote_path`)
);
