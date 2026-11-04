---
description: 'Task list for spec 076-chat-sidebar'
---

# Tasks: Chat sidebar as a rail, search, and one composer for contacts

**Input**: Design documents from `/specs/076-chat-sidebar/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md),
[data-model.md](./data-model.md), [contracts/ipc.md](./contracts/ipc.md), [quickstart.md](./quickstart.md)

**Tests**: Included where the code is testable without a DOM: the Rust search (`chat::search`), the
search composable (Node harness, `check:chat-state`) and the agent-action guard. The Node harness has no
DOM, so the rail layout and the composer shell are covered by the E2E scenarios and the quickstart, and
the task says so. Rust tests live in `*_tests.rs` next to the file (`#[cfg(test)] #[path = "…_tests.rs"]
mod tests;`), never inline.

**Commands**: frontend and formatting as `nix develop --command pnpm …`. Rust on this machine: build
only the affected target, `-j 1`, inside the 10 GB scope, with `env -u CC -u CXX -u AR` (the shell exports
the NDK compilers); ask the operator before a full workspace build.

**Graphify**: before naming a new function, type, component or command, run
`graphify query "<question>" --budget 1200`. The graph of the main checkout is from 2026-10-20 and the
worktree has none (T002); research.md R6 lists the candidates already checked. Record the candidates in
each PR description.

**Organization**: one PR per slice, reviewed and merged before the next starts on top of `main`.

| PR  | Phases           | Tasks     | Content                                     |
| --- | ---------------- | --------- | ------------------------------------------- |
| A   | Phase 1, Phase 2 | T001–T011 | sidebar as a rail, header, no toolbar row   |
| B1  | Phase 3 (US3)    | T012–T017 | composer shell, contact composer, dictation |
| B2  | Phase 4 (US2)    | T018–T030 | `chat_search`, search field, message hits   |
| –   | Phase 5 (polish) | T031–T034 | checks, docs, E2E dispatch                  |

The spec and plan commits (`4ed2e7e5`, `f7cb0759`) ride on PR A.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: can run in parallel (different files, no dependency on an incomplete task)
- **[Story]**: user story from spec.md (US1–US3)

---

## Phase 1: Setup (PR A)

- [x] T001 Record the baseline in `.worktrees/076-chat-sidebar`: `nix develop --command pnpm check:chat-state`, `pnpm typecheck`, `pnpm typecheck:scripts`, `pnpm lint`, `pnpm format:check`. Note any failure that exists before the first edit in the PR description; do not fix it here.
- [x] T002 Try `graphify update .` in the worktree (constitution: bootstrap the graph when absent). If it fails or takes more than a few minutes, warn and carry on; note "graph skipped" in the PR description.

---

## Phase 2: User Story 1 — A sidebar that is always there (Priority: P1) (PR A)

**Goal**: FR-001…FR-008. Rail and expanded sidebar in every width, header with toggle, "Chats" and the new-conversation icon, no toolbar row, no logo or instance name.

**Independent Test**: quickstart section A; E2E `chat-narrow-window` (rewritten) and `chat-sidebar-rail` (new).

- [x] T003 [P] [US1] Create `src/stores/chatSidebar.ts`: a Pinia store `useChatSidebarStore` with one `collapsed` ref (default `false`) and no persistence (research R2). Follow the style of a small store in `src/stores/` (e.g. `passwordsSelection.ts`).
- [x] T004 [P] [US1] Add the strings to `src/i18n/locales/de.json` and `en.json` under `chat.sidebar`: `title` ("Chats"), `rail` (aria-label of the rail list, "Unterhaltungen"). Keep `chat.sidebar.show`/`hide` for the toggle and `chat.list.newConversation` for the new-conversation icon's label. Check the new keys against existing string groups (ts-rs/i18n collision rule).
- [x] T005 [P] [US1] Create `src/components/chat/RailItem.vue`: one round avatar button (`size-9`) with props `name`, `photo` (`string | null`), `kind` (`'agent' | 'peer' | 'requests'`), `active`, `unread` (number); renders `ContactsAvatar` for peers, the round bot icon of `AgentListItem.vue` for the agent, the `lucide:inbox` icon for requests; a `ShadcnBadge` with the count at the corner when `unread > 0`; the name as accessible name and as the tooltip, using the same tooltip mechanism as `UiButton`'s `tooltip` prop; `data-testid="chat-rail-item"`, `data-active`, `data-unread` attributes; emits `select`.
- [x] T006 [US1] Rewrite `src/components/chat/SidebarLayout.vue` per research R1 and R8: delete the toolbar row and the toggle button; add an invisible `aria-hidden` probe element (`absolute @2xl:static`, zero size) and pass it to `useSidebarFrame` as the `panel`; the visible panel (`id="chat-sidebar"`) stays in the flow, `w-14` as a rail and `@2xl:w-64` expanded; a narrow window with `menuOpen` puts the panel `absolute inset-y-0 left-0 z-20 w-64` over the content with a `w-14` placeholder keeping the content's place; back `wideHidden` by `useChatSidebarStore().collapsed` (initialize from the store, `watch` back into it); set `data-state="rail" | "expanded" | "overlay"` on the panel; the `sidebar` slot gets `run`, `collapsed` (`!visible`), `toggle`, `toggleLabel`, `toggleExpanded`; keep the Escape handling and focus the toggle (`chat-sidebar-toggle`) after Escape. `useSidebarFrame.ts` stays unchanged.
- [x] T007 [US1] Rework `src/components/chat/ThreadSidebar.vue`: remove the `instanceName` prop, the Holzi block and the wide "Neue Unterhaltung" button; add props `collapsed`, `toggleLabel`, `toggleExpanded` and emits `toggle`; header row = toggle (`lucide:panel-left`, `data-testid="chat-sidebar-toggle"`, `aria-controls="chat-sidebar"`), title `t('chat.sidebar.title')`, icon button `lucide:square-pen` with `chat.list.newConversation` as label and tooltip (`data-testid="chat-new-conversation"`); collapsed variant: the toggle and new-conversation buttons in a column, then one `RailItem` per entry of the existing `items` in the same order, plus a requests item (`kind: 'requests'`, count) when `requests.length > 0` whose click emits `toggle` and sets `showingRequests = true`; the collapsed variant is `w-14`, centered, scrolls on its own below the buttons (Edge Cases: long list). The expanded list and the requests view stay as they are. The `p-4`/`gap-4` of the old aside shrinks to what the header needs.
- [x] T008 [US1] In `src/components/apps/ChatApp.vue`, pass the new slot props to `ChatThreadSidebar` (`:collapsed`, `:toggle-label`, `:toggle-expanded`, `@toggle`), drop `:instance-name`, and remove `instancesStore` and `instanceName` if nothing else uses them (line 19 and 32; check first).
- [x] T009 [US1] Update `scripts/e2e/scenarios/chat-narrow-window.test.ts`: "shown" now means `#chat-sidebar[data-state]`; wide: toggle → `rail`, toggle → `expanded`; narrow (600 px): the panel is `rail`, the toggle opens `overlay`, Escape and the toggle close it back to `rail`; read the attribute through `instance.exec` (add a small `dataState(instance)` helper next to `isShown` in `scripts/e2e/lib/settings.ts`). `chat-sidebar-toggle` must stay displayed in every state (`scripts/e2e/lib/flows.ts:199` and `tab-content-isolation.test.ts` rely on it).
- [x] T010 [US1] Add `scripts/e2e/scenarios/chat-sidebar-rail.test.ts`: wide window, collapse; `chat-rail-item` count equals the entries of the list (agent, "Ich" at least); click the "Ich" item and assert the notes conversation opens and the item has `data-active`; the new-conversation button opens the dialog in both states; the state survives switching to another tab and back (FR-008). Register it like the other scenarios (check how `scripts/e2e/` lists them, and `check:e2e-exclusions`).
- [ ] T011 [US1] Verify and commit PR A: run quickstart A at 1000 px and 600 px (note what could not be run); `pnpm check:chat-state`, `pnpm typecheck`, `pnpm typecheck:scripts`, `pnpm lint`, `pnpm check:templates`, `pnpm format:check` (prettier on every touched md/json); then `gh workflow run ci.yml --ref 076-chat-sidebar` for the E2E scenarios (E2E runs on `main` only) and read the result before merge.

