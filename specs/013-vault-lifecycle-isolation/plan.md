# Implementation Plan: Vault Lifecycle Isolation

**Branch**: `013-vault-lifecycle-isolation` | **Date**: 2026-09-21 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/013-vault-lifecycle-isolation/spec.md`

## Summary

One app process serves at most one vault session, and closing the vault ends the process. That single
rule replaces the idea of resetting every piece of state by hand: a new vault always runs in a new
process with a fresh backend and a fresh page, so nothing can carry over. What remains to build is
small and concrete:

- **One gateway.** The generated invoke handler is wrapped once. After a close starts, every request
  except a short app-scoped allow-list is rejected with `VaultClosed`, so new commands are gated by
  default.
- **A close that cannot fail.** `close_instance` becomes an infallible, idempotent, immediate
  operation: flip the gate, fire the cancellation token, abort the turn, replace the page, then drain
  in the background (about 1 s cooperative, about 3 s in total) and end the process (relaunch by
  default, exit otherwise, forced after 0.5 s if the normal path hangs). Work is tracked by one
  `TaskTracker`; every database handle carries a tracker token, so "the database is free" means "no
  request still uses it".
- **Secrets that do not linger.** The passphrase becomes a zeroizing, redacted newtype, is never
  cloned, and its frontend field is emptied on success, dismissal or close. The second validation
  connection disappears
  with the atomic-switch path. The `SqlCipherKey` wipe is an upstream change in haex-crdt.
- **Several processes side by side.** A clear message for a vault held elsewhere, a short retry while
  that process finishes closing, a cross-process lock for model publication, a presence lock
  (exclusive while the startup cleanup runs, then shared for the life of the process) that keeps a
  starting process from deleting another process's work in progress, and a vault list that refreshes
  on focus.

The spike (kept on the local branch `spike/vault-gateway`, never merged) proved the gate, the
cancellation and the bounded drain against Tauri's real IPC path; its epoch and scoped-store parts
are not needed.
Decisions and alternatives are in [research.md](research.md); its two items that were open at plan
time (R2, R5) are resolved there now (T048, T049).

## Technical Context

**Language/Version**: Rust 1.95, edition 2021 (`src-tauri`); TypeScript 5 / Vue 3 on a Nuxt 4 SPA
with Pinia (frontend). Unchanged.

**Primary Dependencies**: Tauri 2.11.5, tokio 1.53, tokio-util 0.7.19 (`rt` feature, already enabled
on this branch, for `TaskTracker`). **One direct dependency added**: `zeroize` 1.x with the `serde`
feature (already in `Cargo.lock` at 1.9.0 through other crates). `tauri` with the `test` feature is a
dev-dependency (already on this branch). haex-crdt is pinned by full commit SHA at
`ed230d2c3f58c1b10710b6025ea0ce6c20b8d009`, its `SqlCipherKey` change now landed
(haexmas/haex-crdt#30, T083-T086).

**Storage**: None. No schema change, no migration. The vault database and its file lock are used as
they are.

**Testing**: Rust — sibling `*_tests.rs` files and integration tests in `src-tauri/tests/`, using
the Tauri mock runtime for the real IPC path (`cargo test`, `cargo fmt --check`, `pnpm lint:rust`).
Frontend — the replay harness `scripts/check-chat-state.ts` (`pnpm check:chat-state`, run in CI) plus
typecheck, template check, lint and Prettier. No new test runner. No arbitrary sleeps in async tests.

**Target Platform**: Desktop (Tauri). Linux, macOS, Windows are unchanged in scope; relaunch behavior
per platform is covered by Tauri (`current_binary`, AppImage path).

**Project Type**: Desktop application, Rust backend + Vue SPA frontend.

**Performance Goals**: The vault stops answering at once (gate flip is one lock). The process ends
within about 4 s of the close request (SC-002): 1 s cooperative + 2 s hard + 0.5 s until a forced
end + process teardown (about 3.5 s in the worst case).
Normal request latency is unchanged: the wrapper does one uncontended lock read per request.

**Constraints**: Close can never be refused (FR-002). Deadlines apply only after a close was
requested (FR-006). No `unwrap`/`expect` on I/O or input. Async code must not block the executor:
blocking vault work goes through the tracked `spawn_blocking`. Every touched file over 500 lines must
not grow (see Complexity Tracking).

**Scale/Scope**: About 50 registered commands, all gated by one wrapper; 7 app-scoped. 11 files use
`active_database`. 8 session-scoped spawn sites. 4 files read `active_instance` raw. Two frontend
sheets, two lock controls, one page refresh, two locale files.

## Constitution Check

_GATE: passed before Phase 0; re-checked after Phase 1 design (below). Sources: the repository's
`.spaex/constitution.md` behavior harness and `.specify/memory/constitution.md` (v1.4.0)._

| Requirement                                                           | Assessment                                                                                                                                                                                                                      |
| --------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| All changes in a dedicated worktree on a topic branch                 | ✅ `.worktrees/013-vault-lifecycle-isolation`, branch `013-vault-lifecycle-isolation`; primary checkout untouched.                                                                                                              |
| Test code in separate files, in every language                        | ✅ New `vault_gate/*_tests.rs`, `instances/passphrase_tests.rs`, integration tests in `src-tauri/tests/`; only `#[cfg(test)] mod x_tests;` declarations appear in production files.                                             |
| Graphify consulted before authoring named artifacts                   | ✅ Research R10: no tracker, gateway, zeroize or cross-process lock candidate exists. `close_instance`, `ChatState`, `abort_turn` and the preload handle are extended, not paralleled.                                          |
| 500-LoC boundary for hand-maintained files                            | ⚠️ Handled — see Complexity Tracking. All new files stay well under 500; existing oversized files receive call-site edits only.                                                                                                 |
| Simplifications carry a `ponytail:` comment                           | ✅ Planned at: the lock-retry poll, the publication-lock poll, the presence-lock gate and the fixed 1 s / 3 s / 0.5 s deadlines (ceiling: one poll interval; deadlines tunable in one place).                                   |
| Non-trivial logic leaves one runnable check                           | ✅ Gate state machine, drain ladder, wrapper allow-list, passphrase redaction and erase, publication lock, close protocol each get a test in the established runner.                                                            |
| Do not abstract merely because things look similar                    | ✅ `VaultGate` owns one concern (may requests run, what is still running). `Passphrase` exists because two argument types need identical redaction and erasure and must not diverge.                                            |
| No new crate/abstraction without a concrete problem                   | ✅ No new crate in the tree (`zeroize` is already locked); each new type maps to a spec requirement (see Requirement coverage).                                                                                                 |
| Rust: no `unwrap` on data, errors keep context, closed sets are enums | ✅ `VaultPhase`, `ClosePolicy`, `DrainOutcome` are enums; lock and I/O failures map to typed errors with the cause; no `unwrap` on the drain or lock paths.                                                                     |
| Do not add `.clone()` to silence the borrow checker                   | ✅ The passphrase clones are removed; `VaultDb` clones exist only because blocking closures need owned handles, and each clone carries a tracker token on purpose.                                                              |
| Non-blocking async                                                    | ✅ The drain and the lock polls await; `File::try_lock` is a non-blocking syscall; database work stays in the tracked `spawn_blocking`.                                                                                         |
| ADR for decisions materially affecting a Core Principle               | ✅ No Core Principle is affected (secrets in git, absolute paths, identity, references, opt-in, self-modification, relay, concealment). An ADR is still written because the plan reverses spec 001 FR-022 (a task in tasks.md). |
| No local absolute paths, no secrets in committed files                | ✅ All artifacts use repository-relative paths. Tests use a throwaway passphrase literal, never a real one.                                                                                                                     |
| Cross-repo references pin immutable revisions                         | ✅ The haex-crdt bump is by full commit SHA, in `Cargo.toml`, in a reviewed change.                                                                                                                                             |
| Agents leave no self-references in project artifacts                  | ✅ None in spec artifacts; commit messages must omit model trailers (constitution MUST NOT).                                                                                                                                    |
| Conventional Commits; PR to `main`; no squash                         | ✅ Enforced at commit/PR time per stage. Types per stage in tasks: `feat`, `fix`, `refactor`, `test`, `docs`, `build`.                                                                                                          |
| Phasing discipline                                                    | ✅ Prerequisite for the follow-up portable mode (014): relaunch keeps its configuration (FR-005). No later-phase work pulled forward.                                                                                           |
| Relay unavailability never blocks local work (Principle VII)          | ✅ No relay involvement; everything runs against local disk and process state.                                                                                                                                                  |
| `/speckit-plan` checks against constitution                           | ✅ This table; no unjustified violation.                                                                                                                                                                                        |

**Post-design re-check**: the design added `VaultGate` (with `VaultDb`, the wrapper and the drain),
the `Passphrase` newtype, `PublicationLock`, `ProcessPresence` and `ChildRegistry`. Each is a concrete, tested unit
tied to a requirement; none is speculative. The only open constitutional item is the
oversized-file handling: two of the five touched files grew slightly in practice (T088, +12/+19
lines, routing existing functions through the gate) rather than staying flat as first planned — a
documented, bounded exception, not a silent rule break.

## Requirement coverage

| Spec items                     | Delivered by                                                                                                                      |
| ------------------------------ | --------------------------------------------------------------------------------------------------------------------------------- |
| FR-001, FR-002, FR-006, SC-001 | Gateway wrapper + infallible `close_instance` + closing page ([contracts](contracts/tauri-commands.md))                           |
| FR-003, FR-004, FR-005, SC-002 | Cancellation token, `TaskTracker`, drain ladder, `ClosePolicy`, forced end, `ChildRegistry` ([data-model](data-model.md), R2, R5) |
| FR-007                         | Window-close and exit hooks reusing the same close task (R9)                                                                      |
| FR-008                         | Cancelled turns and transfers keep their existing "cancelled, not complete" persistence; verified in quickstart 1                 |
| FR-009                         | Page replacement by the backend, spinner only ([frontend contract](contracts/frontend-surface.md))                                |
| FR-010, FR-011, FR-012, SC-003 | `VaultAlreadyActive`, removal of the atomic switch, process boundary (R6)                                                         |
| FR-013..FR-017, SC-004, SC-005 | `Passphrase` newtype, no clones, no validation connection, form clearing, upstream key wipe (R7)                                  |
| FR-018..FR-023, SC-006, SC-007 | Error mapping, open-time retry, `PublicationLock`, focus refresh, debug-only logging (R8)                                         |
| FR-024                         | No change to the models directory layout                                                                                          |
| FR-026, SC-010                 | `ProcessPresence` lock; startup cleanup runs only as its callback when the process is alone (R8)                                  |
| FR-025, SC-008                 | Already on the branch (`active_model_info` takes no operation slot)                                                               |

## Project Structure

### Documentation (this feature)

```text
specs/013-vault-lifecycle-isolation/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── tauri-commands.md
│   └── frontend-surface.md
├── checklists/requirements.md
└── tasks.md            # created by /speckit-tasks
```

### Source Code (repository root)

```text
src-tauri/
├── Cargo.toml                          # zeroize (direct); later: haex-crdt rev bump by full SHA
├── src/
│   ├── vault_gate/                     # NEW: one concern, each file well under 500 lines
│   │   ├── mod.rs                      #   VaultGate, VaultPhase, ClosePolicy
│   │   ├── db.rs                       #   VaultDb (Arc<Database> + tracker token)
│   │   ├── drain.rs                    #   ladder: cooperative -> abort -> report
│   │   ├── children.rs                 #   ChildRegistry: kill every child process group before the end
│   │   ├── invoke.rs                   #   handler wrapper + app-scoped allow-list
│   │   └── *_tests.rs                  #   sibling test files
│   ├── state.rs                        # active_instance private; database()/install()/take()
│   ├── state_utils.rs                  # active_database -> VaultDb
│   ├── error.rs                        # + VaultClosed, VaultAlreadyActive; - CloseFailed
│   ├── lib.rs                          # manage gate, wrap handler, build().run(callback), window events
│   ├── instances/
│   │   ├── passphrase.rs               # NEW: Zeroizing newtype, redacted Debug, serde, ts string
│   │   ├── open.rs                     # refuse when active; drop switch + validation connection; no clones
│   │   ├── create.rs                   # same gate rules; no clone
│   │   ├── presence.rs                 # NEW: ProcessPresence lock gating the startup cleanup
│   │   └── close.rs                    # infallible phase 1 + background phase 2
│   ├── chat/{commands,default_model,model_loading}.rs   # register/observe the token (call-site edits)
│   ├── models/{commands,download,paths}.rs              # token in transfers; PublicationLock
│   ├── voice.rs                        # tracked tasks, token
│   └── providers/{connect,mod}.rs      # tracked connect task, cancellable refresh
└── tests/
    ├── vault_gateway.rs                # wrapper + allow-list over the mock runtime
    └── vault_lifecycle_close.rs        # close protocol end to end

src/
├── composables/useErrorString.ts       # + 3 kinds
├── pages/{chat,federation}/[instance].vue   # lock flow: close only
├── pages/index.vue                     # refresh on focus
├── components/onboarding/{UnlockSheet,CreateSheet}.vue   # clear on success and dismissal
├── i18n/locales/{de,en}.json           # + errors.* keys
└── public/closing.html                 # static closing page, spinner only (placement verified, R4)

scripts/
├── lib/chat-state-harness.ts           # NEW: extracted from check-chat-state.ts (prerequisite)
└── check-vault-lifecycle.ts            # NEW: lock-flow and passphrase-lifetime cases

docs/adr/0003-one-vault-session-per-app-process.md      # NEW: supersedes spec 001 FR-022
```

**Structure Decision**: Extend the existing Rust crate and Vue app; no new crate, package or
process. The new gateway lives in its own small module so `state.rs`, `close.rs` and `lib.rs` keep
their current size. The frontend is edited in place.

## Delivery order

Each stage compiles and passes its tests on its own, so the stages can be reviewed separately.

1. **Secret hygiene (Rust)** — `zeroize` dependency, `Passphrase` newtype, redacted `Debug`, no
   clones in `open.rs`/`create.rs`. Independent and small.
2. **Gate, tracker, wrapper** — `vault_gate/`, `VaultDb`, `AppState` encapsulation,
   `active_database` change, the two new error kinds, bindings regenerated. Behavior changes only
   for requests after a close, which cannot happen yet.
3. **Close protocol** — infallible `close_instance`, drain ladder, `ClosePolicy`, window and exit
   hooks, tracked spawn sites, token in long-running commands, closing page (a spinner) and the
   frontend lock flow. This is the stage that makes SC-001 and SC-002 true.
4. **One vault per process** — `open`/`create` refuse while active, atomic switch and validation
   connection removed, ADR, supersession note in the 001 contract.
5. **Several processes** — error mapping and messages, open-time retry, `PublicationLock`, the
   presence lock that gates the startup cleanup, list refresh on focus.
6. **Frontend tests and passphrase lifetime** — harness extraction, form clearing, lock-flow and
   lifetime cases.
7. **Upstream key wipe** — haex-crdt `SqlCipherKey` change and the pinned revision bump. It depends
   on another repository and does not block stages 1 to 6.

## Complexity Tracking

| Item                                                                                                                                                                                                  | Why needed                                                                                                 | Simpler alternative rejected because                                                                                                                                                                                                                                                                                                                                                                                                        |
| ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| New direct dependency `zeroize` (`serde` feature)                                                                                                                                                     | FR-014 needs compiler-enforced erasure on drop and a serde-capable secret string; it is already locked     | Hand-written overwrite loops are easy to get wrong, are not tied to `Drop`, and would need their own serde glue                                                                                                                                                                                                                                                                                                                             |
| New abstraction `VaultGate` (with `VaultDb`, wrapper, drain)                                                                                                                                          | One place for "may requests run" and "what is still running"; no candidate exists (research R10)           | Extending `ChatState` would tie voice, models and providers to the chat module; per-command checks cannot cover a command that forgets them                                                                                                                                                                                                                                                                                                 |
| Existing files over 500 lines that are touched: `models/commands.rs` (966), `chat/commands.rs` (759), `chat/model_loading.rs` (736), `providers/mod.rs` (576), `src/pages/chat/[instance].vue` (1320) | Call sites must observe the token or register a task                                                       | Splitting them here would mix an unrelated refactor into a security change. Rule: no net line growth; new logic goes into `vault_gate/`; splits are separate work. In practice (T088) two of the five grew slightly — `chat/commands.rs` +19, `models/commands.rs` +12 — routing `send_message`/`download_from_hf_inner` through the gate's `spawn`/`run`; judged a necessary, bounded exception rather than a rule break worth blocking on |
| `scripts/check-chat-state.ts` is 1550 lines and asks for extraction first                                                                                                                             | Its header requires `createChatState` to move to `scripts/lib/` before the next new case                   | Adding cases to the oversized file breaks its own rule. The harness moves to `scripts/lib/` first, so the new script reuses it instead of copying it                                                                                                                                                                                                                                                                                        |
| Reverses spec 001 FR-022 (atomic switch)                                                                                                                                                              | Process-per-session is the isolation guarantee; the switch is the source of the leak risk                  | Keeping the switch would need the epoch and scoped-store machinery this plan removes; recorded in ADR 0003 and a note in the 001 contract                                                                                                                                                                                                                                                                                                   |
| FR-001 wording for requests already executing (research R4)                                                                                                                                           | The wrapper cannot replace a response in flight; the page replacement makes any late response unobservable | Typed errors for every in-flight request need an extractor on all commands; offered as a follow-up if the reviewer requires it                                                                                                                                                                                                                                                                                                              |
| External dependency: haex-crdt `SqlCipherKey` zeroize                                                                                                                                                 | Only that crate can erase the key copy it holds                                                            | A wrapper in haex-vault cannot reach the `String` inside the crate. Landed: [haexmas/haex-crdt#30](https://github.com/haexmas/haex-crdt/pull/30), pinned in haex-vault at `ed230d2c3f58c1b10710b6025ea0ce6c20b8d009` (T083-T086)                                                                                                                                                                                                            |
