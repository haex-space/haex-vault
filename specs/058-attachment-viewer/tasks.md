---
description: 'Task list for spec 058-attachment-viewer'
---

# Tasks: Dateianhänge im Passwortmanager anzeigen und öffnen

**Input**: Design documents from `/specs/058-attachment-viewer/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md),
[data-model.md](./data-model.md), [contracts/tauri-commands.md](./contracts/tauri-commands.md),
[quickstart.md](./quickstart.md)

**Tests**: Included. The constitution requires tests with every behaviour change, in files of their
own. Rust unit tests live in `*_tests.rs` declared as `#[cfg(test)] mod x_tests;` in
`src-tauri/src/passwords/mod.rs` (pattern of `binaries_tests`); service tests that need a vault go to
a new integration test `src-tauri/tests/passwords_attachment_view.rs` with the `fixture()` pattern of
`src-tauri/tests/passwords_roundtrip.rs`. TypeScript helpers are checked in `scripts/check-*.ts`.
Run cargo only through `nix develop --command scripts/with-nix-host-bridge.sh cargo …` and only the
affected targets with `-j 4`.

**Organization**: Grouped by user story in priority order (P1: US1, P2: US2, P3: US3). US1 is the MVP.

**Shipping note** (plan.md): PR A = Phase 1 + Phase 2 + US1; PR B = US2; PR C = US3 + Polish. B and C
only depend on A. Each PR is reviewed and merged before the next one starts on top of `main`.

**Rules for every task**: no attachment content and no path in any log or error (FR-013; file names
in logs only through `Redacted`); every new service method starts with `require_user(caller)?`
(FR-014) and no new command goes into the extension bridge or the agent tools; `binaries.rs` (382
lines) does not grow, new Rust logic goes to `src-tauri/src/passwords/attachment_view.rs`; keep
`AttachmentCard.vue` and `Attachments.vue` under 300 lines; every new UI text in `de.json` and
`en.json` (FR-015); `UiButton` inside the entry editor's form needs `type="button"`; before authoring a
new named function, component or module consult graphify (`graphify query`) and extend a candidate
if one fits (plan.md lists those evaluated).

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependency on an incomplete task)
- **[Story]**: User story from spec.md (US1–US3)

---

## Phase 1: Setup (PR A, docs)

- [x] T001 Commit spec, plan, research, data model, contract, quickstart, checklist and this file in `specs/058-attachment-viewer/` and `.specify/feature.json` (`"feature_directory": "specs/058-attachment-viewer"`)
- [x] T002 [P] Add a row to the table in `plans/README.md` after the 057 row: "058: Dateianhänge im Passwortmanager anzeigen und öffnen (`specs/058-attachment-viewer/`)", priority P2, effort M, status "Spezifiziert, geplant und in Aufgaben zerlegt 2026-10-23; Lieferungen PR A–C", dependencies "034, 036, 044 und 048 im Einsatz"
- [x] T003 [P] In `specs/036-password-redesign/spec.md` add a note under FR-039: "Geändert durch Spec 058: Text und PDF öffnen die Anhang-Ansicht; nur andere Anhänge werden beim Tippen gespeichert."

---

## Phase 2: Foundational (PR A, blocks all user stories)

**Purpose**: One rule for the kind of an attachment, the new module, and the shared text component.
No behaviour visible to the user yet.