**Checkpoint**: the chat has a rail; Settings, Passwords, Contacts and Files behave as before (`useSidebarFrame` unchanged).

---

## Phase 3: User Story 3 — Contact chats with the agent's input field (Priority: P1) (PR B1)

**Goal**: FR-015…FR-018. Same input field, dictation, no agent controls.

**Independent Test**: quickstart section B1; the agent conversation's field is unchanged (FR-018, SC-006).

- [x] T012 [US3] Create `src/components/chat/ComposerShell.vue` per research R7 by extracting from `Composer.vue`: the `<form>`, the rounded box, the auto-growing `<textarea>` (`useAutoResizeTextarea`), a `controls` slot (the scrollable left area), `ChatVoiceInputControl` with `v-model:recording`, and the send / cancel buttons. Props: `inputTestId`, `sendTestId`, `placeholder`, `inputDisabled`, `sendDisabled`, `showCancel`; model `input`; emits `send`, `abort`, `transcript`; `defineExpose({ reset })`. Enter (no Shift) and submit send unless a recording runs, exactly as `Composer.vue` does now.
- [x] T013 [US3] Make `src/components/chat/Composer.vue` a wrapper of the shell: attachments, agent activity, settings popover, permission prompt and choice prompt go into the `controls` slot, `streamingMessageId || turnSetupPending` drives `showCancel`; props, emits, test ids (`chat-input`, `chat-send`) and the exposed `reset` stay as they are, so `ChatApp.vue` and `useComposer` need no change. Compare the rendered DOM of the agent field before and after in the browser (FR-018).
- [x] T014 [US3] In `src/components/chat/PeerConversation.vue`, replace `UiTextarea` and the send button with `ChatComposerShell` (`input-test-id="chat-peer-input"`, `send-test-id="chat-peer-send"`, placeholder `t('chat.peer.placeholder')`, `send-disabled` = `!canSend`, empty `controls` slot, no cancel); keep the relay hint, the error line, the counter, `submit()` with its idempotency key and the request banner. Add `onTranscript(text, autoSend)` mirroring `useComposer.onVoiceTranscript` (`draft.value = text`, then `void submit()` when `autoSend`); the send gating of `canSend` applies as for typed text. Remove `onKeydown` if the shell now handles Enter.
- [x] T015 [US3] Add the shell's strings if any are new (none are expected: reuse `chat.peer.*` and `voiceControl.*`); verify `de.json`/`en.json` have no unused key left from the old field.
- [ ] T016 [US3] Verify: quickstart B1 (type, dictate with auto-send on and off, 4,001 characters, the agent conversation unchanged); state in the PR that the Node harness has no DOM and the shell is covered by the browser run, not by an automated test.
- [x] T017 [US3] `pnpm check:chat-state`, `pnpm typecheck`, `pnpm lint`, `pnpm check:templates`, `pnpm format:check`; commit PR B1.

