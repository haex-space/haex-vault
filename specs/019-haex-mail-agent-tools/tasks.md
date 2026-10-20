# Tasks: haex-mail für den Agenten, Regeln und Regel-Oberfläche

**Input**: Design documents from `/specs/019-haex-mail-agent-tools/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/host-mail-commands.md,
contracts/manifest-tools.md, quickstart.md

**Tests**: Test-first per task (write the failing check, then implement). Rust unit tests in sibling
`*_tests.rs` files (never inline `mod tests { … }`); vault-sdk and haex-mail tests in vitest next to the
module (`*.test.ts`). No new runtime dependency in holzi, vault-sdk or haex-mail.

**Conventions**: Every changed file stays under 500 lines (`imap.rs` is at 460, so the new IMAP code goes into
`imap_manage.rs`; `test_server.rs` is at 462, so the new test server code goes into `test_server_manage.rs`).
Texts in haex-mail go into the `<i18n>` blocks of the component or into `app/locales/{de,en}.json` (both
languages). Cross-repo references pin full commit SHAs (constitution IV). Commits follow Conventional Commits
without any trailer. Prettier runs on every touched Markdown and JSON file (`pnpm exec prettier --write`). Each
repository gets its own worktree on a topic branch from `origin/main`; never author in a primary checkout. Before
any cargo build run `df -h` and check the size of old `.worktrees/*/target`; ask the operator before the first
full cargo build (the machine runs out of memory under unbraked builds). Pull requests to haex-space
repositories use the operator's account rules (`gh auth switch --user haex-space`, switch back afterwards;
fall back to `gh api -X POST …/pulls` when `gh pr create` fails with the GraphQL error). Tasks marked
**operator** need an account, a running app or a decision only the operator can give.

**Repositories** (label in each description): `(holzi)` this repository, `(vault-sdk)` haex-space/vault-sdk,
`(haextension)` haex-space/haextension, app `apps/haex-mail`.

**Deliveries** (plan, research R1): E1 = T001–T002; E2a = T005–T012; E2b = T013–T016; E3 = T017–T054, T062 plus T057–T059 as three
pull requests: 3a tools (Phases 3–5), 3b rules (Phases 6–7), 3c accounts (Phase 8), each followed by Phase 9 items
that belong to it. Phase 9 outside these: T055 is its own holzi pull request (`test(chat): evaluate the mail
tools of haex-mail`), T058 runs per pull request, T060 and T061 close the spec after the last merge.

## Phase 1: Setup

- [ ] T001 (vault-sdk) Create a worktree from `origin/main`, rebase `feat/agent-tools` (head `cf2dbcb8849f8be62d71fa957dbfa43d131756ab` when planned) on it and run the repository's checks (`vitest`, type check, lint, crate tests); note the result under this task
- [ ] T002 (vault-sdk) **operator** Open the pull request `feat/agent-tools` → `main`, merge it, let the release process publish the version; note the published version (V1) and the merge SHA under this task
- [ ] T003 (haextension) Create a worktree for the topic branch `019-haex-mail-agent-tools` from `origin/main`, run `pnpm install --frozen-lockfile`, run `pnpm test` in `apps/haex-mail` once and note the baseline under this task
- [ ] T004 (holzi) **operator** Check `df -h` and `.worktrees/*/target` sizes, remove merged worktree targets if the disk is tight, then agree to one baseline run of `cargo test --manifest-path src-tauri/Cargo.toml --no-default-features --lib mail`; note the baseline under this task

---

## Phase 2: Foundational (blocks all user stories)

**Purpose**: The three host functions (E2a, E2b) and the shared base of haex-mail's tools (E3 base). US1 needs the
search, US2 needs "create mailbox", US3 needs "delete". Nothing in Phases 3 to 8 starts before T017.

### E2a — holzi: three bridge methods

- [ ] T005 (holzi) Spike for non-ASCII search text (research R2): read `async-imap`'s `uid_search` and literal support in the cargo registry source, write the first test in `src-tauri/src/extensions/mail/mail_manage_tests.rs` that sends a UTF-8 text to the test server, and replace the "Risiko" paragraph in `research.md` R2 and the matching paragraph in `contracts/host-mail-commands.md` by the outcome (literal works, or error 3001 plus local fallback) If the outcome is negative, **stop** and run `/speckit-clarify` for FR-005 and SC-004 before E2a continues, so the implementation does not drift silently from the spec.
- [ ] T006 [P] (holzi) Add `SearchCriteria` and `SearchResult` to `src-tauri/src/extensions/mail/types.rs` and `check_criteria` to `src-tauri/src/extensions/mail/mod.rs` (at least one criterion, no control characters, ≤ 200 characters, valid `YYYY-MM-DD`, error `MailError::Invalid` → 3001); cases in `mod_tests.rs` first
- [ ] T007 [P] (holzi) Add `src-tauri/src/extensions/mail/test_server_manage.rs` (SEARCH with `FROM`/`TO`/`SUBJECT`/`TEXT`/`SINCE`/`BEFORE`/`SEEN`/`UNSEEN`/`FLAGGED`/`UNFLAGGED`, `CREATE` with `ALREADYEXISTS`, `UID STORE … \Deleted` kept per message, `UID EXPUNGE` of the named UIDs only, `UIDPLUS` switchable in `CAPABILITY`) and only the dispatch hook in `test_server.rs`
- [ ] T008 (holzi) Write the failing tests in `src-tauri/src/extensions/mail/mail_manage_tests.rs` per `contracts/host-mail-commands.md`: every criterion alone and combined; more hits than `limit` with the right `total`; missing mailbox → 1001; `create_mailbox` new, existing (`created: false`), invalid name; `delete_messages` removes only the named UIDs and leaves other `\Deleted` mails, refuses without `UIDPLUS` and leaves every flag unchanged; each of the three needs `fetch` for its own host and port (pattern: `each_server_and_port_needs_its_own_permission`)
- [ ] T009 (holzi) Implement `search`, `create_mailbox`, `delete_messages` in `src-tauri/src/extensions/mail/imap_manage.rs` (`with_session!` and `imap_err` made `pub(super)` in `imap.rs`, nothing else changed there); `UIDPLUS` is checked before the first `STORE`; register the module in `mail/mod.rs`
- [ ] T010 (holzi) Add `search`, `create_mailbox`, `delete_messages` bridge functions to `src-tauri/src/extensions/mail/commands.rs` (names `extension_mail_search`, `extension_mail_create_mailbox`, `extension_mail_delete_messages`, check via `imap_server`, `limit` and `uids` bounds per contract) and entries in `src-tauri/src/extensions/bridge/allowlist.rs`; adjust the allowlist test and `bridge/dispatch_tests.rs` where they count or list mail methods
- [ ] T011 [P] (holzi) Add the three names and their rules (criteria object, 1000 bound, `UIDPLUS` refusal, `created` flag) to the mail row of `specs/017-extension-host/contracts/bridge.md`
- [ ] T012 (holzi) **operator** Run `cargo fmt --check`, the repository's Rust lint (both feature sets), `cargo test --manifest-path src-tauri/Cargo.toml --no-default-features --lib mail`, Prettier on touched Markdown, `wc -l` over the touched files; commit and open the pull request `feat(extensions): mail search, create mailbox and delete for extensions` (delivery E2a)

### E2b — vault-sdk: the counterparts

- [ ] T013 [P] (vault-sdk) Add `MailSearchCriteria` and `MailSearchResult` to `src/types/mail.ts` (fields exactly as in the contract)
- [ ] T014 [P] (vault-sdk) Add `search`, `createMailbox`, `deleteMessages` to `MAIL_COMMANDS` in `src/commands/mail.ts` (`extension_mail_search`, `extension_mail_create_mailbox`, `extension_mail_delete_messages`) and mention them in the permission comment
- [ ] T015 (vault-sdk) Extend `src/api/__tests__/mail.test.ts` with failing cases for `searchMessagesAsync`, `createMailboxAsync`, `deleteMessagesAsync` (command name, parameters, result), then implement them in `src/api/mail.ts`
- [ ] T016 (vault-sdk) **operator** Run the repository's checks, open the pull request (after T002 is merged), merge it and let the release process publish V2; note V2 and the merge SHA under this task (delivery E2b)

### E3 base — haex-mail

- [ ] T017 (haextension) Raise `@haex-space/vault-sdk` in `apps/haex-mail/package.json` to V2 (needs T002 and T016), refresh the lockfile, run `pnpm test` and `pnpm build`; the baseline from T003 must not get worse
- [ ] T018 [P] (haextension) Add `apps/haex-mail/app/tools/context.ts` with `context.test.ts`: `withMailContext(fn)` awaits `haexVault.initializeAsync()` and `accountsStore.loadAccountsAsync()` before `fn`; result helpers (`jsonResult`, `toolError` with a short message and no credentials, no server detail); `parseMessageId` and `formatMessageId` for `accountId::mailbox::uid`; `resolveMailbox(accountId, nameOrRole)` over the cached mailboxes and `assignRoles`
- [ ] T019 [P] (haextension) Add `apps/haex-mail/app/tools/manifest.test.ts` (fails until the tools exist): `manifest.tools` meets rules 1–8 of 018 `contracts/manifest-tools.md` and the schema subset; `effect` is `destructive` for exactly `add_rule`, `send_mail`, `delete_messages`, `read` for exactly `list_accounts`, `list_mailboxes`, `find_messages`, `get_message`, `list_rules`, otherwise `change`; every declared tool has a handler in the registry and every handler a declaration This test pins SC-005: the approval per mode comes from 018 and 032, here only the declared effects are checked.
- [ ] T062 [P] (haextension) Add `apps/haex-mail/app/tools/noSecrets.test.ts`: an account whose password is a recognisable marker, every handler runs against a mocked client, and neither results nor error messages contain the marker; `manifest.test.ts` additionally checks that no `inputSchema` has a property whose name starts with `password` (FR-018, FR-019, SC-006)
- [ ] T020 (haextension) Add `apps/haex-mail/app/tools/index.ts` (registry: name → handler, empty at first) and `apps/haex-mail/app/plugins/agent-tools.client.ts` that registers each registry entry with `client.tools.register(name, handler)` once the client exists, through `useNuxtApp().$haexVault`
- [ ] T021 (haextension) Propose, before touching code, extracting non-reloading variants of `bulkSetFlagAsync` and `bulkMoveToMailboxAsync` (`app/stores/mail/bulk.ts`) so tools and rules can change messages without reloading the visible list: cite `file:line` of the candidates, the proposed signatures, estimated lines saved against a parallel version and one rewritten call site as proof of concept; **wait for the operator to approve, redirect or ask for more** (constitution, graphify-first authoring; research R14). Blocks T029 and T041

**Checkpoint**: T012, T016 and T017 done → the base of every story exists.

---

## Phase 3: User Story 1 — Der Agent liest, filtert und sucht Mails (Priority: P1) 🎯 MVP

**Goal**: „Wie viele ungelesene Mails habe ich?“ und Suche über den ganzen Ordner beantwortet der Agent mit den Werkzeugen
von haex-mail, auch bei geschlossenem Fenster.

**Independent Test**: Quickstart steps 1–4 (count, search beyond the loaded 50, read without marking read, injection text
stays data).

- [ ] T022 [P] [US1] Add `apps/haex-mail/app/lib/messageQuery.ts` with `messageQuery.test.ts` first: `toSearchCriteria(input)` (dates, flags, text), mailbox role or name, page clamp (default 25, at most 50), merge across accounts by date, sort window of 200 for `sender` and `subject`, `window`/`hasMore`/`total` in the page Include a two-account case for paging: every account supplies `offset + limit` candidates, the merge is sorted and cut at `offset`/`limit`, `total` is the sum and `hasMore` means `total > offset + limit`.
- [ ] T023 [P] [US1] Add `list_accounts` (accounts only for now: `{ id, displayName, email }`, never credentials) in `apps/haex-mail/app/tools/accounts.ts` with `accounts.test.ts`
- [ ] T024 [US1] Add `list_mailboxes` in `apps/haex-mail/app/tools/mailboxes.ts` with `mailboxes.test.ts`: live `listMailboxesAsync(…, { includeStatus: true })` per account, roles from `assignRoles`, a failing account returns its cached counts with `cached: true` and the reason while the others stay unaffected
- [ ] T025 [US1] Add `find_messages` in `apps/haex-mail/app/tools/messages.ts` with `messages.test.ts`: `searchMessagesAsync` per account and mailbox, envelopes of the requested page through `fetchEnvelopesAsync` with `uidList` (chunks within the 1000 bound), `hasAttachment` filter on the fetched envelopes, all accounts in parallel when `account` is missing, a mailbox name an account lacks skips that account, the non-ASCII behaviour of T005 (local fallback marked in the result when chosen)
- [ ] T026 [US1] Add `get_message` in `apps/haex-mail/app/tools/readMessage.ts` with `readMessage.test.ts`: `fetchMessageAsync`, `bodyText` preferred, else `htmlToText(bodyHtml)`, 12 000 characters with `truncated: true`, never sets `\Seen`, empty fields instead of errors for mails without subject or text
- [ ] T027 [US1] Declare `list_accounts`, `list_mailboxes`, `find_messages`, `get_message` in `apps/haex-mail/haextension/manifest.json` (de/en titles, English descriptions, schemas from `contracts/manifest-tools.md`, at least four German and four English example sentences each, distinct from the neighbouring tools) and add their handlers to `app/tools/index.ts`; `manifest.test.ts` passes for them
- [ ] T028 [US1] **operator** Walk through `quickstart.md` steps 1–4 against holzi with E2a (needs the running app and a test account); record skipped steps in the pull request description

**Checkpoint**: The MVP — Phases 1–3 — answers the unread-mail question and searches whole folders.

---

## Phase 4: User Story 2 — Der Agent sortiert Mails (Priority: P1)

**Goal**: Verschieben, Markieren und Ordner anlegen mit den Freigabe-Regeln für „ändernd“.

**Independent Test**: Quickstart step 5 (move to archive by role, create `Steuer`, mark).

- [ ] T029 [US2] After T021 is approved: implement `mark_messages` and `move_messages` in `apps/haex-mail/app/tools/messageChanges.ts` with `messageChanges.test.ts` (at most 100 ids, role resolved per account, result `{ done, failed: { id, reason }[] }` with a failing mail not stopping the others, a mail gone from the server reported as failed) on top of the extracted non-reloading helpers
- [ ] T030 [P] [US2] Add `create_mailbox` to `apps/haex-mail/app/tools/mailboxes.ts` with a case in `mailboxes.test.ts`: `createMailboxAsync`, `created: false` when it exists, refreshes the cached mailbox list
- [ ] T031 [US2] Declare `mark_messages`, `move_messages`, `create_mailbox` (effect `change`) in `manifest.json` with examples and register the handlers
- [ ] T032 [US2] **operator** Walk through `quickstart.md` step 5; record skipped steps

---

## Phase 5: User Story 3 — Der Agent schreibt Mails, Senden bleibt eine bewusste Entscheidung (Priority: P1)

**Goal**: Entwürfe anlegen, Senden und endgültig Löschen als „zerstörend“ mit verständlicher Zustimmung.

**Independent Test**: Quickstart steps 6–7 (draft → send with consent in Auto → Sent; delete with subject shown).

- [ ] T033 [US3] Add `create_draft` in `apps/haex-mail/app/tools/drafts.ts` with `drafts.test.ts`: `buildReplyContextAsync` for `reply`, `reply-all`, `forward` (no attachments carried over), `buildRfc822Async`, `appendMessageAsync` into the `drafts` role with `\Draft \Seen`, the UID found by the Message-ID among the 20 newest envelopes of the drafts mailbox, result `{ draftId, to, subject, body }`, never sends
- [ ] T034 [US3] Add `send_mail` in `apps/haex-mail/app/tools/sendMail.ts` with `sendMail.test.ts`: input `{ draftId, to, subject, body }` compared with the stored draft (ignoring whitespace and line-ending differences); a mismatch sends nothing and returns an error carrying the draft's values; success sends via `sendMessageAsync`, appends the copy to the `sent` role and removes the draft with `deleteMessagesAsync`; a failing send keeps the draft
- [ ] T035 [P] [US3] Add `delete_messages` in `apps/haex-mail/app/tools/deleteMessages.ts` with `deleteMessages.test.ts`: input `{ messages: { id, subject }[] }` (at most 100), each subject compared with the server's (ignoring whitespace and case), a mismatch or a missing mail is reported and not deleted, a server without `UIDPLUS` surfaces the host refusal and leaves the mails
- [ ] T036 [US3] Declare `create_draft` (`change`), `send_mail` and `delete_messages` (`destructive`) in `manifest.json` with examples and register the handlers; `manifest.test.ts` effect mapping passes for them
- [ ] T037 [US3] **operator** Walk through `quickstart.md` steps 6–7 in modes Manuell, Auto and Plan; record skipped steps

---

## Phase 6: User Story 4 — Regeln von Hand anlegen, ändern und löschen (Priority: P2)

**Goal**: Regeln mit Oberfläche in den Einstellungen; neue Posteingangs-Mails laufen durch die aktiven Regeln.

**Independent Test**: Quickstart steps 8–9 (create, apply to existing, receive a matching mail, switch off, missing target).

- [ ] T038 [US4] Add `apps/haex-mail/app/lib/rules.ts` with `rules.test.ts` first: types, `validateRule` (rules of `data-model.md`), `matchRule` (case-insensitive substring, `to` over `to` and `cc`, `Name <address>` text via `senderText`), `summarizeRule` (structured parts for the UI), `planActions` (all matching rules, move last, earliest-created move wins, a moved mail is not run again, all-account rules only with roles), `targetMissing`, `planRuleRun(through, uids)` (first run sets the mark without running, later runs for UIDs above it)
- [ ] T039 [US4] Add the `rules` table and `rulesUidThrough` on `mailboxes` in `apps/haex-mail/app/database/schemas/index.ts`, generate `app/database/migrations/0003_*.sql` with `pnpm db:generate`, read the generated SQL and the journal and snapshot before committing
- [ ] T040 [US4] Add `apps/haex-mail/app/stores/rules.ts` (list, create, update, delete, enable; validates with `validateRule`; `origin` kept on edit); logic stays in `lib/rules.ts`, the store only reads and writes
- [ ] T041 [US4] Add `apps/haex-mail/app/stores/mail/rules.ts` `applyRulesToNewInboxAsync` and call it after `persistEnvelopesAsync` in `stores/mail/watch.ts` and for INBOX in `refreshMessagesAsync` (`stores/mail/sync.ts`); reset `rulesUidThrough` in `invalidateMailboxCacheAsync`; changes go through the helpers of T021; a missing target skips the whole rule (needs T021 approved) Call it **before** the visible list is reloaded (`loadMessagesAsync`), so a sorted-away mail never shows up in the inbox first (FR-022).
- [ ] T042 [US4] Add `applyRuleToExistingAsync(ruleId)` to `app/stores/mail/rules.ts`: the 1000 newest INBOX envelopes, local matching, result `{ applied, limited }`; leaves `rulesUidThrough` unchanged
- [ ] T043 [P] [US4] Add `apps/haex-mail/app/components/RuleDialog.vue` (name, account, condition rows with +/−, action list, checkbox „Auf vorhandene Mails in INBOX anwenden“, validation messages; de/en `<i18n>` block)
- [ ] T044 [P] [US4] Add `apps/haex-mail/app/components/RulesSection.vue` (list with name, plain-language summary, on/off switch, edit, delete confirmation, „Zielordner fehlt“ mark, „vom Agenten angelegt“ badge; de/en `<i18n>` block), following `pages/settings.vue` for accounts
- [ ] T045 [US4] Add the „Regeln“ section under „Konten“ in `apps/haex-mail/app/pages/settings.vue`; run lint, type check and `pnpm test`
- [ ] T046 [US4] **operator** Walk through `quickstart.md` steps 8–9 (desktop and mobile layout); record skipped steps

---

## Phase 7: User Story 5 — Der Agent verwaltet Regeln (Priority: P2)

**Goal**: `list_rules`, `add_rule` (zerstörend), `remove_rule` mit denselben Regeln wie die Oberfläche.

**Independent Test**: Quickstart step 10.

- [ ] T047 [US5] Add `list_rules`, `add_rule`, `remove_rule` in `apps/haex-mail/app/tools/rules.ts` with `rules.test.ts`: `add_rule` validates with `validateRule`, writes `origin: 'agent'`, maps `to` of an action to `role` or `mailbox`, runs `applyRuleToExistingAsync` for `applyToExisting` and returns `{ id, applied?, appliedLimit? }`; `remove_rule` by `id` or `name`, an ambiguous name removes nothing and names the ids, an unknown one is an error; no action type that forwards or sends exists
- [ ] T048 [US5] Declare `list_rules` (`read`), `add_rule` (`destructive`), `remove_rule` (`change`) in `manifest.json` with examples and register the handlers
- [ ] T049 [US5] **operator** Walk through `quickstart.md` step 10 in modes Manuell, Auto and Plan; record skipped steps

---

## Phase 8: User Story 6 — Ein Mailkonto einrichten, ohne dass das Passwort durchs Modell läuft (Priority: P2)

**Goal**: Konto aus dem Passwortmanager übernehmen oder den Assistenten vorbelegt öffnen; kein Passwort durch das Modell.

**Independent Test**: Quickstart step 11.

- [ ] T050 [US6] Extend `list_accounts` in `apps/haex-mail/app/tools/accounts.ts` with `importable` (`listImportableVaultAccountsAsync`, at most 20, `{ id, title, username }`) and a case in `accounts.test.ts` that no secret field appears
- [ ] T051 [US6] Add `import_account` in `apps/haex-mail/app/tools/accounts.ts` with a case in `accounts.test.ts`: `importAccountAsync(passwordItemId)`, an item with incomplete data is refused with the reason, the result carries the account only
- [ ] T052 [US6] Add `open_account_setup` in `apps/haex-mail/app/tools/accounts.ts`: stores the prefill (`email`, hosts, ports, security, `displayName`) in `app/stores/ui/index.ts`, navigates to the wizard and returns `{ prepared: true, hint }`; `components/AccountSetupWizard.vue` reads the prefill and leaves the password field empty; the tool's schema has no password property (test in `accounts.test.ts`)
- [ ] T053 [US6] Declare `import_account` and `open_account_setup` (`change`) in `manifest.json` with examples and register the handlers; with this task all 15 tools are declared and `manifest.test.ts` passes completely
- [ ] T054 [US6] **operator** Walk through `quickstart.md` step 11; confirm no password appears in history, consent or logs; record skipped steps

---

## Phase 9: Polish & Cross-Cutting

- [ ] T055 [P] (holzi) Add the tools of haex-mail as an entry in `src-tauri/src/chat/eval/extension_tools.json` (same shape as the manifest block) and at least 24 sentences of kind `extension` to `src-tauri/src/chat/eval/eval_set.json` (including „Wie viele ungelesene Mails habe ich?“ and „Schieb alle Newsletter ins Archiv“, none naming the extension except two); adjust `extension_tools_tests.rs` first (FR-035)
- [ ] T056 **operator** Run the model evaluation with Qwen3-4B (`cargo test --manifest-path src-tauri/Cargo.toml --test model_tool_eval -- --ignored --nocapture`) and measure SC-009 (a read call with haex-mail closed ≤ 3 s slower than open); record SC-001 to SC-003 and SC-009 in `specs/019-haex-mail-agent-tools/eval-results.md`; a miss on two-step tasks is a finding for 018, not a task here (plan, risk 1)
- [ ] T057 (haextension) Sign the manifest and bump the app version the way the repository's release flow does (`pnpm package`), then run `pnpm test`, lint, type check and `pnpm build` for `apps/haex-mail`
- [ ] T058 [P] (haextension) Check every new or changed file stays under 500 lines (`wc -l` over the plan's structure tree); split by topic where needed
- [ ] T059 (haextension) **operator** Open the three pull requests of E3 (3a: Phases 3–5 plus T017–T021, T055 stays in holzi; 3b: Phases 6–7; 3c: Phase 8), each with Conventional Commit messages and the walked-through quickstart steps in the description
- [ ] T060 Add **Mailwerkzeug**, **Regel** and **Aktion (einer Regel)** to `CONTEXT.md`; run `graphify update` in each worktree after its change landed
- [ ] T061 Write the merge SHAs of E1, E2a, E2b, E3a, E3b, E3c as `repository + full SHA` references into `specs/019-haex-mail-agent-tools/plan.md` (constitution IV) and run `/speckit-analyze` on the finished spec

---

## Dependencies & Execution Order

### Phase Dependencies

- Phase 1 has no dependency. T002 gates T016 and T017; T003 gates every haextension task.
- Phase 2: T005 → T006/T007 → T008 → T009 → T010 → T012 (E2a). T013 ∥ T014 → T015 → T016 (E2b, after T002).
  T017 needs T002 and T016. T018 ∥ T019 ∥ T062 → T020 (all after T017). T021 can start after T003 and blocks T029 and T041.
- Phase 3 (US1) needs T017–T020. Phase 4 (US2) needs T021 and T017–T020; `create_mailbox` (T030) also needs E2a for real
  servers, not for its unit test. Phase 5 (US3) needs E2a (`delete_messages`) and T033 before T034.
- Phase 6 (US4) needs T017–T021; Phase 7 (US5) needs Phase 6 (T040, T042); Phase 8 (US6) is independent of Phases 4–7
  after T020.
- Phase 9 needs the stories it measures: T056 needs Phases 3–5 for the read sentences and Phases 6–7 for the rule sentences.

### Within Each Story

Test before implementation; pure `lib/` module before store; store before tool or component; manifest entry and
registry entry in the same task as the last handler of the group so `manifest.test.ts` stays green at every commit.

### Parallel Opportunities

- T006 ∥ T007; T011 alongside T009 and T010; T013 ∥ T014; T018 ∥ T019 ∥ T062.
- T022 ∥ T023 (different files); T030 ∥ T029 once T021 is approved; T035 alongside T033 and T034.
- T043 ∥ T044; Phase 8 alongside Phases 6–7 when two people work.
- T055 ∥ T058.

## Parallel Example: User Story 1

```text
T022 messageQuery.ts + test   |  T023 list_accounts + test
T024 list_mailboxes  →  T025 find_messages  →  T026 get_message  →  T027 manifest + registry
```

## Implementation Strategy

### MVP First

Phases 1–3: with E2a and E2b merged and V2 raised in haex-mail, the agent answers „Wie viele ungelesene Mails habe ich?“
and searches whole folders. Stop and run quickstart steps 1–4.

### Incremental Delivery

1. E1 (vault-sdk release) → 2. E2a (holzi) ∥ E2b (vault-sdk release) → 3. E3a tools (US1–US3, MVP after US1) → 4. E3b rules
   (US4–US5) → 5. E3c accounts (US6) → 6. polish and measurement (Phase 9). Every step is its own pull request and
   can stop after any story.

### Notes

- Paths for haex-mail are relative to `apps/haex-mail/` of haex-space/haextension; vault-sdk paths relative to its root;
  holzi paths relative to this repository.
- Quickstart step 12 (model evaluation) is T056, step 13 (host tests) is T012.