- [x] T004 In `src/lib/passwords/format.ts` change `fileKind(fileName)`: `imageMime(fileName)` → `'image'` (unchanged), else `viewerKind(fileName)` from `~/lib/files/viewerKind` `'pdf'` → `'pdf'`, `'text'` → `'text'`, everything else → `'other'`; delete `TEXT_EXTENSIONS` (research R1, data-model "Art eines Anhangs"). Use a relative import if `scripts/check-passwords-format.ts` imports `format.ts` directly under node (as `viewerKind.ts` itself does with `../../types/bindings/ViewerKind.ts`)
- [x] T005 [P] In `scripts/check-passwords-format.ts` add cases for `fileKind`: `notiz.txt`, `README.md`, `config.yaml`, `daten.json`, `log.LOG` → `text`; `police.pdf`, `Brief.PDF` → `pdf`; `foto.jpg`, `bild.webp` → `image`; `grafik.svg`, `scan.bmp` → `other` (no preview, research R1); `tabelle.ods`, `ohne-endung`, `.bashrc` → `other`
- [x] T006 Create `src-tauri/src/passwords/attachment_view.rs` with a module doc comment (spec 058, research R2–R4) and declare `pub mod attachment_view;` plus `#[cfg(test)] mod attachment_view_tests;` in `src-tauri/src/passwords/mod.rs`; create an empty `src-tauri/src/passwords/attachment_view_tests.rs`
- [x] T007 [P] Create `src/components/files/TextContent.vue` (auto-imported as `FilesTextContent`): props `text: TextContent`; renders the truncated notice `t('files.viewer.truncated')` and the `<pre class="p-4 font-mono text-sm break-words whitespace-pre-wrap">` exactly as `src/components/files/Viewer.vue` lines 192–203 do today; keep `data-testid="files-viewer-text"` on the caller's wrapper, not inside the component
- [x] T008 In `src/components/files/Viewer.vue` replace the inline text block with `<FilesTextContent :text="text" />` inside the existing `data-testid="files-viewer-text"` wrapper; no other change (SC-003)

**Checkpoint**: `pnpm check:passwords`, `pnpm check:files` and `pnpm typecheck` green; the files app still shows text as before (`files-*` e2e scenarios unchanged).

---

## Phase 3: User Story 1 - Einen Textanhang in holzi lesen (Priority: P1) 🎯 MVP

**Goal**: A tap on a text attachment opens the attachment view with its text (spec US1, FR-001–FR-003,
FR-005–FR-008 for text, FR-013–FR-015).

**Independent Test**: Attach a text file to an entry, tap its card: the view shows the text as in the
file; nothing is written to the device (quickstart M1–M5, M8, M14, M15).

### Tests for User Story 1