**Checkpoint**: contact and group conversations dictate and send like the agent's field.

---

## Phase 4: User Story 2 — Search chats (Priority: P2) (PR B2)

**Goal**: FR-009…FR-014. Name filter in the window, message hits from `chat_search`.

**Independent Test**: quickstart section B2; `cargo test chat::search`; `pnpm check:chat-state` (search composable).

### Tests first

- [x] T018 [P] [US2] Write `src-tauri/src/chat/search_tests.rs` (use the helpers of `chat::peer::test_support` and `storage::chat_messages_tests` for a vault database): a peer message in a direct thread, a group thread and the notes thread is found; a message of a thread with `state = 'request'` is not; an agent `user` and `assistant` message is found, `tool_call`, `tool_result` and `system` rows are not; `%`, `_` and `\` in the text match literally; fewer than 2 characters (after trimming, counted in scalar values) returns no hits and runs no query; 51 matches return 50 hits and `more = true`; newest first across both families; the snippet keeps multibyte characters whole at both cut points and carries ellipses; a text with no match returns an empty result. The tests fail until T019.

### Implementation

- [x] T019 [US2] Create `src-tauri/src/chat/search.rs` (declared in `src-tauri/src/chat/mod.rs`, tests via `#[cfg(test)] #[path = "search_tests.rs"] mod tests;`): ts-rs types `ChatSearchHit { thread_id, kind, message_id, at, snippet }` and `ChatSearchResult { hits, more }` (camelCase, `#[ts(export)]`, `Chat` prefix, see data-model.md); `search(q: &mut impl Query, text: &str) -> Result<ChatSearchResult>` with the two read-only queries of research R3 (peer: join `chat_threads`, `state <> 'request'`, `text IS NOT NULL`, `ORDER BY sort_at DESC`; agent: `role IN ('user','assistant')`, `ORDER BY created_at DESC`), filtering rows with Unicode lowercase matching until 51 hits, merged newest first and cut to 50; the snippet cut on char boundaries; a `ponytail:` comment noting the linear-scan performance ceiling and the upgrade path (indexed normalized column).
- [x] T020 [US2] Add the command `chat_search(state, args: { text })` in `src-tauri/src/chat/search.rs` using `active_database(&state)?` and `db.read(...)` like `chat_peer_messages_list`; register it in `src-tauri/src/lib.rs` next to the other chat commands (imports near line 82–91, handler list near 818–831). No catalog action, no MCP route (research R5).
- [x] T021 [US2] Generate the bindings with the narrowest `cargo test` that exports the two types (`-j 1`, 10 GB scope), then run `sed -i -E "s/[[:space:]]+$//" src/types/bindings/*.ts` as the CI does; make sure no binding other than `ChatSearchHit` and `ChatSearchResult` changed (`git status`).
- [x] T022 [P] [US2] Create `src/composables/useChatSearch.ts`: `text` ref; `hits`/`more`/`searching`/`failed` refs; `watchDebounced(text, run, { debounce: 250 })`; below 2 characters (trimmed) it clears the hits without invoking; a result that arrives after `text` changed is dropped (compare a request counter); `invoke<ChatSearchResult>('chat_search', { args: { text } })`; an error of the current text goes to an `onError` callback, which the sidebar shows as a toast (`errString`); `clear()` resets everything. Plain `invoke` wrapper style of `useConversationCommands`.
- [x] T023 [P] [US2] Create `scripts/check-chat-search.ts` and add it to the `check:chat-state` script in `package.json` (CI already runs that script): the composable with a faked `invoke` — one character does not invoke; two do, once after the debounce for a burst of keystrokes; a slow earlier result never replaces a newer one; `clear()` empties the hits; an error is reported once and the hits stay empty. Use the sandbox style of `scripts/lib/chat-state-harness.ts` only if the composable needs auto-imports; otherwise import it directly.
- [x] T024 [P] [US2] Add the strings to `de.json`/`en.json` under `chat.search`: `placeholder` ("Chats durchsuchen"), `noNames` ("Keine Unterhaltung gefunden"), `messages` ("Nachrichten"), `noMessages` ("Keine Nachricht gefunden"), `more` ("Weitere Treffer vorhanden – Suchbegriff eingrenzen"), `error`; check against existing groups.
- [x] T025 [P] [US2] Create `src/components/chat/SearchHits.vue`: props `hits`, a `resolve(hit)` function giving `{ name, photo, isAgent }`, `more`; renders the "Nachrichten" section with one button per hit (`data-testid="chat-search-hit"`): avatar (reuse `ContactsAvatar` / the agent circle), conversation name, the snippet with the matched text emphasised (compare case-insensitively on the window side), `listTime` of `at`; emits `select(hit)`; shows `more` when set.
- [x] T026 [US2] In `src/components/chat/ThreadSidebar.vue` (expanded variant only, FR-009, spec scenario 6): add the search field under the header (`UiInput` with `:prepend-icon`, `clearable`, `type="search"`, `data-testid="chat-sidebar-search"`, Escape clears — as in `contacts/Toolbar.vue`), bound to `useChatSearch().text`; when the text is non-empty, filter `items` by case-insensitive name (agent name `chat.agent.name`, peers by `displayName`) immediately, show `noNames` when none match, and render `SearchHits` below; hits of agent sessions resolve to the agent entry, peer hits through `peers`; emit `selectHit(hit)`; the search text stays when the sidebar collapses and expands (hold it in the sidebar component, which stays mounted).
- [x] T027 [US2] In `src/components/apps/ChatApp.vue`, handle `@select-hit` inside `run(...)`: for `kind === 'agent'` call `ui.openConversation(threadId)` (the session), otherwise `ui.openPeerConversation(threadId)`; notes (`kind === 'note'`) use `ui.openNotes`-equivalent through `openPeerConversation` with the note thread id (check what `ui.openNotes` does first and reuse it). Requests never come back as hits (T019).
- [x] T028 [US2] Extend `scripts/check-agent-actions.ts` with an assertion that no action in `ALL_ACTIONS` and no tool name from `listAgentActions` mentions `chat_search` or `chat.search` (research R5, 072 SC-010).
- [x] T029 [US2] Add the Rust command to the invariant list if one exists for window-only commands (grep for a test listing registered commands, e.g. in `src-tauri/tests/` or `lib.rs` tests); otherwise skip and say so in the PR.
- [ ] T030 [US2] Verify: `cargo test chat::search` (the T018 cases pass), `pnpm check:chat-state`, `pnpm check:agent-actions`, quickstart B2 in the app (also `%` and `_`, one character, collapse and expand with a typed search, a large vault if one is at hand for SC-003); commit PR B2.

