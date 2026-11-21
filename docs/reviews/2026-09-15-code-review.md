# Full code review — 2026-09-15

Reviewed baseline: `a874bd6` (main), later merged forward to `3bc687f`; the
merge brought only a spaex atoms revision bump and left
`.spaex/constitution.md` byte-identical, so nothing here was re-scoped.
Approximately 21,000 lines of Rust and
7,200 lines of frontend source, tests and build scripts. The review covered
every Rust subsystem, the Vue pages/components and composables, persistence,
the Tauri command surface, build scripts and the i18n locale pair, with a
dedicated pass against the spaex constitution assembled at
`.spaex/constitution.md`.

This review's emphasis was the two structural NON-NEGOTIABLE clauses from
`com.github.haexmas.atoms.general-coding` — test code in files separate from
production code, and hand-maintained files over 500 LoC — alongside the usual
correctness pass. The 2026-09-13 review's deferred split plan is executed
in part here and documented in full.

## Confirmed findings and corrections

| Priority | Trigger and previous behavior                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 | Correction                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| -------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| P1       | A `send_message` naming a `threadId` with no row in `chat_threads` was accepted: `persist_send_transaction` discarded `update_thread`'s affected-row count and, with no foreign keys enforced, inserted the user and assistant rows anyway. The reply streamed normally, `list_threads` never returned the conversation, and it was gone on reload. `rename_thread` and `delete_thread` already validated existence.                                                                                                          | Look the thread up inside the same `BEGIN IMMEDIATE` transaction and return the new `PersistedSend::UnknownThread`, which `send_message` surfaces as `HaexError::NotFound`. No row is written and the idempotency key stays unreserved.                                                                                                                                                                                                               |
| P1       | `HuggingFaceModelManagement.vue` did not compile. Its tab-bar `@click` held two statements on separate lines with no `;` separator; Vue collapses a template attribute's newlines before parsing, so the SFC compiler rejected it (`Unexpected token, expected ","`). 1 of 24 templates failed to compile at `a874bd6`, meaning `pnpm generate` and `pnpm tauri:build` could not succeed. CI never caught it: there is no build job, and `vue-tsc` does not parse template expressions.                                       | Moved both multi-statement inline handlers into named functions — `selectTab` here and `onInput` in `AliasSetting.vue` — the only form Prettier and the Vue compiler agree on. All 24 templates now compile.                                                                                                                                                                                                                                          |
| P2       | `pnpm format:check` existed as a script but ran in no CI job, and the two policies it would have enforced were in direct conflict: Prettier's `semi: false` strips exactly the `;` separators an inline multi-statement Vue handler needs. `AliasSetting.vue` compiled but was Prettier-unclean (commit `7fa992c` added those semicolons on purpose); `HuggingFaceModelManagement.vue` was Prettier-clean but uncompilable. Adding the check without resolving that would have made CI demand a change that breaks the build. | Removed the conflict at its root with named handlers, then closed the CI gap from both sides: a `Check frontend formatting` step (`format:check`) and a new `Check Vue templates compile` step (`check:templates`, `scripts/check-vue-templates.ts`) that compiles every tracked SFC template with `vue/compiler-sfc`. Verified by reintroducing the original fault: `format:check`, `typecheck` and `lint` all still pass; only the new guard fails. |
| P2       | `hardware::probe()` ran directly on the async executor in five `async fn` Tauri commands. On CUDA builds it spawns `nvidia-smi` and polls it with a blocking 10 ms sleep for up to `CUDA_PROBE_TIMEOUT`, stalling a Tokio worker for ~500 ms per catalog listing or HF detail fetch.                                                                                                                                                                                                                                          | Added `hardware::probe_async`, which runs the probe on the blocking pool and falls back to an inline probe if the pool cannot accept the task, since a hardware snapshot is advisory. All five call sites moved over; the two stale "cheap enough to run inline" comments were corrected.                                                                                                                                                             |
| P2       | Two production modules carried inline `#[cfg(test)] mod tests { … }` blocks with full test bodies — `providers/mod.rs`, where the block also sat wedged between two production functions, and `instances/create.rs`.                                                                                                                                                                                                                                                                                                          | Moved to `providers/providers_tests.rs` and `instances/create_tests.rs`. `create.rs` keeps the permitted one-line `#[cfg(test)] #[path] mod tests;` declaration so its private `publish_active` and `open_new_database` stay private — the sibling-module alternative would have required widening production visibility for a test.                                                                                                                  |
| P2       | Five hand-maintained files exceeded 500 LoC with no documented exception.                                                                                                                                                                                                                                                                                                                                                                                                                                                     | Split three and documented the rest: `adapters/anthropic.rs` 580 → 407 with request construction moved to `adapters/request.rs`; `adapters/anthropic_tests.rs` 750 → 235 with `anthropic_stream_tests.rs` and `request_tests.rs`; `models/huggingface_tests.rs` 886 → 309 with `huggingface_search_tests.rs` and `huggingface_install_tests.rs`; `providers/mod.rs` 529 → 500 through the test extraction above.                                      |
| P3       | `openIntegrityDialog` — 24 lines including its explanatory comment about `HaexError`'s snake-cased hash fields — was copied verbatim into both `pages/chat/[instance].vue` and `components/models/HuggingFaceModelManagement.vue`, as was the `IntegrityDialogState` interface.                                                                                                                                                                                                                                               | Extracted as `useModels.parseModelIntegrityFailure` plus the exported `ModelIntegrityFailure` type, beside the `ModelIntegrityStatus` it narrows. Both components now hold four lines of dialog wiring.                                                                                                                                                                                                                                               |
| P3       | `model-load-error` was emitted as a bare string literal while its ten sibling events all went through an `EVENT_*` constant, so a typo would have broken the frontend listener silently.                                                                                                                                                                                                                                                                                                                                      | Added `EVENT_MODEL_LOAD_ERROR` and routed `emit_load_error` through it.                                                                                                                                                                                                                                                                                                                                                                               |
| P3       | `CONTEXT.md` documented the locale files at `src/i18n/de/*.json` and `src/i18n/en/*.json`; they actually live at `src/i18n/locales/de.json` and `src/i18n/locales/en.json`. A doc-comment in `models/paths.rs` read "returns the returns an explicit ambiguity error".                                                                                                                                                                                                                                                        | Both corrected.                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| P3       | `validate_filename` rejected a `..` substring; its sibling `validate_slug` did not. Not exploitable — both reject path separators, and no catalog or Hugging Face derived id contains `..` — but the asymmetry invited a future caller to assume the weaker guard.                                                                                                                                                                                                                                                            | Mirrored the `..` check into `validate_slug`.                                                                                                                                                                                                                                                                                                                                                                                                         |

