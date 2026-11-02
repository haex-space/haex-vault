# Implementation Plan: Chat sidebar as a rail, search, and one composer for contacts

**Branch**: `076-chat-sidebar` | **Date**: 2026-11-02 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/076-chat-sidebar/spec.md`

## Summary

Three slices, one pull request each, in this order:

1. **A (US1, FR-001…FR-008)**: the chat frame loses its toolbar row; the sidebar is always present as
   a rail (avatars) or expanded (list), in every window width. Front end only.
2. **B1 (US3, FR-015…FR-018)**: contact and group conversations use the agent composer's shell
   (text, dictation, send) without agent controls. Front end only.
3. **B2 (US2, FR-009…FR-014)**: a search field in the sidebar; names filter in the window, message
   content is found by one new read-only command `chat_search` (ordered read-only scan with Unicode
   case-insensitive matching over
   `chat_peer_messages` and `chat_messages`).

Real audio messages (the operator's second half of "Sprachnachrichten") are not planned here: they ride
on 072's file transfer (US3, PR #755 and later) and get their own spec when it lands.

## Technical Context

**Language/Version**: TypeScript / Vue 3 / Nuxt 4 (front end); Rust 2021, Tauri 2 (core)

**Primary Dependencies**: existing only (VueUse `watchDebounced`, `UiInput`, `ContactsAvatar`, ts-rs);
no new dependency

**Storage**: the vault database (SQLite, CRDT tables `chat_threads`, `chat_peer_messages`,
`chat_messages`); no schema change, no index (R3)

**Testing**: `check:*` node harness for composables and chat state (`pnpm check:chat-state`), Rust
`*_tests.rs` beside the module (`cargo test` for `chat::search`), E2E scenario
`chat-narrow-window` rewritten and a rail scenario added (E2E runs on `main` only; dispatch it for the
branch before merging)

**Target Platform**: Linux, Windows, macOS, Android (the app's targets); narrow windows are the
Android case

**Project Type**: desktop and mobile app (Tauri), front end in `src/`, core in `src-tauri/`

**Performance Goals**: name filter < 100 ms (SC-002); message hits ≤ 1 s with 50,000 messages (SC-003)

**Constraints**: the shared `useSidebarFrame` (Settings, Passwords, Contacts, Files) must not change
behavior; the search command must not be reachable by agents (R5); no `INSERT … ON CONFLICT` (not
relevant here: read only)

**Scale/Scope**: ~12 files front end for A and B1, 1 Rust module + 1 composable + 1 list component for B2

## Constitution Check

_GATE: Must pass before Phase 0 research. Re-check after Phase 1 design._

Against `.specify/memory/constitution.md` (v1.4.0) and the spaex constitution of this repository:

| Principle                                        | Status | Note                                                                                                                                                              |
| ------------------------------------------------ | ------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| I No secrets in git                              | pass   | nothing secret added                                                                                                                                              |
| II No local absolute paths                       | pass   | the docs use repo-relative paths only                                                                                                                             |
| III Identity device-independent                  | pass   | the sidebar state is per window on the device, not an identity                                                                                                    |
| IV Cross-repo refs pin revisions                 | pass   | no external references; `#755` is a PR of this repository, named for context only                                                                                 |
| V External sources opt-in                        | pass   | none                                                                                                                                                              |
| VI Self-modifying instructions review-gated      | pass   | the change to spec 072's assumptions goes through this branch's pull request                                                                                      |
| VII Relay unavailability never blocks local work | pass   | search reads the local vault only (FR-014)                                                                                                                        |
| VIII No concealment instructions                 | pass   | none                                                                                                                                                              |
| Workflow (specify → plan → tasks → implement)    | pass   | followed; PRs are merge-commit or rebase, never squash                                                                                                            |
| Phase discipline                                 | pass   | builds on shipped 072 slices; the file-transfer-dependent slice (audio messages) is deferred                                                                      |
| spaex: worktree on a topic branch                | pass   | `.worktrees/076-chat-sidebar`                                                                                                                                     |
| spaex: graphify before new artifacts             | note   | no graph in the worktree and the main checkout's graph is from 2026-10-20; queried it with `--graph` as a hint only, then read the code (R6 lists the candidates) |
| spaex: `pnpm exec prettier --write` on md/json   | todo   | run before each commit that touches `specs/`                                                                                                                      |

No violation; the Complexity Tracking table stays empty. Re-checked after Phase 1: unchanged.

## Project Structure

### Documentation (this feature)

```text
specs/076-chat-sidebar/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/ipc.md
├── checklists/requirements.md
└── tasks.md             # $speckit-tasks, not part of this command
```

### Source Code (repository root)

```text
src/
├── components/chat/
│   ├── SidebarLayout.vue        # A: no toolbar row; rail + expanded + narrow overlay
│   ├── ThreadSidebar.vue        # A: header "Chats", collapsed prop, no logo/instance name; B2: search
│   ├── RailItem.vue             # A: new, one avatar button with badge and tooltip
│   ├── SearchHits.vue           # B2: new, the "Nachrichten" section
│   ├── ComposerShell.vue        # B1: new, box + textarea + voice + send, with a controls slot
│   ├── Composer.vue             # B1: wraps the shell, agent controls in the slot
│   └── PeerConversation.vue     # B1: uses the shell
├── stores/chatSidebar.ts        # A: collapsed flag kept per window (R2)
├── composables/
│   ├── useSidebarFrame.ts       # unchanged (R1)
│   └── useChatSearch.ts         # B2: debounce, stale-result guard, invoke
├── i18n/locales/{de,en}.json    # A, B1, B2 strings
└── types/bindings/              # B2: ts-rs ChatSearchHit, ChatSearchResult
src-tauri/src/chat/
├── search.rs                    # B2: query + command
└── search_tests.rs              # B2
scripts/
├── check-chat-search.ts         # B2
└── e2e/scenarios/chat-narrow-window.test.ts (rewritten), chat-sidebar-rail.test.ts (new)
```

**Structure Decision**: the existing split of `src/` (Nuxt) and `src-tauri/` (Rust core) stays. One new
Rust module `chat::search` next to `chat::thread_commands`; peer-conversation SQL stays in
`chat::peer` style but the search joins both families, so it lives one level up.

## Complexity Tracking

_No violations to justify._
