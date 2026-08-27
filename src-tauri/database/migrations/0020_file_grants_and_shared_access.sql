-- Phase 4 Round F1: sharing schema for content-addressable file encryption.
--
-- Two new tables land here:
--
-- `haex_file_grants` records the fact "content object X is shared with
-- space Y". A grant row is paired with a sidecar file in the bucket at
-- `space-<space_id>/<hex32>.m` that carries the DEK wrapped under the
-- space's MLS epoch key. The grant row makes the fact CRDT-visible to
-- all members of that space without a bucket LIST — so the UI can render
-- "shared with: [Alpha]" instantly, and a device joining the space
-- mid-life picks up existing grants from the space CRDT stream instead
-- of a full LIST bootstrap. The bucket is still authoritative for
-- actually opening the file; a reconciliation task pairs bucket state
-- with grant rows and repairs drift in either direction.
--
-- `haex_s3_shared_access` distributes per-space scoped S3 credentials to
-- the space's members. The owner mints `ScopedCred` blobs via the
-- existing `remote_storage/iam_adapter` and encrypts each under the
-- current MLS epoch key so only members can extract them. New members
-- pick up any historic epoch's credential row through the same key
-- history that already flows `haex_mls_sync_keys`.
--
-- Both tables sync via space-scoped CRDT (see ADR 0003) — added to
-- `SPACE_SCOPED_CRDT_TABLES` in the same round.
CREATE TABLE `haex_file_grants` (
	`id` text PRIMARY KEY NOT NULL,
	`content_key` text NOT NULL,
	`space_id` text NOT NULL,
	`sidecar_key` text NOT NULL,
	`epoch` integer NOT NULL,
	`created_at` text DEFAULT (CURRENT_TIMESTAMP) NOT NULL
);
--> statement-breakpoint
CREATE UNIQUE INDEX `haex_file_grants_content_space_uniq` ON `haex_file_grants` (`content_key`, `space_id`);
--> statement-breakpoint
CREATE INDEX `haex_file_grants_content_idx` ON `haex_file_grants` (`content_key`);
--> statement-breakpoint
CREATE INDEX `haex_file_grants_space_idx` ON `haex_file_grants` (`space_id`);
--> statement-breakpoint
CREATE TABLE `haex_s3_shared_access` (
	`id` text PRIMARY KEY NOT NULL,
	`space_id` text NOT NULL,
	`backend_id` text NOT NULL,
	`member_did` text NOT NULL,
	`encrypted_cred` text NOT NULL,
	`epoch` integer NOT NULL,
	`expires_at` text,
	`created_at` text DEFAULT (CURRENT_TIMESTAMP) NOT NULL
);
--> statement-breakpoint
CREATE UNIQUE INDEX `haex_s3_shared_access_space_backend_did_uniq` ON `haex_s3_shared_access` (`space_id`, `backend_id`, `member_did`);
--> statement-breakpoint
CREATE INDEX `haex_s3_shared_access_space_idx` ON `haex_s3_shared_access` (`space_id`);
--> statement-breakpoint
CREATE INDEX `haex_s3_shared_access_member_idx` ON `haex_s3_shared_access` (`member_did`);