- [x] T009 [P] [US1] In `src-tauri/src/passwords/attachment_view_tests.rs` test the pure part `text_of(file_name, bytes) -> Result<TextContent>`: UTF-8 text with umlauts comes back unchanged; invalid UTF-8 bytes become U+FFFD; more than `TEXT_LIMIT` bytes gives exactly `TEXT_LIMIT` bytes and `truncated = true`; a NUL in the first 8 KiB gives `InvalidInput { reason: "binary" }`; an empty file gives `text = ""`, `truncated = false`; `bild.png` and `police.pdf` give `InvalidInput { reason: "not_text" }`
- [x] T010 [P] [US1] Create `src-tauri/tests/passwords_attachment_view.rs` with a `fixture()` like `src-tauri/tests/passwords_roundtrip.rs`; test `PasswordsService::attachment_text`: a text attachment added with `binaries::add_bytes` returns its text; an unknown id gives `PasswordsNotFound`; a caller other than `Caller::User` is refused (same error as `attachment_preview` gives); an entry moved to the trash still returns the text **Note**: written as unit tests in `src-tauri/src/passwords/service/attachments_tests.rs` (`open_test_vault` + `VaultGate`, `#[tokio::test]`) instead of a new integration test binary; the trash case is left out, the backend reads attachments regardless of the trash (the read-only state is the window's).

### Implementation for User Story 1

- [x] T011 [US1] In `src-tauri/src/passwords/attachment_view.rs` add `pub fn text_of(file_name: &str, bytes: Vec<u8>) -> Result<TextContent>`: refuse unless `crate::files::kind::viewer_kind(file_name) == ViewerKind::Text` with `HolziError::InvalidInput { reason: "not_text" }`; else `crate::files::local::text::text_from(bytes, TEXT_LIMIT)` and map `FilesErrorCode::Binary` to `InvalidInput { reason: "binary" }` (any other `FilesError` to `InvalidInput { reason: "unreadable" }`), never with the content in the message (research R2, contract)
- [x] T012 [US1] In `src-tauri/src/passwords/service/attachments.rs` add `pub async fn attachment_text(&self, caller: &Caller, attachment_id: String) -> Result<TextContent>`: `require_user(caller)?`, read with `binaries::attachment_data` in `self.db().read`, then `attachment_view::text_of` inside `tauri::async_runtime::spawn_blocking` (up to 25 MiB, async must not block)
- [x] T013 [US1] In `src-tauri/src/passwords/commands/attachments.rs` add `#[tauri::command] passwords_attachment_text(state, args: AttachmentIdArgs) -> Result<TextContent>` and register it in `src-tauri/src/lib.rs` next to `passwords_attachment_preview` (import list line ~125, handler list line ~640) **Note**: no new binding for PR A; `TextContent` and `AttachmentIdArgs` exist.
- [x] T014 [US1] In `src/composables/usePasswords.ts` add `attachmentTextAsync(attachmentId)` → `invoke<TextContent>('passwords_attachment_text', { args: { attachmentId } })` and return it
- [x] T015 [US1] In `src/composables/useErrorString.ts` map the `InvalidInput` reasons `binary`, `not_text` and `unreadable` of password commands to `errors.passwords.attachment.binary` ("Diese Datei ist kein Text."), `.notText` ("Diese Datei lässt sich hier nicht anzeigen."), `.unreadable` ("Die Datei lässt sich nicht lesen."); and `PasswordsNotFound` while viewing stays the existing text. Add the keys to `src/i18n/locales/de.json` and `src/i18n/locales/en.json` **Note**: the reasons are mapped inside `AttachmentViewer.vue` (`failureText`), not globally in `useErrorString.ts`: `binary`/`not_text` are generic words other commands may send. `PasswordsNotFound` reads "not available on this device" there. Keys under `passwords.attachments.viewer.*`.
- [x] T016 [US1] Create `src/components/passwords/AttachmentViewer.vue` (`PasswordsAttachmentViewer`): props `attachment: AttachmentView | null`, `systemApps: boolean`; emits `close`, `save`, `openSystem`; a dialog panel `absolute inset-0 z-30 flex flex-col bg-background` over the entry (`role="dialog"`, `aria-label` = file name, `tabindex="-1"`, focused on mount, Escape → `close`), `data-testid="passwords-attachment-viewer"`; toolbar like `src/components/files/Viewer.vue` (close `passwords-attachment-viewer-close`, name, `lucide:download` save `passwords-attachment-viewer-save`, `lucide:external-link` open-system `passwords-attachment-viewer-open-system` only with `systemApps`); for kind `text` load with `attachmentTextAsync` (guard against a newer attachment arriving while loading) and render `<div data-testid="passwords-attachment-viewer-text"><FilesTextContent :text="text" /></div>`, an empty text shows `t('passwords.attachments.viewer.empty')`; an error shows the message from `errString` with buttons "Speichern unter" and, with `systemApps`, "Mit System-App öffnen" (`data-testid="passwords-attachment-viewer-failed"`); every `UiButton` with `type="button"`; texts under `passwords.attachments.viewer.*` in de/en **Note**: PR A ships the text view only; `systemApps`/`openSystem` come with US3 (T042), PDF with US2 (T029). The view is teleported to the closest `[data-passwords-app]` (now `relative`) so it covers the whole password manager window.
- [x] T017 [US1] In `src/components/passwords/AttachmentCard.vue` change `activate()`: `image` → `emit('open')`, `text` and `pdf` → `emit('view')` (new emit), `other` → `emit('save')`; the `aria-label` of the tap area becomes `t('passwords.attachments.view', { name })` for text and pdf; update the component doc comment (036 FR-039 as changed by 058) **Note**: in PR A only `text` emits `view`; `pdf` joins in T029 so a PDF keeps saving until it can be shown. The aria-label reuses `passwords.attachments.open` ("{name} ansehen").
- [x] T018 [US1] In `src/components/passwords/Attachments.vue` hold `viewing = ref<AttachmentView | null>(null)`; `@view` on the card sets it; render `<PasswordsAttachmentViewer v-if="viewing" :attachment="viewing" :system-apps="false" @close="closeViewer" @save="downloadAsync(viewing)" />` (system apps follow in US3); `closeViewer` clears it and focuses the card's tap button (`passwords-attachment-open-<id>`) again; when the attachment disappears from `props.attachments` (removed on another device) close the view and show `toast(t('passwords.attachments.viewer.gone'))`; give the root `relative` if the panel needs it; update the doc comment
- [x] T019 [US1] Make sure the view closes with the vault (FR-008): check that locking the vault unmounts the password manager window (spec 013); if it does not, close the view on the vault-lock event used by the other password views. Record the finding as a one-line comment in `AttachmentViewer.vue` **Finding**: closing the vault ends the process and relaunches to the unlock screen (spec 013 §Clarifications), so the view goes with it; recorded in the doc comment of `AttachmentViewer.vue`.
- [x] T020 [US1] Extend `scripts/e2e/scenarios/passwords-attachments.test.ts`: attach `notiz.txt` (a few lines incl. "Grüße") with `addAttachments`; tap its card → `passwords-attachment-viewer-text` holds "Grüße"; Escape → the viewer is gone and `document.activeElement` is the card's tap button; attach `kaputt.txt` (bytes `a\0b`) → tap → `passwords-attachment-viewer-failed`; the existing assertions (lightbox, PDF opens no lightbox) stay; at 360 px `horizontalOverflow` stays false with the viewer open

**Checkpoint**: US1 works on its own. `cargo test -j 4 --lib passwords::service::attachments`,
`cargo test -j 4 --lib passwords::attachment_view`, `pnpm check:passwords`, `pnpm typecheck`,
e2e `passwords-attachments` green. Commit, open PR A.

---

## Phase 4: User Story 2 - Ein PDF in holzi ansehen (Priority: P2)

**Goal**: A tap on a PDF attachment shows it page by page with the PDF viewer of the files app (spec
US2, FR-004, FR-006).

**Independent Test**: Attach a two-page PDF, tap its card, go to page 2, zoom (quickstart M6, M7).

### Tests for User Story 2

- [x] T021 [P] [US2] Create `src-tauri/src/files/streaming_tests.rs` (declare `#[cfg(test)] #[path = "streaming_tests.rs"] mod tests;` at the end of `src-tauri/src/files/streaming.rs`, pattern of `text.rs`): `BytesSource` reports its size and content type; `open_range(0, len)` reads all bytes; a range in the middle reads exactly that slice; a range that starts at or beyond the end reads nothing; a length past the end stops at the end
- [x] T022 [P] [US2] In `src-tauri/src/passwords/attachment_view_tests.rs` test `pdf_bytes(file_name, bytes)`: `police.pdf` and `Brief.PDF` pass the bytes through; `notiz.txt` gives `InvalidInput { reason: "not_pdf" }`
- [x] T023 [P] [US2] In `src-tauri/tests/passwords_attachment_view.rs` test `PasswordsService::attachment_pdf`: a PDF attachment returns its bytes unchanged; a text attachment gives `not_pdf`; a caller other than `Caller::User` is refused **Note**: unit tests in `src-tauri/src/passwords/service/attachments_tests.rs`, as for T010.

### Implementation for User Story 2

- [x] T024 [US2] In `src-tauri/src/files/streaming.rs` add `pub struct BytesSource { bytes: Arc<[u8]>, content_type: String }` with `pub fn new(bytes: Arc<[u8]>, content_type: impl Into<String>) -> Self` and `impl StreamingSource` (`size` = length, `open_range` = `std::io::Cursor` over the clamped slice, as `platform_probe::Fixture` does today); doc comment: a file held in memory, for attachments of the password manager (spec 058 R3)
- [x] T025 [US2] In `src-tauri/src/platform_probe.rs` remove `struct Fixture` and its `impl StreamingSource`; `serve_fixtures` registers `Arc::new(BytesSource::new(Arc::from(*bytes), *content_type))` (operator agreed 2026-10-23, plan "Vorschlag zur Erweiterung"); run the platform probe tests that cover `serve_fixtures`, if any (`grep -rn serve_fixtures src-tauri`) **Note**: `platform_probe_tests.rs` reads its ranges through `BytesSource` now; 16 probe tests pass with `--features platform-probe`.
- [x] T026 [US2] In `src-tauri/src/passwords/attachment_view.rs` add `pub fn pdf_bytes(file_name: &str, bytes: Vec<u8>) -> Result<Vec<u8>>` (refuse unless `viewer_kind == ViewerKind::Pdf` with `InvalidInput { reason: "not_pdf" }`); in `src-tauri/src/passwords/service/attachments.rs` add `pub async fn attachment_pdf(&self, caller: &Caller, attachment_id: String) -> Result<Vec<u8>>` (`require_user`, `attachment_data`, `pdf_bytes`)
- [x] T027 [US2] In `src-tauri/src/passwords/commands/attachments.rs` add `AttachmentOpenArgs { attachment_id: String, tab_id: String }` (`Deserialize`, `TS`, camelCase, exported to `src/types/bindings/`) and `#[tauri::command] passwords_attachment_open(state, media: State<'_, MediaServer>, args: AttachmentOpenArgs) -> Result<String>`: bytes from `attachment_pdf`, then `media.register(&args.tab_id, Arc::new(BytesSource::new(Arc::from(bytes), "application/pdf")))`; register the command in `src-tauri/src/lib.rs`; regenerate bindings with `pnpm generate:ts-types` (through the host bridge, trailing whitespace stripped)
- [x] T028 [US2] In `src/composables/usePasswords.ts` add `attachmentOpenAsync(attachmentId, tabId)` → `invoke<string>('passwords_attachment_open', { args: { attachmentId, tabId } })`; map reason `not_pdf` in `src/composables/useErrorString.ts` to `errors.passwords.attachment.notText` (same text as T015) **Note**: `not_pdf` is mapped in `AttachmentViewer.vue` (`failureText`) like the reasons of T015.
- [x] T029 [US2] In `src/components/passwords/AttachmentCard.vue` let `pdf` emit `view` too (and use `passwords.attachments.open` as its aria-label); in `src/components/passwords/AttachmentViewer.vue` for kind `pdf`: get the tab with `useWmTab().tabId`, `attachmentOpenAsync` → `url`; render `<LazyFilesPdfViewer :url="url" @failed="unplayable = true" />` inside `data-testid="passwords-attachment-viewer-pdf"`; on `failed` show the failed block of T016 with `t('passwords.attachments.viewer.pdfUnreadable')`; release the URL with `useFiles().releaseAsync(url)` when the attachment changes, when the view closes and in `onBeforeUnmount` (same `dropUrl` pattern as `Viewer.vue`, also for a load that ends after the view moved on) **Note**: an unreadable PDF shows `passwords.attachments.viewer.pdfUnreadable` in the failed block.
- [ ] T030 [US2] Extend `scripts/e2e/scenarios/passwords-attachments.test.ts`: tap the PDF card → `passwords-attachment-viewer-pdf` is shown and its page counter (testid of `PdfViewer.vue`) reads page 1 of N; Escape closes it; replace the old "a tap on the PDF opens no lightbox" assertion with "opens the attachment viewer, not the lightbox"

**Checkpoint**: US1 and US2 work. Targeted cargo tests (`files::streaming`, `passwords::attachment_view`, `--test passwords_attachment_view`), `pnpm typecheck`, e2e `passwords-attachments` green. Commit, open PR B.

---

## Phase 5: User Story 3 - Einen Anhang mit der System-App öffnen (Priority: P3)

**Goal**: On desktop every attachment opens with the system's app after the notice of 048; the copy
lives in holzi's folder for decrypted copies and goes on lock, close, failure and next start (spec
US3, FR-009–FR-012).