**Checkpoint**: the sidebar finds chats by name and messages by content; agents cannot reach it.

---

## Phase 5: Polish & cross-cutting

- [ ] T031 [P] Update [specs/072-unified-chat/tasks.md](../072-unified-chat/tasks.md) T017's note ("composer … with text, attachment button and dictation") only if the shell changes what it describes; otherwise leave it. Add a "Follow-up" paragraph to this spec's [spec.md](./spec.md) Assumptions pointing at the audio-message slice once PR #755 and the later file-transfer slices have landed (no new spec number is reserved now).
- [ ] T032 Run the complete check set once on the final branch: `pnpm check:chat-state check:agent-actions check:mcp-actions check:wm-state check:wm-navigation check:templates`, `typecheck`, `typecheck:scripts`, `lint`, `format:check`, `cargo fmt --check`, `cargo clippy` for the affected target, and the Android clippy line from memory if `chat::search` gained any `cfg` branch (it should not). Report what was run and what was skipped.
- [ ] T033 PR descriptions: per slice the graphify candidates checked, the baseline failures from T001, what was verified by hand and what has no automated test (rail layout, shell); no attribution trailers, Conventional Commits, merge-commit or rebase merge only.
- [ ] T034 After each PR's E2E dispatch finishes, read the run (compare the run's SHA with the branch head, not the PR title) before reporting it green.

---

## Dependencies & Execution Order

- T001, T002 first. Phase 2 (PR A) has no prerequisite besides them; T003–T005 are parallel, T006 needs T003, T007 needs T004–T006's slot props, T008 needs T007, T009–T010 need T006–T008.
- Phase 3 (PR B1) needs PR A merged only for a clean base; it touches no sidebar file. T012 → T013 → T014 → T015–T017.
- Phase 4 (PR B2) needs PR A merged (T026 edits `ThreadSidebar.vue`) and is independent of B1. T018 first (red), T019 → T020 → T021, T022–T025 parallel after T021, T026 needs T022 and T025, T027 needs T026, T028 any time after T020.
- Phase 5 closes the series.

### Parallel examples

```text
PR A:  T003 + T004 + T005 together
PR B2: T018 first;  T022 + T023 + T024 + T025 together after T021
```

## Implementation Strategy

1. PR A alone is the MVP: the visible restructuring, no backend.
2. PR B1 next, because FR-006 of spec 072 already requires dictation in contact chats.
3. PR B2 last: the only one with Rust; red tests first.
4. Real audio messages are not scheduled here; they wait for the file transfer (spec 072 US3).
