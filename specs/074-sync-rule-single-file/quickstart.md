# Quickstart: sync rules for single files

How to check the feature once it is built. The first part runs without a window; the second is the hand check
that only the operator can do.

## Automatic checks

Run from the worktree root (the Nix dev shell for `pnpm`, the host recipe for `cargo`):

```bash
# Rust: the module tests, with a bounded memory budget
env -u CC -u CXX -u AR systemd-run --user --scope -q -p MemoryMax=10G -p MemorySwapMax=0 \
  cargo test --manifest-path src-tauri/Cargo.toml -j 1 file_sync
env -u CC -u CXX -u AR systemd-run --user --scope -q -p MemoryMax=10G -p MemorySwapMax=0 \
  cargo test --manifest-path src-tauri/Cargo.toml -j 1 identity::migrations_sync_rules
sed -i -E "s/[[:space:]]+$//" src/types/bindings/*.ts

# Lints for desktop and Android (see the memory note on the NDK variables)
env -u CC -u CXX -u AR systemd-run --user --scope -q -p MemoryMax=10G -p MemorySwapMax=0 \
  cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -j 2 -- -D warnings

# Frontend and checks
nix develop --command pnpm export:eval-tools
nix develop --command pnpm typecheck
nix develop --command pnpm typecheck:scripts
nix develop --command pnpm lint
nix develop --command pnpm check:files
nix develop --command pnpm check:agent-actions
```

If the library test build is killed for memory, the CI runs the Rust tests; say so in the pull request.

The tests that carry the spec's table (names fixed in `tasks.md`):

| Spec                                 | Test                                                                                                                                                        |
| ------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------- |
| US1: copy, unchanged copies nothing  | `run_tests`: a file rule copies a changed source and a second run copies nothing                                                                            |
| US1.4: target picker folders only    | unchanged picker (no `allowFiles`), hand check step 2                                                                                                       |
| US2.1 to 2.3, FR-006, SC-001, SC-002 | `run_tests`: rename, move, delete, a folder above gone, a folder at the path (the same reason as a missing file); target unchanged, outcome `invalidSource` |
| US2.4, US2.7, FR-007                 | `run_tests`: the file is back, and replaced by a rename of a new file; the next run continues                                                               |
| US1.6, US3, FR-004                   | `plan_tests`, `run_tests`: copy back, conflict, missing copy copied again, never a deletion                                                                 |
| FR-008                               | `place_file_tests`: unmounted mount root is unreachable, a missing folder is invalid                                                                        |
| FR-010                               | `plan_tests`, `commands_tests`: `.haex-ignore` as source is copied; editor refuses                                                                          |
| FR-011                               | `rules_tests`: the overlap table of research R6                                                                                                             |
| FR-013, SC-003                       | `watch_tests`: a change of the file wakes the rule, a sibling does not                                                                                      |
| FR-014, SC-004                       | the existing tests of spec 061, unchanged                                                                                                                   |
| the migration                        | `migrations_sync_rules_source_kind_tests`                                                                                                                   |
| agents                               | `agent_tests`: a file rule through the tool; create without `sourceKind` is a folder rule, update without it keeps a file rule a file rule                  |
| the whole flow in the window         | E2E `files-sync-file-rule` (run by dispatching the branch's CI, since E2E runs on `main` only)                                                              |

## Hand check (operator)

Needs a build of this branch and any vault.

1. Make a file `Haushalt.ods` (any file) in a folder of the device and an empty folder as target.
2. Sync rules, "Neue Regel": source picker, open the folder, pick the file; target picker, pick the folder (only
   folders are offered). The delete mode field is not shown. Save; the rule appears with the file icon.
3. "Jetzt abgleichen": the copy appears under the same name. Run again: nothing is copied.
4. Change the file, run: the new version is in the target.
5. Rename the file and run: the row shows "Quelle ungültig" with the reason; the copy is untouched. Rename it back
   and run: the rule is valid again without any other step.
6. Save from an editor that replaces the file (or `mv new.ods Haushalt.ods`): the source stays valid.
7. Delete the copy and run: it comes back.
8. Switch the rule to "beidseitig", edit the copy, run: the change is in the source. Edit both, run: a conflict
   copy appears.
9. Open the rule's menu: no exclusion editor.
10. Try a second rule with the same source file: it is refused. Try a second file into the same target folder: it is
    accepted.
11. A rule from before this build still works and shows no file icon.
12. With a vault from an older build (before the migration): open it; the rules are there and are folder rules.