**Independent Test**: On a desktop system open an attachment with "Mit System-App öffnen": the notice
appears, after confirming the system app opens it; after locking the vault holzi's copy is gone
(quickstart M9–M13, M16).

### Tests for User Story 3

- [ ] T031 [P] [US3] In `src-tauri/src/passwords/attachment_view_tests.rs` test `copy_file_name`: `notiz.txt` unchanged; `a:b?.txt` → `a_b_.txt`; `x<y>|"*.md` → every forbidden char `_`; `../geheim` → no `/` left; `datei.` and `datei ` → trailing `_`; `CON.txt`, `nul`, `Com1.log`, `lpt9` → prefixed with `_`; `CONSOLE.txt` unchanged; an empty name → `file`; a 300-character name → at most 255 characters
- [ ] T032 [P] [US3] In `src-tauri/src/passwords/attachment_view_tests.rs` test `write_copy(root, file_name, bytes) -> Result<PathBuf>` and `remove_copy(path)` with a `tempfile::tempdir()` root: the copy lies in a new `<root>/<uuid>/` folder with the cleaned name and the exact bytes; two copies of the same name get two folders; `remove_copy` removes the uuid folder; a root that cannot be created (a file in its place) gives `InvalidInput { reason: "unwritable" }` and leaves no folder; `crate::files::encrypted::opened::sweep(root)` removes all copies
- [ ] T033 [P] [US3] In `src-tauri/tests/passwords_attachment_view.rs` test `PasswordsService::attachment_copy_for_app(caller, attachment_id, root, device, confirmed, dont_ask_again) -> Result<Option<PathBuf>>`: without `confirmed` and without the preference → `Ok(None)` and no file under `root`; `confirmed = true, dont_ask_again = false` → a copy, preference still absent; `confirmed = true, dont_ask_again = true` → a copy and `preferences` row (`PrefScope::Device(device)`, key `passwords.attachmentOpenAck`, value `"1"`); afterwards `confirmed = false` → a copy without notice; the same call with another device UUID (insert a second `known_devices` row) → `Ok(None)`; a caller other than `Caller::User` is refused before anything is read

