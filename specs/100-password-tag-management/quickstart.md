# Quickstart: validating tag management by selection, with merge

## Prerequisites

- A vault with at least 60 tags and a few entries tagged "Job", "Arbeit" and both.
- `nix develop` shell in the worktree; `pnpm install --frozen-lockfile` done.

## Automated

```text
nix develop --command scripts/with-nix-host-bridge.sh cargo test --manifest-path src-tauri/Cargo.toml passwords::tags
nix develop --command pnpm typecheck
nix develop --command pnpm lint
nix develop --command pnpm format:check
nix develop --command pnpm check:passwords
nix develop --command pnpm check:wallet
```

E2E (`passwords-tags`, `passwords-narrow-window`) runs in CI on `main` through the e2e workflow; the
scenarios name what they saw on failure.

## Manual

- **M1 (US1)**: open "Tags verwalten". Expect one dropdown, no list. Type three letters, pick a tag:
  name, color, count, rename and delete appear. Rename to an existing name in other case: hint under the
  field. Change the color: it shows in the dropdown at once.
- **M2 (US1)**: delete a tag and confirm: it is gone from the dropdown and from the tag filter; the
  selection is empty.
- **M3 (US2)**: select "Job", choose "Zusammenführen", pick "Arbeit": the confirmation names both and
  the entry count. Cancel: nothing changed. Confirm: "Arbeit" is selected with the union count, "Job" is
  gone everywhere, the entry that had both shows "Arbeit" once.
- **M4 (US2, FR-010)**: with an agent profile whose password rights name "Arbeit", the confirmation names
  that profile.
- **M5 (two devices)**: merge on device 1, sync; device 2 shows the same. Known limit: R2 in research.md.
- **M6 (US3)**: repeat M1 and M3 at 360 px width and on the Android emulator or a phone, and once with
  the keyboard only.
- **M7 (counts per app, US1 scenario 8-9)**: tag 5 logins and 5 Wallet passes with "Konzert". The manager
  shows the password manager's icon with 5 and Wallet's icon with 5 (the images of the dock, also after switching the icon set in the settings), no "10". Filter by "Konzert" in the password
  manager: 5 entries. Open the tag manager from Wallet's header: same tags, same numbers.
- **M8 (name rule, FR-017)**: rename a tag to nothing and to spaces: a hint under the field, nothing
  changes. Add " " as a tag to an entry: refused.
- **M9 (merge picture, US2 scenarios 8-11)**: choosing the target shows `old tag (key icon + number,
Wallet icon + number) → input`; the input holds the name of the target. The long explanation is
  behind the info button; the agent warning (M4) is not. Change the name to "Beruf" and confirm: every
  entry carries "Beruf" once. Enter the name of another tag, nothing and 65 characters: a hint under the
  input each time and nothing changes. Enter the name of the old tag: allowed.
