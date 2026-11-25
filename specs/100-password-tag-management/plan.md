# Implementation Plan: Tag management by selection, with merge

**Branch**: `feat/100-tag-management` | **Date**: 2026-11-25 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/100-password-tag-management/spec.md`

## Summary

The tag manager of the password manager (spec 034 FR-011) shows one row per tag. It becomes one
searchable dropdown plus the editor of the chosen tag (rename, color, delete, entry count per app), and
gains "merge into another tag". Tags are shared with Wallet, so the count is shown per app (password
manager icon + number, Wallet icon + number) and the manager also opens from Wallet's header. The merge is one new backend operation that reuses the link-moving step
`reconcile_tags` already has (extracted, not copied), runs in one write transaction, and is refused for
every caller but the user. No schema change, no migration. A second small read tells the merge
confirmation which agent profiles name the two tags (FR-010).

## Technical Context

**Language/Version**: Rust (Tauri 2 backend, `haex-crdt` writes), TypeScript + Vue 3 / Nuxt (`src/`).

**Primary Dependencies**: existing only: `haex-ui` layer (`Shadcn*` Popover and Command, `UiDrawerModal`,
`UiButton`), `vue-sonner`, `vue-i18n`, `ts-rs` bindings. No new dependency.

**Storage**: `haex_passwords_tags`, `haex_passwords_item_tags` (CRDT tables, synced). Unchanged.

**Testing**: `cargo test` unit tests in `src-tauri/src/passwords/tags_tests.rs` (separate test file);
Node check where logic is pure; one new e2e scenario `scripts/e2e/scenarios/passwords-tags.test.ts`
and an update of `passwords-narrow-window.test.ts` (its test ids change). E2E runs on `main` CI only.

**Target Platform**: Linux, Windows, macOS, Android (iOS deferred, not blocking).

**Project Type**: desktop + mobile app (Tauri).

**Performance Goals**: a tag with a few hundred links merges in one write transaction; the dropdown
filters 500 tags without perceptible delay.

**Constraints**: all or nothing; every removed row gets its own delete marker (remote delete applies
without foreign keys); only `Caller::User`.

**Scale/Scope**: up to a few hundred tags, up to a few thousand links.

## Constitution Check

_GATE: passes before Phase 0 and after Phase 1._

| Rule                                                      | Status                                                                                                                                                                                     |
| --------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Author in a repository-local worktree on a topic branch   | `.worktrees/tag-management`, `feat/100-tag-management`                                                                                                                                     |
| Graphify before authoring a named artifact                | Queried. Candidate: `reconcile_tags` (extend: extract the link move). No searchable select exists in `src/` (only `ColorSelect`, the header tag menu): `TagPicker` is clearly independent. |
| One implementation per rule                               | The link move exists once, in `tags.rs`, used by `reconcile_tags` and `merge_tags`.                                                                                                        |
| Test code in separate files, tests with behaviour         | Rust tests in `tags_tests.rs` and a new `tag_references_tests.rs`; e2e scenario files.                                                                                                     |
| Files under 500 lines                                     | `tags.rs` ~290 → ~350; `TagManager.vue` 150 → split into `TagManager`, `TagPicker`, `TagMergeDialog`.                                                                                      |
| UI feedback: toast for failures, wrong input under field  | Kept as in `TagManager.vue` today; a merge shows no success text, the changed state is the confirmation.                                                                                   |
| Every device and OS                                       | Popover + Command work on touch; narrow-window e2e covers it; Android emulator scenario runs the same file.                                                                                |
| Spec files English, texts via i18n (de + en)              | Yes.                                                                                                                                                                                       |
| Conventional commits, no agent attribution, PR, no squash | Yes.                                                                                                                                                                                       |
| Prettier on every touched Markdown/JSON                   | Run before commit (`nix develop --command pnpm exec prettier --write`).                                                                                                                    |

## Project Structure

### Documentation (this feature)

```text
specs/100-password-tag-management/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   └── tauri-commands.md
└── tasks.md             # speckit-tasks, not created here
```

### Source Code (repository root)

```text
src-tauri/src/passwords/
├── tags.rs                      # + merge_tags, move_links (extracted from reconcile_tags)
├── tags_tests.rs                # + merge tests
├── service/organize.rs          # + merge_tags (require_user)
├── commands/organize.rs         # + MergeTagsArgs, passwords_merge_tags, passwords_tag_agent_profiles
└── …
src-tauri/src/agent_profiles/
└── tag_references.rs            # profiles whose password rights name a tag (read)
src-tauri/src/lib.rs             # register the two commands
src/types/bindings/              # MergeTagsArgs.ts (generated)
src/composables/usePasswords.ts  # + mergeTagsAsync, tagAgentProfilesAsync
src/components/passwords/
├── TagManager.vue               # dialog: picker, editor of the chosen tag, delete
├── TagPicker.vue                # new: searchable dropdown (Popover + Command) over displayTags
├── TagUsage.vue                 # new: the two icon + number badges of a tag
└── TagMergeDialog.vue           # new: target picker + confirmation
src/lib/passwords/scope.ts       # + tagUsageOf(ids, headers) -> { login, pass } (pure)
scripts/check-wallet-scope.ts    # + tagUsageOf cases
src/lib/wallet/header.ts         # + 'tags' button and command
src/composables/useWalletHeader.ts, src/components/apps/WalletApp.vue   # open the manager
scripts/check-wallet-header.ts   # + the new control
src/i18n/locales/{de,en}.json    # passwords.tags.*
scripts/e2e/scenarios/passwords-tags.test.ts        # new
scripts/e2e/scenarios/passwords-narrow-window.test.ts  # ids
specs/034-password-manager/spec.md                   # FR-011 note
```

**Structure Decision**: the feature stays inside the password manager's existing frontend components
and Rust module. The only new Rust file is the read of agent-profile references, because it reads the
tables of `agent_profiles`, not of `passwords`.

## Complexity Tracking

No constitution violations. One scope reduction against the spec is recorded in research R2 (FR-008).