### Implementation for User Story 3

- [ ] T034 [US3] In `src-tauri/src/files/local/edit.rs` make `FORBIDDEN` `pub(crate)`
- [ ] T035 [US3] In `src-tauri/src/passwords/attachment_view.rs` add `pub fn copy_file_name(name: &str) -> String` per research R4: `binaries::sanitize_file_name(name)`, then every char of `crate::files::local::edit::FORBIDDEN` → `_`, a trailing `.` or space → `_`, a stem (part before the first `.`) equal, ignoring case, to `CON`, `PRN`, `AUX`, `NUL`, `COM1`–`COM9` or `LPT1`–`LPT9` → `_` prefix; the result stays at most 255 characters
- [ ] T036 [US3] In `src-tauri/src/passwords/attachment_view.rs` add `pub async fn write_copy(root: &Path, file_name: &str, bytes: &[u8]) -> Result<PathBuf>` (new `<root>/<uuid>/<copy_file_name>`, `tokio::fs`, `sync_all`; on any failure remove the uuid folder and return `InvalidInput { reason: "unwritable" }`) and `pub async fn remove_copy(path: &Path)` (removes the parent uuid folder, logs a failure without the name); `pub const OPEN_ACK_KEY: &str = "passwords.attachmentOpenAck";`
- [ ] T037 [US3] In `src-tauri/src/passwords/service/attachments.rs` add `attachment_copy_for_app` with the signature of T033: `require_user`; without `confirmed` read `storage::preferences::get(q, PrefScope::Device(device), OPEN_ACK_KEY)` and return `Ok(None)` unless it is `Some("1")` — before reading any attachment bytes; with `confirmed && dont_ask_again` write the row with `storage::preferences::insert_or_update` (`PrefScope::Device(device)`); then `attachment_data` and `write_copy(root, …)`
- [ ] T038 [US3] In `src-tauri/src/passwords/commands/attachments.rs` add `AttachmentOpenSystemArgs { attachment_id, confirmed: bool, dont_ask_again: bool }` and `#[derive(Serialize, TS)] #[serde(rename_all = "camelCase")] pub enum OpenSystemOutcome { Opened, NeedsNotice }` (exported to `src/types/bindings/`), and `#[tauri::command] passwords_attachment_open_system(app, state, files: State<'_, FilesState>, args) -> Result<OpenSystemOutcome>`: under `#[cfg(mobile)]` return `InvalidInput { reason: "unsupported" }` at once; on desktop call `files.forget_on_close(state.gate())` (arms the clean-up when the files app never ran, research R4), resolve the device with `crate::device::commands::resolve_vault_device_uuid(&app, &db)`, call `attachment_copy_for_app` with `&files.opened_encrypted`; `None` → `NeedsNotice`; else `app.opener().open_path(path.to_string_lossy(), None::<&str>)`, on error `remove_copy(&path).await`, log `"passwords: the system could not open an attachment"` (no name, no path) and return `InvalidInput { reason: "no_app" }`; register in `src-tauri/src/lib.rs`; check that `FilesState::opened_encrypted` is reachable (`pub` or `pub(crate)`), else widen it; regenerate bindings (`pnpm generate:ts-types`)
- [ ] T039 [US3] In `src/composables/usePasswords.ts` add `attachmentOpenSystemAsync(attachmentId, confirmed = false, dontAskAgain = false)` → `invoke<OpenSystemOutcome>('passwords_attachment_open_system', { args: { attachmentId, confirmed, dontAskAgain } })`; map reasons `unwritable` (existing text if present, else `errors.passwords.attachment.unwritable` "Die Kopie ließ sich nicht anlegen.") and `no_app` (`errors.passwords.attachment.noApp` "Für diese Datei ist keine App installiert.") in `src/composables/useErrorString.ts`, de/en
- [ ] T040 [US3] In `src/components/passwords/Attachments.vue` add the open-system flow (~15 lines, plan "Geprüfte Kandidaten"): `const { isAndroid } = useDeviceCapabilities()`, `systemApps = computed(() => !isAndroid.value)`; `openSystemAsync(attachment)` calls `attachmentOpenSystemAsync(id)`, on `'needsNotice'` keeps the attachment as `pendingOpen` and opens `<FilesOpenSystemNotice v-model:open="noticing" @confirm="confirmOpenAsync" />`; `confirmOpenAsync(dontAskAgain)` calls `attachmentOpenSystemAsync(id, true, dontAskAgain)`; errors as `toast.error(errString(cause))`; pass `:system-apps="systemApps"` and `@open-system` to the viewer and to every card
- [ ] T041 [US3] In `src/components/passwords/AttachmentCard.vue` add a prop `systemApps?: boolean` and, when set, a `lucide:external-link` `UiButton` (`type="button"`, ghost, `icon-sm`, tooltip and aria-label `t('passwords.attachments.openSystem')`, `data-testid="passwords-attachment-open-system-<id>"`) before the download button that emits `openSystem`; texts in de/en ("Mit System-App öffnen" / "Open with system app")
- [ ] T042 [US3] In `src/components/passwords/AttachmentViewer.vue` add the prop `systemApps?: boolean` and the emit `openSystem`, pass them from `Attachments.vue`, and wire the toolbar button and the button of the failed block to `emit('openSystem')` (both only with `systemApps`)
- [ ] T043 [US3] Extend `scripts/e2e/scenarios/passwords-attachments.test.ts` (desktop only, the e2e rig runs Linux): tap `passwords-attachment-open-system-<id>` of a non-viewable attachment → `files-open-system-notice` is shown; cancel → the notice is gone and no toast shows an error; do not confirm in e2e (it would start an app on the runner), the copy and its clean-up are covered by T032/T033 and quickstart M9–M13