`current_title` in `chat/commands.rs` had exactly one caller, the branch the
P1 fix rewrote, so it was removed. Its `.ok()?` also swallowed a failed
`SELECT` into a blank title; the replacement propagates that error.

## Reviewed and deliberately unchanged

- `models/paths.rs` path handling is sound: both validators reject path
  separators and leading dots, and `resolve_relative` rejects anything that is
  not exactly two segments.
- `adapters/anthropic.rs` pagination already refuses to follow a `has_more`
  cursor that does not advance, so a misbehaving provider cannot spin the loop.
- `renderMarkdown` passes `marked` output through `DOMPurify.sanitize`; the
  single `v-html` site is the sanitized result.
- All 24 SFC templates compile, now enforced by `pnpm check:templates`. That
  guard is the only CI check that sees a template-expression error: reintroducing
  the tab-handler fault leaves `format:check`, `typecheck` and `lint` green.
- No `any`, `@ts-ignore` or `@ts-expect-error` in the frontend; `strict` is on.
- The `de`/`en` locale files are at exact key parity (267 keys each). A scan of
  every `.vue` template for German diacritics in element text and in the
  `title`/`label`/`placeholder`/`aria-label` attributes found no hardcoded
  string; this catches the common case, not a German literal that happens to
  avoid umlauts.
- No `Runtime::new`, `runtime::Builder` or `block_on` anywhere in the crate.
- The remaining `expect()` calls are on internal-struct serialization with the
  invariant documented at the call site, which the constitution's `MAY` clause
  permits. The `Option` unwraps in `chat/tools/cli.rs` are guarded by the
  loop's own `is_none()` conditions.
- `instances/open.rs` holds the state mutex only across the atomic
  drop/publish, with the blocking SQLCipher open outside it.

## Maintainability exceptions and follow-up split plan

Five files remain over 500 LoC. Each now carries its reason and an ordered,
symbol-level split plan **in the file itself** — the 2026-09-13 review recorded
four of them here in `docs/reviews/`, which does not survive someone reading
the file, and `scripts/check-chat-state.ts` was not recorded at all.

- `src-tauri/src/chat/commands.rs` (2994): five mechanical PRs, in order —
  `chat/events.rs`, `chat/model_loading.rs`, `chat/send_admission.rs`,
  `chat/turn.rs`, `chat/default_model.rs` — leaving `send_message`, the abort
  commands and the permission commands behind.
- `src/pages/chat/[instance].vue` (1937), `scripts/check-chat-state.ts` (633):
  sequenced together and deliberately blocked. The harness regex-extracts this
  page's `<script setup>` block, strips every `import` line and replays what is
  left against injected globals, so anything moved into a composable becomes
  invisible to all 19 replay tests — exactly the tests covering the event
  ordering and ownership rules that make the page long. The harness must be
  reworked to import composables **first**; only then extract
  `useChatTranscript`, then `useThreadSidebar`, then the model-management
  child component.
