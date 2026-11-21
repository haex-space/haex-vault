# Chat files up to 200 MiB: design

Date: 2026-11-21. Second PR of [long messages](./2026-11-20-long-messages-design.md). Amends spec 072
FR-034 and research R7, R14.

## Problem

A chat file is limited to 100 MB (`crypto::MAX_PLAINTEXT`). Operator: at least 200 MB. The limit is
not a policy only: every step turns the whole file into a new whole buffer. From reading the code (not
measured), the peak is three copies of the file at one moment, in two places:

| Moment                                               | Alive at once                                                                                 |
| ---------------------------------------------------- | --------------------------------------------------------------------------------------------- |
| Send, upload                                         | the plaintext (`Picked.bytes`, kept for the local copy) and the ciphertext (`encrypt`): 2     |
| Send, `ChatFileStore::put` → `put_file`              | the plaintext, the sealed object `last` and the `body` it is copied into: 3                   |
| Receive, `crypto::decrypt` (inside `spawn_blocking`) | the downloaded ciphertext, `Decryptor`'s own buffer copy of it and the plaintext it builds: 3 |
| Receive, `ChatFileStore::put` → `put_file`           | the plaintext, `last` and `body`: 3                                                           |

At 200 MB that is up to 600 MB, on a phone too. The format is not the obstacle: `HXCF` v1 already seals 1 MiB segments with a per-segment nonce, and `Decryptor` already
reads segment by segment. Only the callers hold everything.

## Decisions

1. **The limit is 200 MiB** (209,715,200 bytes). A file shown as "200 MB" by a tool that counts in
   2^20 or in 10^6 passes either way. A lower server limit still wins (FR-034, `HEAD /upload`).
2. **No new format.** `HXCF` v1 stays. Both sides keep working, and this PR needs no migration and no
   trigger-version change.
3. **Send reads the file twice, from its source, and holds one segment.** Blossom signs the upload
   with `x=<sha256 of the ciphertext>` before a byte is sent, so the hash must be known first. The
   sealing is deterministic (fixed key and nonce prefix), so
   - pass 1 reads the source segment by segment, seals each and feeds the ciphertext hash, the
     plaintext hash and the length; nothing is kept,
   - `check_upload` asks the server with those values,
   - pass 2 reads and seals again into the upload body (a bounded channel, `Body::wrap_stream`) and
     into the own copy in the store,
   - pass 2 recomputes both hashes; a file that changed between the passes fails with
     `chat.file.changed` and nothing is sent.
4. **A sent file has a source, not bytes.** `Picked.bytes: Vec<u8>` becomes a source: bytes in memory
   (a long text, ≤ 4 MiB) or a picked file that is opened again for every pass (`picked::open_read`;
   an Android `content://` target opens again). Nothing but the long text is read whole.
5. **Receive writes while it downloads.** The response chunks go through the ciphertext hash and
   `Decryptor`; the plaintext goes straight into a streaming put of the store. The content object
   is complete before the sidecar is written (`put_file` already orders it so, FR-016), so the sidecar
   is written only after `Decryptor::finish` and the hash check passed. A failed check deletes the
   content object and leaves no file.
6. **A streaming put in `files::encrypted`.** `SealedParts` seals plaintext into parts and
   `files::transfer::remote_encrypted` already uploads such parts to storage for copies between
   storages. `put_file` gets a sibling that takes a reader or a push interface instead of `&[u8]`.
   This is also what spec 048 wanted for big files (`ponytail:` of R14).
7. **Timeouts follow progress.** `TRANSFER` (5 minutes for the whole request) fails a 200 MiB file at
   0.7 MB/s. Upload and download fail after 30 seconds without progress, and after
   `5 min + size / 64 KiB/s` in total. The stall limit alone would let a server that sends one byte every 29
   seconds hold the transfer open for ever; the total keeps the old protection and scales with the file.
8. **Progress for sending.** Streaming gives the bytes sent for free; the sender's `bytes_done` is
   reported like the receiver's (FR-037).
9. **Per message the sum stays bounded by the same constant** (`MAX_MESSAGE_PLAINTEXT`), now 200 MiB
   for the sum of the files of one message. Memory is no longer the reason; a message of many large
   files is a different feature.

## Memory after

One segment (1 MiB) plus the buffers of the channel (a few segments) per transfer, on send and on
receive. The semaphore of one for store writes stays: writes still take turns.

## Tests

Rust tests in `*_tests.rs`: streaming seal equals `encrypt` byte for byte; pass 2 detects a changed
source; a stalled server fails after the stall time and a slow one does not; a 3-segment file and an
empty file through send and receive; a failed check leaves no stored file; the two-vault test sends
a file above the old limit. A memory check: the process does not grow by the file's size (a 150 MiB
file through the test server).

## Split

1. Spec 072 amendment (FR-034, R7, R14, data model) and the constant.
2. `crypto`: streaming sealer.
3. `blossom`: streaming upload and download, stall timeouts.
4. `files::encrypted`: streaming put; `ChatFileStore::put_stream`.
5. `transfers` and `commands/files.rs`: sources, two passes, receive pipeline, progress.
