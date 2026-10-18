# File browser and viewer

**Status**: Design agreed with the operator on 2026-10-07; to be specified as spec 044.
Reuses what haex-vault 2.x (Legacy) already solved (media streaming, range reads, PDF viewer, search walk);
haex-files is obsolete and no reference.

## Context

holzi gets a built-in file browser for the whole file system of the device and for the S3
storages of [spec 038](../../specs/038-storage-connections/spec.md). It is the base for later
work agreed in the same session:

| Spec      | Topic                                                                                                               |
| --------- | ------------------------------------------------------------------------------------------------------------------- |
| 044       | This design: file browser and viewer                                                                                |
| 045       | Sync rules (replaces 025): device ↔ device in plaintext, cloud with optional encryption, one-/two-way, delete modes |
| 046       | Background service (Android and desktop tray): file sync and notifications                                          |
| 027 / 029 | Rework: a space is a frame of entries (folders or single files) with rights per member and entry                    |
| later     | Content search with embeddings (needs its own index)                                                                |

## Decisions

| #   | Question                        | Decision                                                                                                                                                                                                                                                                                                                                                         |
| --- | ------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | Built-in or extension?          | Built-in app, next to Chat, Passwords and Settings.                                                                                                                                                                                                                                                                                                              |
| 2   | Agent access                    | Agents (built-in chat agent, external agents over MCP) may list, search and read the whole file system. Writing goes through the approval gate. holzi's own data (vault files, keys, config, bundles) stays blocked for agents. Accepted risk: with a cloud model, whatever the agent reads leaves the device, and a crafted file can steer it to read `~/.ssh`. |
| 3   | What the user can do            | A file manager: browse, view and play, create folder, rename, copy, move, delete, drag & drop from the system into holzi.                                                                                                                                                                                                                                        |
| 4   | Sources in v1                   | This device and the S3 storages of spec 038. Other own devices come later.                                                                                                                                                                                                                                                                                       |
| 5   | Search                          | Live walk from the current folder, no index. Name (fuzzy), type, size, date. An index comes with the embeddings spec.                                                                                                                                                                                                                                            |
| 6   | Formats the webview can't play  | Info view with "open with system app". PDF through pdf.js (WebKitGTK shows no PDFs). No ffmpeg, no HEIC conversion.                                                                                                                                                                                                                                              |
| 7   | Android file access             | `MANAGE_EXTERNAL_STORAGE` ("all files access"): real paths, one code path with desktop. Play Store is out of scope ([spec 043](../../specs/043-android-build/spec.md)). haex-vault 2.x's SAF/content-URI path is not taken.                                                                                                                                      |
| 8   | How content reaches the webview | A loopback HTTP server with opaque tokens (haex-vault 2.x `media_server`). Not `asset://`, not a custom scheme: WebKitGTK's GStreamer backend refuses custom schemes for media and `asset://` has no range support.                                                                                                                                              |

## Architecture

**Rust, new module `files/`**

- **`FilesService`** is the only place that reads or changes files: list, stat, search, create
  folder, rename, copy, move, delete, register content. Every call carries a **caller** fixed by
  its entrance, as in [ADR 0007](../adr/0007-secrets-in-vault-db-protected-by-grants.md):
  `User` (window), `BuiltinAgent` (chat), `McpAgent(id)` (spec 021).
- **Block check** (`files/access.rs`, a pure module like `passwords/access.rs`): always on the
  real target after resolving `..` and symlinks. holzi's own data is blocked for agents; the user
  sees it read-only.
- **Sources** behind a `FileSource` trait: `LocalSource` (device file system, Android with real
  paths) and `S3Source` (storages of 038; folders are key prefixes; credentials stay in Rust).
- **`StreamingSource`** (`size`, `read_range`) taken over from haex-vault 2.x, one implementation per
  source. The 038 S3 client gains ranged GET.
- **`MediaServer`** on `127.0.0.1` with a random port. It serves only registered files under a
  random token; anything else is 404. It serves media, images, PDFs and thumbnails. **A file is
  never loaded fully into RAM.**
- **Thumbnails** are scaled in Rust (`image` crate) and cached per device (ADR 0001, never
  synced), keyed by source, path, size and mtime. Videos get an icon.
- **Search** walks breadth-first in Rust, streams hits as events and is cancellable.

**Frontend**

- `FilesApp.vue`: list and grid with virtual scrolling, breadcrumbs, sidebar (known places,
  drives, S3 storages), viewer, transfer bar. Tabs like the other apps.
- The viewer only receives token URLs. Text is read directly, PDF through pdf.js, images,
  video and audio through native elements, everything else gets the info view.

**Agent actions** (catalog in `src/lib/actions/`, [ADR 0006](../adr/0006-actions-as-builtin-agent-tools.md)):

| Action                                                   | Effect        | Gate   |
| -------------------------------------------------------- | ------------- | ------ |
| `files.list`, `files.stat`, `files.search`, `files.read` | `read`        | Safe   |
| `files.folder.create`, `files.copy`, `files.rename`      | `write`       | Change |
| `files.move`, `files.delete`                             | `destructive` | Risky  |

