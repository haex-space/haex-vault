# One format for encrypted folders on storages

Status: proposed
Date: 2026-10-10

## Context

Spec 048 lets users keep single folders of an S3 storage (spec 038) encrypted, with the provider seeing
no names, no structure and no content. Two later features need the same thing: sync rules ("cloud,
encrypted", successor of 025) and the rework of spaces 027/029, where a folder is shared with other
users. If each built its own encryption, holzi would have three formats, three key schemes and three
sets of checks for one rule.

haex-vault has a per-file scheme in `file_sync/crypto` (repository
`https://github.com/haex-space/haex-vault`, revision `8dce379d94e18fcd42c3b73686a06f984ca3f574`). Its
model fits, but its format lets a provider truncate files at a block boundary, point a sidecar at
another file's content and replay an old version under the current key (spec 048 research R3).

## Decision

**holzi has one format for encrypted folders, `HXEF`, versioned, specified in
[`docs/formats/encrypted-folder-v1.md`](../formats/encrypted-folder-v1.md) with test vectors.**
Spec 048 defines version 1.

- Per-file encryption under a folder: random object names, encrypted sidecars that carry each
  entry's parent, name and metadata, content in 64 KiB XChaCha20-Poly1305 blocks bound to their
  header, index and a last-block flag.
- Three key levels: a key derived from the vault content key (`sync::content_keys`) wraps a random
  folder key in the folder head; the folder key wraps a fresh key per content object. Sharing adds a
  recipient to the head and never hands out the vault-derived key.
- The format code is pure (no I/O) and lives in one module, `files/encrypted/format`. The file browser,
  agents, extensions, sync rules and spaces all use it; none carries a second implementation.
- It is not compatible with haex-vault's `HXFE`. There is no reader for `HXFE` in holzi.

## Consequences

- Sync rules and the spaces rework start from this format and only add recipients or new object kinds
  in a later version.
- A change to the format needs a new version number, new test vectors and an update of this ADR's
  format document; readers reject unknown versions instead of guessing.
- Data that haex-vault encrypted with `HXFE` has to be downloaded with haex-vault and stored again
  through holzi.