**Checkpoint**: All three stories work. Targeted cargo tests, `pnpm check:passwords`, `pnpm typecheck`, e2e `passwords-attachments` green; quickstart M9–M13 by hand. Commit, open PR C.

---

## Phase 6: Polish & Cross-Cutting Concerns (PR C)

- [ ] T044 [P] Grep `src-tauri/src/passwords/attachment_view.rs`, `src-tauri/src/passwords/service/attachments.rs` and `src-tauri/src/passwords/commands/attachments.rs` for `log::` and `tracing::` calls and check that none prints a file name without `Redacted`, a path or content (FR-013)
- [ ] T045 [P] For the whole repository (`src-tauri/Cargo.toml`, `package.json` scripts) run `nix develop --command scripts/with-nix-host-bridge.sh cargo fmt --manifest-path src-tauri/Cargo.toml --check`, `nix develop --command pnpm lint:all`, `nix develop --command pnpm typecheck`, `nix develop --command pnpm format:check`; fix what they report
- [ ] T046 Update the doc comments that still say a PDF or text only downloads: `binaries::preview_bytes` (binaries.rs:251), the module doc of `src-tauri/src/passwords/commands/attachments.rs`, `src/components/passwords/AttachmentLightbox.vue` header ("a PDF never does" stays true)
- [ ] T047 Update the 058 row in `plans/README.md` with the merged PRs; save the open manual checks in the row
- [ ] T048 Manual quickstart `specs/058-attachment-viewer/quickstart.md` M1–M16 on Linux, the Windows step with `a:b?.txt`, and A1–A2 on Android (operator; needs real devices)

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: none
- **Foundational (Phase 2)**: after Setup; blocks all stories
- **US1 (Phase 3)**: after Phase 2; MVP
- **US2 (Phase 4)**: after US1 (needs `AttachmentViewer.vue` and the `view` emit)
- **US3 (Phase 5)**: after US1 (needs the viewer for its toolbar button); independent of US2
- **Polish (Phase 6)**: after the stories it touches

### Within Each Story

- Tests first, they fail until the implementation lands
- Rust pure function → service method → command → composable → component → e2e

### Parallel Opportunities

- T002, T003 in parallel with T001
- T005 and T007 in parallel with T004/T006
- US1: T009 and T010 in parallel; then T011 → T012 → T013; T014/T015 in parallel with T013
- US2: T021, T022, T023 in parallel; T024 before T025 and T027
- US3: T031, T032, T033 in parallel; T034 → T035 → T036 → T037 → T038
- US2 and US3 can be built in parallel by two people once US1 is merged

## Parallel Example: User Story 1

```text
Task: "T009 unit tests for text_of in src-tauri/src/passwords/attachment_view_tests.rs"
Task: "T010 service tests for attachment_text in src-tauri/tests/passwords_attachment_view.rs"
```

## Implementation Strategy

### MVP First (User Story 1 only)

1. Phase 1 + Phase 2
2. Phase 3 (US1)
3. **Stop and validate**: quickstart M1–M5, M8, M14, M15; PR A
4. Ship: text attachments are readable in holzi

### Incremental Delivery

1. PR A: Setup + Foundational + US1 → text readable
2. PR B: US2 → PDFs readable
3. PR C: US3 + Polish → everything else opens with the system app on desktop
