# Research: Tag management by selection, with merge

## R1 - How a merge is written

**Decision**: `tags::merge_tags(tx, source_id, target_id)` in one `CrdtTransaction`: refuse
`source == target` (`InvalidInput { reason: "same" }`), refuse a missing tag (`PasswordsNotFound`), move
every link of the source to the target, delete the source. The move is the loop that
`reconcile_tags` already runs for duplicate names, extracted into one private
`move_links(tx, from, to)` that both call.

**Rationale**: the loop already does what a merge needs: each old link is deleted with its own marker,
a new link gets the derived id `item_tag_id(item, target)`, an entry that has the target already gets no
second link, and the source row is deleted with a marker. One implementation per rule; the existing
`reconcile_*` tests keep guarding it.

**Alternatives**: a SQL `UPDATE ... SET tag_id` (no per-row markers, the sync would not carry it);
re-tagging through `bulk_set` by names (loads every entry, fold-resolves names that may race).

## R2 - Concurrent changes on other devices (scope reduction against FR-008)

**Finding**: the sync applies changes per column in HLC order and has no conflict policy for tags
(`ApplyPolicy` is only mentioned as an upgrade path in `tags.rs`). A merge is one write group, so it
arrives atomically, and every entry that carried the source when the merge ran carries the target
afterwards. Two cases are not covered without a new mechanism:

1. another device tags an entry with the source tag while (before syncing) the merge runs here: that
   link points to a deleted tag and the entry loses nothing but this one new tag, exactly as when the
   tag is deleted today;
2. A is merged into B on one device and B into A on another: both tags end up deleted and the links
   are decided per row by HLC; entries can lose both tags.

**Decision**: accept both, as `delete_tag` accepts the first. Mark the code with a `ponytail:` comment
naming the ceiling; the upgrade path is a synced merge-redirect record applied at open (like
`reconcile_tags`), which needs a migration and its own spec. The spec's FR-008 and its edge case are
reworded to promise only what holds (see spec).

**Alternatives**: the redirect record now (table + migration + cycle handling: out of proportion for a
case that needs two offline devices and one user doing opposite merges); soft-delete the source (a
second tag state every reader would have to learn).

## R3 - The searchable dropdown

**Finding**: `src/` has no combobox. The `haex-ui` layer ships `ShadcnPopover` and `ShadcnCommand`
(Reka listbox with an input). Its filter is `useFilter({ sensitivity: 'base' })`: case and accents are
ignored, and composed and decomposed umlauts compare equal, which is the rule FR-002 asks for.

**Decision**: one `TagPicker.vue` (Popover + Command) used twice: for the main selection and for the
merge target (prop `exclude`). It lists `store.displayTags`, not `store.tags`: equal names that two
devices created show as one entry (the smallest id, which `reconcile_tags` keeps), so the Command
item `value` (the name) is unique, and the counts are over all entries as `store.tags` has them. Each
item shows a color dot, the name and the usage badges (R7). No own filter code.

**Alternatives**: `UiSelect` (no search); the header-style menu (no search, no counts in a form); an
`<input list>` datalist (no styling, poor on Android).

## R4 - Telling the user which agents are affected (FR-010)

**Finding**: agent rights live in `haex_agent_profile_grants` rows of group `passwords`; the tag names
are a JSON array in `tags`, the tag a new entry gets is the row `key`. Views do not carry rights, so the
frontend cannot decide this itself.

**Decision**: a read command `passwords_tag_agent_profiles { tagId } -> string[]` returns the names of
the profiles whose password rights name that tag by `fold` (list or default tag). The merge dialog calls
it for source and target when it opens and shows one warning line naming the profiles when the result is
not empty. Browser-bridge approvals (spec 049) also scope by tag; they are not covered by the warning
and the spec's FR-010 says "agent", so this stays a documented gap.

**Alternatives**: a fixed warning in every confirmation (cries wolf, users stop reading it); rewriting
the rights on merge (changes what the user granted without asking).

## R5 - What stays as it is

Rename, color and delete keep their commands and error reasons. The wrong-name hint stays an inline
line under the field (allowed by the UI feedback rule). Delete keeps its alert dialog. The tag list in
the store (`store.tags`, `displayTags`) already reloads on table changes, so every place that shows
tags updates after a merge without new wiring (FR-011).

## R6 - Verification

Rust: merge moves links, no duplicate link, source gone, target name and color unchanged, source
without links, same tag refused, missing tag refused, nothing changes on refusal, markers exist for
every removed row, idempotent against a second call (`PasswordsNotFound`). Reference read: list tags,
default tag, fold match, no match. E2E: US1 and US2 end to end and the narrow-window layout. The e2e rig
runs on `main` only, so the scenarios carry diagnostics from the start (tests name what they saw).

## R7 - Counts per app

**Finding**: tags are one namespace for logins and Wallet passes (`scopeOf`: `pass` is Wallet's,
everything else the password manager's). The manager used to show one number over all links, while the
password manager's tag filter counts logins only (`loginTagsOf`), so "10 entries" opened a filter with 5.

**Decision**: a pure `tagUsageOf(tagIds, headers) -> { login, pass }` in `src/lib/passwords/scope.ts`,
built on `inScope` exactly like `loginTagsOf`, so the two can never disagree. `TagUsage.vue` shows
the app icon + number for the two apps, each icon taken from the registry entry of the app
(`getAppDefinition('system.passwords')` and `('system.wallet')` in `src/lib/wm/apps.ts`) and drawn with
`WmTabIcon`, the component the tabs use: the coloured image `/app-icons/<app>.svg` when the entry has
one, its Iconify icon otherwise. The icon-set setting (spec 053) concerns file icons only, so the app
icons do not vary with it; going through the registry keeps them identical to dock, launcher and tabs
wherever they come from. Each with an accessible name from i18n (`passwords.tags.usage.login|pass`), zero in
a muted style. The merge confirmation names both numbers in words. `scripts/check-wallet-scope.ts` tests
it (it already tests `loginTagsOf`).

**Alternatives**: one combined number (confuses, as the user pointed out); the number of the current
app only (hides that rename and delete reach the other app).

## R8 - Opening the manager from Wallet

**Decision**: `TagManager.vue` stays where it is and is mounted by both apps (its auto-import name is
`PasswordsTagManager`). Wallet gets a header button `tags` (`lucide:tags`, `wallet-tags`) in
`lib/wallet/header.ts` with the command `{ kind: 'tags' }`, `useWalletHeader` returns `tagManagerOpen`
(like `capturing`, so the dialog stays open when the button collapses, spec 093 FR-009), `WalletApp.vue`
mounts the dialog. `scripts/check-wallet-header.ts` is extended.

## R9 - Tag name of at least one character (FR-017)

**Finding**: already enforced everywhere in the backend: `validate_name` trims and refuses empty
(`empty`) and over 64 characters (`too_long`); `get_or_create` (adding to an entry, bulk tagging),
`rename_tag` call it, and `push_tag` of the import drops empty names. The manager shows `empty` under
the field.

**Decision**: no new code path; lock it with tests (rename to "" and " ", tagging by " ", import
drops it) and keep the inline hint. No tag with an empty name can be made, so none needs hiding in the
picker.
