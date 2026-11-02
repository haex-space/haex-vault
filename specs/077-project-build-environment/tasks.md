# Tasks: Project-scoped build environment

## Phase 1: Setup

- [x] T001 Confirm registered topic worktree, approvals, installed dependencies and ignored outputs in `.gitignore`.

## Phase 2: Foundation

- [x] T002 Add isolated subprocess regression cases in `scripts/check-nix-host-bridge.ts` and demonstrate failures before production edits.

## Phase 3: US1 — Project-only CUDA startup (P1)

Independent acceptance: the actual CUDA development command compiles and starts from the project devShell without personal build exports.

- [x] T003 [US1] Discover host metadata, matched loader/libc and host runtime directories in `scripts/with-nix-host-bridge.sh`; preserve display selection and use host pixbuf configuration.
- [x] T004 [US1] Consume bridge settings and track rebuild inputs in `src-tauri/build.rs`; update the child-process cleanup explanation in `src-tauri/src/lib.rs`.
- [x] T005 [US1] Register passing bridge tests in `package.json` and `.github/workflows/ci.yml`.
- [x] T006 [US1] Run actual CUDA startup and inspect ELF runtime paths; record results in `specs/077-project-build-environment/quickstart.md`.

## Phase 4: US2 — Remove personal build exports (P2)

Independent acceptance: a fresh shell has no personal Android/CUDA build overrides, and the devShell still supplies SDK, NDK, Java and CUDA.

- [x] T007 [US2] Document project-shell startup and stale-shell handling in `README.md`.
- [x] T008 [US2] After T006, remove only the identified local shell build block and temporary CUDA symlinks; record verification without personal paths in `specs/077-project-build-environment/quickstart.md`.
- [x] T009 [US2] Recheck prerequisites and actual CUDA startup after cleanup using `scripts/with-nix-host-bridge.sh`.

## Phase 5: Validation and handoff

- [x] T010 Run formatter, script static checks, Bash syntax and Cargo fmt/check/clippy/test; record results and unrelated failures in `specs/077-project-build-environment/quickstart.md`.
- [x] T011 Review final diff, stop test processes, and prepare topic-branch pull request with validation evidence.

## Dependencies and execution

T001 → T002 → T003 → T004 → T005 → T006 → T008 → T009. T007 may run alongside T005 after the startup interface is settled. T010 follows implementation; T011 follows verification. US1 is the minimal deliverable; US2 deliberately waits for successful startup before personal cleanup. No additional infrastructure or external contracts are required.