- `src-tauri/tests/chat_tool_loop.rs` (1835): extract the ~360-line
  `StubAdapter`/`ScriptedTool`/`run_scripted_turn` fixture to
  `tests/common/tool_loop_fixture.rs` first, then split into tool-loop core,
  permission gating and retry binaries.
- `src/components/models/HuggingFaceModelManagement.vue` (598): add replay
  coverage for the installed-model tab first, then extract the
  catalog-download tab and the update-check panel.

`src-tauri/src/models/huggingface.rs` (1248) and
`src-tauri/src/models/commands.rs` (952) already carried in-file exceptions
with split plans and were left as they are.

## Verification

The `send_message` fix has a regression test in
`src-tauri/tests/chat_message_idempotency.rs` that asserts the rejection, that
no message row is left under the unknown thread, and that the idempotency key
stays unreserved. It was confirmed to fail against the pre-fix behavior:
reverting only the existence guard makes it report
`Fresh { … }` where `UnknownThread` is expected.

The splits were pure moves of whole items, so the guarantee they need is that
no case was dropped or silently duplicated: the library suite stayed at exactly
152 across both the adapter and the Hugging Face test splits, and the whole
suite went from 226 to 227 — the one added case. Test count is evidence that
nothing was lost, not independent proof of behavior; the moved code is
byte-identical apart from the import headers each new file needed.

Commands used:

```sh
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --jobs 2 -- -D warnings
cargo clippy --manifest-path src-tauri/Cargo.toml --no-default-features --all-targets --jobs 2 -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --jobs 2
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features --jobs 2
pnpm lint
pnpm typecheck
pnpm check:chat-state
pnpm check:templates
pnpm typecheck:scripts
pnpm generate
pnpm format:check
python3 scripts/ci/test_check_docs.py
python3 scripts/ci/check-docs.py
git diff --check a874bd6...HEAD
```

Results: 227 Rust tests passed with default CPU features (3 ignored — all three
need `HAEX_TEST_GGUF` pointing at an operator-provided GGUF), 214 passed
without default features, and 19 frontend replay tests passed. Rustfmt, Clippy with warnings
denied on both feature sets, ESLint, Nuxt typecheck, the documentation
regression, the documentation validator and the patch whitespace check all
passed.

A full `pnpm generate` could not be completed in the review worktree: its
`node_modules` is symlinked from the primary checkout, and the `haex-ui` layer
cached under `node_modules/.c12/` then fails to resolve its own
`@/components/shadcn/*` imports. That is a worktree artifact — the primary
checkout holds a successful `.output/` from 2026-09-14 and the layer is
unchanged — but it means the template fix is verified by `pnpm check:templates`
compiling all 24 templates and by the parse errors disappearing from the build
log, not by one green build. Whoever lands this should confirm a build in a
normal checkout.

The CI pipeline was then reworked around these findings and measured on real
runs: wall clock fell from 19m42s to 2m0s once the cargo cache was warm. The
Rust job became a matrix over the two feature sets, running in parallel; the
`Vault → model → CLI E2E` job was removed because `cargo test` already runs the
exact test it compiled 5m25s to execute; `CARGO_PROFILE_DEV_DEBUG` was set to
`line-tables-only`, which matters less for build time than for staying under
GitHub's 10 GB cache ceiling (the no-default target tree alone is 5.5 GB at the
default); and `--jobs 2`, which had halved cargo's parallelism on a 4-vCPU
runner without a stated reason, was removed. CI now also runs on pushes to
`main`, and `main`'s branch protection finally requires the four checks — it
previously required none, so nothing had ever blocked a red merge.

A `Frontend build` job was then added and proven both ways with a real
`node_modules`: `pnpm generate` exits 0 on this branch, and reintroducing the
original tab handler makes it exit 1 with exactly the parse error. The 13 extra
errors seen earlier were the symlink artifact alone. CI also now runs on pushes
to `main` — a direct push was previously unchecked entirely — with the two
diff-scoped steps skipped when there is no pull-request base, and the Rust job
gained a cargo cache.

`check:templates` is kept alongside the build: it isolates this specific class
of defect in about a second and names the offending template line, where the
build reports it only after installing and compiling everything.

CUDA/Metal hardware and a native interactive WebView were not exercised, so the
`probe_async` change is verified by its call sites and the test suite rather
than by an observed CUDA probe. The frontend checks ran on Node 22.17.0, which
is below the repository's declared engine range in `package.json`; pnpm warns
and proceeds. CI pins 22.19.0. No new dependencies were added, and no
production visibility was widened for a test.
