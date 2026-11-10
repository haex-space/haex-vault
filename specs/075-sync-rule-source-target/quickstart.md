# Quickstart: check the rename

Run everything through the devShell (`nix develop --command …`); cargo through `scripts/with-nix-host-bridge.sh`.
Build the Rust tests only inside the 10 GB scope; the library test build is large.

## 1. No old name is left (SC-001)

```sh
rg -n '\b(a_size|b_size|a_mtime_ms|b_mtime_ms|listing_a|listing_b|aToB|AToB)\b|\bplace_(a|b)(_[a-z]+)?\b|SyncRuleSide::(A|B)\b|\bplace[AB]\b' \
  src src-tauri/src scripts specs/061-sync-rules
```

Before the change this finds 49 files with 351 hits and no noise (`place_at`, `placeAsync`, `MarketplaceAuthType` and
`data_size` do not match). The only hits left afterwards are `file_sync/agent_tests.rs` (the test that sends the old names on purpose), `identity/migrations_sync_rules.rs` (the applied `0044`) and the new migration with its test,
which name the old columns on purpose. The bare fields `a` and `b` of the engine (R1) cannot be searched by text;
the compiler finds them, since the old fields no longer exist, and the review checks the locals. `files-picker-a-root`, `files-picker-b-root` and `placeA` as a test id must
be gone as well:

```sh
rg -n "picker-(a|b)-root|confirm-place(A|B)" src scripts
```

## 2. Build and checks

```sh
nix develop --command scripts/with-nix-host-bridge.sh cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets
nix develop --command pnpm typecheck
nix develop --command pnpm typecheck:scripts
nix develop --command pnpm lint
nix develop --command pnpm check:files                # `check-files-sync-rules.ts` among them; then every other check:*
nix develop --command pnpm check:agent-actions         # fails when tools.json differs from the schema
nix develop --command pnpm exec prettier --write specs/075-sync-rule-source-target
```

For the Android build, clippy for `aarch64-linux-android` as well, since `file_sync` has `cfg` branches.

## 3. Generated files

```sh
nix develop --command scripts/with-nix-host-bridge.sh cargo test --manifest-path src-tauri/Cargo.toml file_sync
sed -i -E "s/[[:space:]]+$//" src/types/bindings/*.ts
git diff --stat src/types/bindings        # five SyncRule* files (the other three only name SyncRuleSide)
nix develop --command pnpm export:eval-tools          # then `git diff src-tauri/src/chat/eval/tools.json`
```

## 4. Migration (SC-003, US2)

```sh
nix develop --command scripts/with-nix-host-bridge.sh cargo test --manifest-path src-tauri/Cargo.toml \
  migrations_sync_rules_source_target
```

By hand, once: start the previous build with a vault that has a one-way rule, a two-way rule with the delete mode
"ask" and a pending deletion; start this build on the same vault; the list shows both rules with the same source,
target and direction, the pending deletion is still listed on the same place, and running the one-way rule that had
nothing to do shows a plan of 0 steps.

## 5. Agent (US3)

In `agent-sync-rules` the draft uses `source` and `target`. The scenario sends no call with the old names: the refusal
of `placeA` and `placeB` (invalid-input error, the list of rules unchanged) is checked by the Rust test
`a_call_with_the_old_place_names_is_refused_and_changes_nothing` in `src-tauri/src/file_sync/agent_tests.rs`. E2E runs on `main` only, so dispatch
`gh workflow run ci.yml --ref 075-sync-rule-source-target` before the merge.
