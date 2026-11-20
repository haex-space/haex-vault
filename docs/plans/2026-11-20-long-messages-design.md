# Long chat messages: design

Date: 2026-11-20. Amends spec 072 (FR-012, research R2) and, in a second PR, FR-034 / R14.

## Problem

A chat text is limited to 4,000 characters (`envelope::MAX_TEXT_CHARS`, mirrored in
`usePeerConversation.ts`). Operator: far too little, at least 100,000 characters. The limit also
applies to notes ("Ich", `send.rs` calls `checked_text`), although a note never leaves the vault.

## Why the limit exists

A message is one Nostr event. The gift wrap must stay ≤ 60 KiB (relays commonly cap at 64 KiB), the
rumor ≤ 24 KiB, and the ratchet plaintext ≤ 20 KiB (spec 095). 100,000 characters are 100–400 KB
and cannot travel in one event.

## Decisions

1. **Inline or file by actual size, not by a constant.** The send command builds the envelope with
   the real own card and a trial wrap, exactly as `send::message` does. If it fits, the message
   goes as today, without the 4,000 limit. If not, the text goes as the file `message.txt`
   (`text/plain`, UTF-8) over the existing Blossom path, with a preview as `text` (first ~1,000
   characters, cut at a word boundary).
2. **Upper bound 1,000,000 characters** for any text (`chat.error.tooLong` above). The frontend has
   no other fixed limit; the counter shows only near the bound.
3. **Notes** need no file: the text is stored in the row and syncs to own devices like any note.
   Only the upper bound applies. Counter and send lock in the frontend depend on the conversation
   kind.
4. **Marker.** `FileAttachment` gets `inline: bool` (default `false`, skipped when `false`), additive
   in envelope v1. The receiver honours it only for `text/plain` and ≤ 4 MB; otherwise the
   attachment is an ordinary file. A device without the feature shows preview plus a plain
   attachment.
5. **Display: collapsed with "Weiterlesen".** Every long text (inline, file-backed, note) is clamped
   to ~12 lines; overflow shows "Weiterlesen", expanded shows "Weniger anzeigen" (state per view,
   not stored). A file-backed message always offers "Weiterlesen (84 KB)": a click reads the full
   text from the local chat file store through the new command `chat_attachment_text` (only for
   `inline` attachments, not a general file reader). The sender has the text at once (own copy is
   stored on send). The receiver downloads it automatically (≈ 400 KB, far below the 25 MB auto
   limit); until then the button shows progress, on failure the reason and "Erneut versuchen", for
   `gone` the preview stays with "Text nicht mehr verfügbar".
6. **Without a Blossom server** only the file case fails (`chat.file.noServer` with a hint to the
   settings); the draft stays in the field. A failed upload retries with the same idempotency key.
7. **Database.** `chat_attachments` gets a column `inline`; `ChatPeerAttachment` (ts-rs) gets
   `inline: boolean`. New `#[ts(export)]` names keep the domain prefix.

## Tests

Rust tests in `*_tests.rs` next to the module: inline-versus-file boundary (4-byte characters, text
full of quotes and newlines that double when escaped), upper bound, note path, preview cut,
marker ignored for wrong type or size, two-vault test (receiver sees the preview, then the text).
Frontend: check script for clamp and "Weiterlesen"; all `check:*` and `typecheck:scripts`.

## Split

1. **PR 1, this design:** spec 072 amendment (FR-012, R2), backend, `chat_attachment_text`, display.
2. **PR 2, files up to 200 MB:** `crypto::encrypt`, `blossom::upload`, `store::put`, the download
   and `picked::read` work on segments instead of whole files in memory (the `ponytail:` upgrade of
   R14); then `MAX_PLAINTEXT` becomes 200 MB for all platforms. A lower server limit still wins
   (FR-034). Spec 072 FR-034 and R14 change first.

## Not in scope: finding own devices in the LAN without internet

Own devices find each other only through presence on Nostr relays; the iroh endpoint has only a
`MemoryLookup`. Spec 024 R7 rejected mDNS ("the iroh relays cover the local network too"), which
holds only with internet or own relays. Offline LAN sync needs its own spec that reverses R7: mDNS
announcement, sync started without presence, and a check of the claim in spec 056's constitution
check ("own devices in the same network keep finding each other without a Nostr relay"), for which
no mechanism exists today.