`files.read` returns text up to a size limit. The `files.*` actions run **in Rust**, not through
the frontend, so they work without a running webview. This departs from ADR 0006 and needs a
short ADR of its own.

## Data flow

- **Navigate**: `files_list(source, path)` returns all entries (name, kind, size, mtime, MIME).
  The open local folder is watched (`notify`); S3 has a refresh button and reloads on tab focus.
  Each tab has its own path; session restore (spec 022) keeps source and path. View, sort and
  "show hidden files" (default off) are device preferences.
- **Thumbnails**: requested only for visible entries, generated on demand. On S3 the image is
  fetched once; above a limit (about 50 MB) only an icon.
- **Open**: `files_open` checks access, registers the file with the `MediaServer` and returns kind
  and token URL. **Share URLs** (the token URLs) belong to the tab: dropped when the viewer or tab
  closes, all of them when the vault locks. (haex-vault 2.x keeps them until the app exits.)
- **Change**: copy and move work across sources (local → S3 upload, S3 → local download), streamed
  with progress events and cancellable, listed in a transfer bar. Name clash: replace, keep both,
  or skip, with "apply to all". Delete: system trash on desktop (`trash` crate); on Android and on
  S3 permanent after confirmation. Cut/copy/paste inside holzi. Drag & drop from the system into
  holzi; dragging out of holzi needs a plugin and comes later.
- **Search**: typing starts `files_search` from the current folder; hits arrive as events; new
  input cancels the old search. Filters for type (image, video, audio, document, text), size and
  date apply to search results and to the open folder. On S3 the search walks prefix by prefix and
  shows progress.

## Errors and edge cases

- **No OS permission** on an entry: lock icon, "no access" on open; no crash, no error flood.
- **Android without all-files access**: explanation and a button to the system settings instead of
  the list; checked again on return.
- **Blocked holzi data via an agent**: tool error `files_blocked` with the reason, never content.
- **Open folder disappears**: jump to the nearest existing parent with a notice. **Drive removed**:
  gone from the sidebar, transfers to it abort with the reason.
- **Symlinks** are marked and followed on open; search does **not** follow them (no loops).
- **Transfers** never leave a half target: write to a temp file beside it and rename at the end;
  abort the S3 multipart upload. S3 network errors retry up to three times with backoff, then show
  "retry" in the transfer bar. Free space is checked before the start where it is known (local).
  Copying a folder into itself is refused before the start.
- **Viewer**: the media element's `error` event → info view. Text above 5 MB → first 5 MB with a
  notice; binary → info view. A broken image gets an icon and is not retried until size or mtime
  change.
- **Agent search limits**: e.g. 30 s or 500 hits, then a partial result with `truncated: true`.

## Testing

- **Rust** (`*_tests.rs`): block check per caller and action (`..`, symlinks to vault files, case
  insensitivity on Windows and macOS, Android paths); `LocalSource` against a temp dir (list, copy,
  move, trash, name clash, copy into itself, abort without half target); `S3Source` against the 038
  test storage (`remote_storage/test_support.rs`; prefixes, ranged GET, multipart abort);
  `MediaServer` (range parser cases from haex-vault 2.x, unknown token → 404, tokens invalid after vault
  lock, flat memory reading a 1 GB file); search (cancel by new search, agent limit with
  `truncated`, symlink loop not followed); thumbnails (cache key, broken image not retried).
- **Frontend** (`scripts/check-files-*.ts`, `node --test`): browser state (navigation, restore per
  tab, filters, selection, clipboard); viewer dispatch (MIME → text/PDF/image/video/audio/info,
  media error → info); `files.*` actions and gates in `check-agent-actions.ts`.
- **E2E** (spec 016), one scene: a folder with text, PNG, PDF, MP3, MP4; browse, search, open each,
  rename, copy, delete. The MP4 plays and seeks (range request visible in the server log, the bug
  haex-vault 2.x had on Linux). The chat agent finds a file with `files.search`; `files.read` on the
  vault file ends with `files_blocked`.
- **Manual**: playback on Linux (WebKitGTK/GStreamer), Windows and Android, since codecs differ per
  system.

## haex-vault 2.x references

Repository `https://github.com/haex-space/haex-vault-legacy`, local checkout at `fc4e84b6`:

- `src-tauri/src/media_server/mod.rs`: loopback range server with tokens, and why it exists.
- `src-tauri/src/remote_storage/streaming/{source,s3_source,protocol}.rs`: `StreamingSource`, S3
  ranged reads, range header parser and its tests.
- `src/components/haex/system/files/PdfViewer.vue`: PDF viewer.
- `src/composables/useFileSearch.ts`: breadth-first search with a generation counter for cancel.
- `docs/plans/2026-06-03-unified-media-playback-and-filebrowser-refactor-design.md`: the Linux
  playback bug, the "never fully into RAM" rule, and why `useFileBrowser.ts` (1567 lines) had to be
  split; holzi keeps the logic in Rust instead.
- `docs/plans/2026-06-16-file-sync-in-file-browser-design.md`: sync badges in the browser, input
  for spec 045.
