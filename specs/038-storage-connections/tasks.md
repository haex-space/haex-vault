---
description: 'Task list for spec 038-storage-connections'
---

# Tasks: Speicherverbindungen (S3) und ihre Weitergabe an Erweiterungen

**Input**: Design documents from `/specs/038-storage-connections/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md),
[data-model.md](./data-model.md), [contracts/bridge.md](./contracts/bridge.md),
[contracts/tauri-commands.md](./contracts/tauri-commands.md), [contracts/access-z14.md](./contracts/access-z14.md),
[quickstart.md](./quickstart.md)

**Tests**: Included. The constitution requires a runnable check for non-trivial logic, and SC-002/SC-003
need a corpus of credential-leak and key-escape cases. Rust tests live in `*_tests.rs` next to the file
(`#[cfg(test)] #[path = "…_tests.rs"] mod tests;`), never inline. No network services in `cargo test`:
`wiremock` (local listener) for S3, a fake `ObjectStore` for the bridge (research R10).

**Organization**: Grouped by user story in priority order (P1: US1, US2; P2: US3, US4).

**Shipping note** (plan.md, Lieferungen): PR A = Phase 1 (docs); PR B = Phase 2a (rule Z14 in 034);
PR C = Phase 2b + US1 + US4; PR D = US2 + US3; the vault-sdk PR (T040) runs next to PR D. Each PR is
reviewed and merged before the next one starts on top of `main`.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependency on an incomplete task)
- **[Story]**: User story from spec.md (US1–US4)

---

## Phase 1: Setup (PR A, docs)

- [x] T001 Prepare `.worktrees/038-storage-connections`: real `pnpm install` (no symlinked `node_modules`), reflink `src-tauri/target` from a worktree with the same `Cargo.lock` if one exists, check `nix develop --command scripts/with-nix-host-bridge.sh cargo check --manifest-path src-tauri/Cargo.toml`
- [x] T002 [P] Before the first new name, run bounded `graphify query … --budget 1200` for "password item visibility trash access ItemState", "extension dialog confirm open_dialog resolve", "web fetch limits max_response_bytes stream", "settings category registry view"; note the candidates checked in the PR description (constitution graphify rule; warn and continue if graphify fails) **Note**: run 2026-10-05; candidates extended instead of duplicated: `access::ItemState`/`visible`/`authorize_*` (owner added), `trash::purge_item` (reused for `delete_owned_item`); `require_user` and `EntryUsage` checked, different purpose; dialog (`fs/dialogs.rs`, `bridge/methods.rs`), limits (`sql::exec::Limits`, `web.rs::fetch`) and settings (`settings/registry.ts`, `ExtensionLimitsView.vue`) noted for PR C/D
- [x] T003 [P] Align spec 017 with 038: in `specs/017-extension-host/tasks.md` T106 add a note "built by spec 038 (storage connections), see `specs/038-storage-connections/tasks.md`"; in `specs/017-extension-host/research.md` R21 bullet "Entfernter Speicher" replace "S3-Client wählt der Plan von 029. Bis dahin „nicht verfügbar“" with the 038 decisions (rusty-s3, prefix `holzi-ext/<extension_id>/`, credentials only in holzi); in `specs/017-extension-host/contracts/bridge.md` row `extension_remote_storage_*` replace "über 029, sonst 8001" with "über 038 (`specs/038-storage-connections/contracts/bridge.md`)"; in `specs/017-extension-host/contracts/permissions.md` row `remoteStorage` change the target text to "Kennung des Speichers (038) oder `*`"; in `specs/017-extension-host/spec.md` FR-054 replace "die Speicher aus 029" with "die Speicher aus 038"
- [x] T004 [P] Align spec 029 with 038 in `specs/029-own-s3-storage/spec.md`: under "Beziehung zu bestehenden Specs" add a bullet that the Speicherverbindung (provider, endpoint, region, addressing, credentials in 034 with owner `storage`, rule Z14) is defined by `specs/038-storage-connections/spec.md` and 029 adds the Eignungsprüfung, space buckets and access keys on top; in Key Entities point "Speicherverbindung" to 038
- [x] T005 [P] Add a row for 038 to `plans/README.md` (priority P2, effort M, status "Spezifiziert und geplant 2026-10-05", gate: 017 L1–L4 and 034 in use; unblocks 017 T106 and is reused by 029), and in the 017 row change "der entfernte Speicher in L5 setzt Spec 029 voraus" to "… setzt Spec 038 voraus"
- [x] T006 Run `pnpm format:check` (specs are covered), commit, open PR A against `main` **Note**: PR A = haexmas/holzi#285, merged 2026-10-06

---

## Phase 2: Foundational

### Phase 2a: Rule Z14 in the password manager (PR B)

**Purpose**: Without Z14 an extension with a `passwords` grant for `*` could read the S3 credentials (research R2). Blocks every story.

- [x] T007 Write the tests first in `src-tauri/src/passwords/access_tests.rs`: an entry with `owner: Some("storage")` is `NotFound` for read/update/delete for `Extension`, `ExternalAgent` and `Internal { feature: "other" }` with a `ReadWrite` grant for `Scope::All`; `Forbidden` still comes first without any grant (Z3); `User` and `Internal { feature: "storage" }` (with a grant) see it
- [x] T008 Add the column in a new migration `0027_passwords_owner` in `src-tauri/src/identity/migrations_passwords_owner.rs` (`ALTER TABLE haex_passwords_item_details ADD COLUMN owner TEXT`; "`NULL` = Eintrag des Nutzers; sonst Name einer holzi-Funktion (`storage`)"), register it in `src-tauri/src/identity/migrations.rs`, raise `HOLZI_TRIGGER_VERSION`, test in `src-tauri/src/identity/migrations_passwords_owner_tests.rs` (column exists, existing rows `NULL`, trigger version raised) **Note**: the trigger rewrite on an upgraded vault is tested in `src-tauri/tests/vault_upgrade.rs` (`reopening_a_pre_0027_vault_tracks_the_owner_of_a_password_entry`), which can open the old vault at trigger version 16; the 0022 schema test now checks 0022's columns as the leading ones
- [x] T009 Implement Z14 in `src-tauri/src/passwords/access.rs`: `ItemState { tags, in_trash, owner: Option<&str> }`, `visible` checks the owner first (`User` always, `Internal { feature }` only when `feature == owner`, everybody else `NotFound`), keeping Z3 (`Forbidden` without a grant) before it; update every construction of `ItemState` (grep `ItemState {`: `service/items.rs`, `service/trash.rs`, `references_db.rs`, `passkeys_ops.rs`, `copy.rs`) **Note**: `item_state` returns `items::StoredState` (`view()` lends an `ItemState`); `access::sees_owned` is the one rule, used by `visible` and the lists
- [x] T010 Read the owner in `src-tauri/src/passwords/items.rs`: `item_state` returns it, `headers_in_scope` and `agent_headers` leave out owned entries for every caller except `User`; tests in `src-tauri/src/passwords/items_tests.rs` (list with `*` grant omits the owned entry; agent headers omit it) **Note**: the list and agent-header cases are tested through the service in `src-tauri/tests/passwords_owner.rs` (public API), not in `items_tests.rs`
- [x] T011 [P] References: test in a new `src-tauri/tests/passwords_owner.rs` (`passwords_references.rs` is already over 500 lines) that a placeholder in a user entry pointing at an owned entry does not resolve for an extension with a `*` grant (field absent), and resolves for `User`
- [x] T012 Service methods in `src-tauri/src/passwords/service/items.rs` and `src-tauri/src/passwords/service/trash.rs`: `create_owned_item(caller: &Caller, input: ItemInput) -> Result<String>` allowed only for `Caller::Internal { feature }` (sets `owner = feature`, otherwise `PasswordsForbidden`), and `delete_owned_item(caller, item_id)` that deletes permanently only when `caller` is `Internal { feature }` and `owner == feature`; no Tauri command and no bridge method sets or changes `owner` (`create_item`/`update_item` keep `owner` untouched); tests in the matching `*_tests.rs` **Note**: in a new `src-tauri/src/passwords/service/owned.rs`; tested in `src-tauri/tests/passwords_owner.rs`
- [x] T013 [P] Frontend marker: add `owner` to `ItemHeader` (Rust `src-tauri/src/passwords/model.rs`, ts-rs binding regenerated) and show "gehört zu: Speicher" / "belongs to: storage" on owned entries in the password list and detail (`src/components/passwords/`, keys in `src/i18n/locales/{de,en}.json`)
- [x] T014 Copy rule Z14 from `specs/038-storage-connections/contracts/access-z14.md` into `specs/034-password-manager/contracts/access.md` (rule table, `ItemState`, the sentence "es gibt keine für holzi reservierten Tags" stays true and gets "Einträge mit Eigentümer: Z14")
- [x] T015 Extension check: in a new `src-tauri/src/extensions/passwords_owner_tests.rs` (registered in `src-tauri/src/extensions/passwords.rs`) prove through the bridge that `extension_password_list/read/update/delete` with a `*` grant never reach an owned entry (1001 for read/update/delete by id, absent in list) — SC-002 part 1
- [x] T016 Run `cargo fmt --check`, `pnpm lint:rust`, the password and extension lib tests, `pnpm typecheck`, `pnpm lint`, `pnpm format:check`; commit, open PR B

### Phase 2b: S3 in holzi, data (PR C, start)

- [x] T017 Add `rusty-s3 = "0.10"` (default features `rustcrypto` and `full`, NOT `aws-lc-rs`) to `src-tauri/Cargo.toml` with a comment pointing to research R1; confirm `cargo tree -i aws-lc-rs` shows no new path through `rusty-s3` **Note**: `cargo tree -i aws-lc-rs` shows only the existing path through `rustls`, none through `rusty-s3`
- [x] T018 Migration `0028_storage_connections` in `src-tauri/src/identity/migrations_storage.rs`, registered in `src-tauri/src/identity/migrations.rs`, `HOLZI_TRIGGER_VERSION` raised, tables in `SYNCED_TABLES`/`DEVICE_TABLES` lists: `haex_storage_connections` (`id` TEXT PK UUIDv4; `provider_name` TEXT "Pflicht, 1–80 Zeichen"; `provider_kind` TEXT "`aws`, `rustfs`, `other`"; `endpoint` TEXT; `endpoint_scope` TEXT "`public` oder `local`" (research R8, FR-009b); `region` TEXT "Pflicht, 1–64 Zeichen"; `addressing` TEXT "`path` oder `virtual`"; `credentials_item_id` TEXT; `created_at`, `updated_at` TEXT), `haex_storages` (`id` TEXT PK UUIDv4; `connection_id` TEXT; `name` TEXT "Pflicht, 1–80 Zeichen"; `bucket` TEXT "Bucket-Name nach S3 (3–63, Kleinbuchstaben, Ziffern, `.-`)"; `created_at`, `updated_at`), `storage_tests_no_sync` (`storage_id` TEXT PK, `tested_at` TEXT, `outcome` TEXT "`passed`, `accessDenied`, `unreachable`, `bucketMissing`, `missingRight`"); no UNIQUE constraints in synced tables; test in `src-tauri/src/identity/migrations_storage_tests.rs` **Note**: `haex_storages.connection_id` references the connection with `ON DELETE CASCADE` (like the extension tables); trigger version 18 also checked in `src-tauri/tests/vault_upgrade.rs` (`reopening_a_pre_0028_vault_installs_the_storage_triggers`)
- [x] T019 [P] Types and trait in `src-tauri/src/remote_storage/mod.rs`: `ConnectionRow`, `StorageRow`, `TestOutcome`, `ProviderKind`, `Addressing`, `StorageError { NotFound, AccessDenied, Network, TooLarge, TimedOut }`, trait `ObjectStore { put, get (limit), list (prefix, max), delete }` taking a resolved `Endpoint` + `Credentials` (zeroized secret, redacted `Debug`); register `pub mod remote_storage;` in `src-tauri/src/lib.rs` **Note**: `StorageError` has one more variant, `MissingRight` (403 `AccessDenied`: valid credentials without the right), so the test can tell `accessDenied` from `missingRight` (R9); fake in `src-tauri/src/remote_storage/test_support.rs`
- [x] T020 [P] Endpoint check (research R8, FR-009b, operator 2026-10-06) in `src-tauri/src/remote_storage/address.rs` + `address_tests.rs`: the scope of an endpoint when it is set (`local` when the host resolves to loopback or private addresses, else `public`; `local` allowed for the user's input and for an extension's proposal under a `remoteStorage`/`add` grant for that host); on each call every resolved address must be in the stored scope; link-local (`169.254.0.0/16` incl. `169.254.169.254`, `fe80::/10`), unspecified, multicast and broadcast addresses refused always, also IPv4-mapped (`::ffff:a.b.c.d`); `https` always, `http` only with scope `local`; table of cases (public IP over http refused, `http://192.168.1.5:9000` allowed and flagged `insecure` with scope `local`, a `public` endpoint that now resolves to `127.0.0.1` refused, `http://169.254.169.254` and `https://[fe80::1]` refused always, `https://[::ffff:169.254.169.254]` refused, `ftp://` refused, userinfo in URL refused) **Note**: `address::scope_of` fixes the scope when the endpoint is set (a host with both local and public addresses is refused, a name without addresses fails the test as `unreachable`); `address::pin` checks every address of each call against the stored scope; NAT64 addresses (`64:ff9b::/96`) count as their IPv4 address; the extension side (`add` grant) comes with PR D (T039)
- [x] T020a Resolve and pin (research R8, Review 2026-10-06) in `src-tauri/src/remote_storage/address.rs` + `address_tests.rs`, used by T021 and the probe (T024): before every call resolve the endpoint host once, check every resolved address with T020 (one disallowed address fails the call: 2002 `network`, in the probe `unreachable`), then build the request so that reqwest connects to exactly the checked address (`ClientBuilder::resolve` for this host) and does not resolve again; host name kept for TLS (SNI, certificate) and the signed `Host` header; tests with an injected resolver: a name that resolves to `127.0.0.1` refused for `extension`, a resolver that answers public first and `169.254.169.254` second (DNS rebinding) never reaches the second address, a mixed answer (public + link-local) refused **Note**: reqwest connects through `ClientBuilder::resolve_to_addrs` with all checked addresses of the one lookup, and `no_proxy()`, so a system proxy cannot bypass the check
- [x] T021 S3 implementation in `src-tauri/src/remote_storage/s3.rs` + `s3_tests.rs` (wiremock): `rusty-s3` presigned actions sent with the shared `reqwest` client (rustls/ring, redirects off) to the address pinned by T020a; PUT, GET streamed with a byte limit and one deadline (as `extensions/web.rs::send`), ListObjectsV2 following continuation tokens up to `max` keys (more → `TooLarge`), DELETE; map 404/NoSuchKey/NoSuchBucket → `NotFound`, 401/403/InvalidAccessKeyId/SignatureDoesNotMatch/ExpiredToken → `AccessDenied`, network/timeout/5xx/3xx → `Network`; no endpoint, header or body text in error messages (log the cause without secrets)
- [x] T022 Store in `src-tauri/src/remote_storage/store.rs` + `store_tests.rs`: read/write connections and storages through `VaultDb`, validation of the field rules above, `remove_storage` deletes the row and every `extension_permissions` row of kind `remoteStorage` with that target in the same write (FR-007), `remove_connection` removes its storages the same way; test results in `storage_tests_no_sync`

**Checkpoint**: PR B merged; Phase 2b compiles and is tested — stories can start.

---

## Phase 3: User Story 1 - Eigenen Speicher in holzi verbinden (Priority: P1) 🎯 MVP

**Goal**: Connections and storages in the settings, tested before saving, credentials in the password manager.

**Independent Test**: quickstart §4 against a local RustFS; automated: commands against wiremock.

- [x] T023 [US1] Credentials in `src-tauri/src/remote_storage/credentials.rs` + `credentials_tests.rs`: create the entry with `create_owned_item` as `Caller::Internal { feature: "storage" }` (title `S3: <provider_name>`, username = access key id, password = secret, key-value `sessionToken`, url = endpoint), read it with `read_secret_item` and a grant fixed in code, replace it on new credentials, delete it with `delete_owned_item` when no connection uses it; credentials state `present`/`syncing`/`missing` per data-model.md (Review 2026-10-06, instead of `credentialsMissing: bool`): a missing entry with a delete marker in `haex_deleted_rows` → `missing`, without one → `syncing`; a call by an extension fails with 2002 `network` while `syncing` and 2002 `accessDenied` while `missing`; tests for all three states **Note**: an entry in the trash counts as `missing` too
- [x] T024 [US1] Connection test (research R9) in `src-tauri/src/remote_storage/probe.rs` + `probe_tests.rs` against the fake `ObjectStore`: write, read, list, delete `holzi-test/<UUID>`, delete also after a failed step, outcomes `passed`/`accessDenied`/`unreachable`/`bucketMissing`/`missingRight`; SC-004: a failed probe stores no credentials, deletes its test object also after a failed step, and names the object's key when the provider refuses the delete or cannot be reached
- [x] T025 [US1] Tauri commands in `src-tauri/src/remote_storage/commands.rs` per `contracts/tauri-commands.md` (`storage_list`, `storage_connection_save` with test before saving a new connection, new credentials or a changed endpoint, region or addressing (FR-003, Review 2026-10-06) and `endpoint_scope` set from the user's endpoint (R8), `storage_connection_remove`, `storage_save`, `storage_remove`, `storage_removal_preview`, `storage_test`), ts-rs types to `src/types/bindings/`, registered in `src-tauri/src/lib.rs`; tests in `src-tauri/src/remote_storage/commands_tests.rs` (no secret in any return value, nothing saved on a failed test, removal preview lists storages and extensions) **Note**: logic in `src-tauri/src/remote_storage/service.rs` (`StorageService`, also `access_of` for PR D), tests in `service_tests.rs`; `storage_test` returns `TestResult { outcome, leftoverKey }` so the key of a leftover test object reaches the user (SC-004); the test bucket is only checked when a test runs; `endpoint_scope` is set anew when the endpoint, region or addressing changes, and `ConnectionView.endpointScope` marks a local endpoint in the list
- [x] T026 [US1] Usage warning: register a `EntryUsage` provider for owned storage credentials in `src-tauri/src/state.rs` / `src-tauri/src/remote_storage/credentials.rs` so deleting "S3: …" in the password manager warns with the connection name (`src-tauri/src/passwords/usage.rs`) **Note**: the trait `EntryUsage` names a function, not a connection; the warning reads "Wird von diesen Funktionen genutzt: Speicher" (`passwords.owners.<feature>`); `passwords_item_usage` now runs on a blocking thread because the provider reads the vault
- [x] T027 [P] [US1] Settings category "Speicher"/"Storage" in `src/lib/settings/registry.ts` (category id, location, keywords) and `src/components/wm/appRoutes.ts`, i18n in `src/i18n/locales/{de,en}.json` **Note**: category between "Agenten" and "Erweiterungen"; locations `storage.connection`, `storage.storage`, `storage.remove`; `scripts/check-settings.ts` and the e2e scene `settings-categories` list it
- [x] T028 [US1] Settings views in `src/components/settings/storage/` (COSMIC/GNOME style, reuse Group/Row/OptionRow): `StorageListView.vue` (connections with their storages, last test, "neue Zugangsdaten nötig" for `missing`, "wird synchronisiert" for `syncing`), `ConnectionForm.vue` (provider presets AWS/RustFS/other prefill endpoint and addressing, insecure badge for `http`, credential fields, bucket for the test), `StorageForm.vue`, removal confirmation with `storage_removal_preview`; options save on selection (spec 022 FR-004, spec 023 FR-021); only the forms with credentials confirm once, because they are tested before saving **Note**: `StorageListView.vue`, `ConnectionForm.vue` (a new connection also gets its first storage on the tested bucket), `StorageForm.vue` (with "Jetzt testen"), `RemovalView.vue`; overview live through `vault-data-changed` (`src/composables/useStorage.ts`)
- [x] T029 [US1] Run quickstart §1–§4 (§4 manual against RustFS) and note the result in this file, including the time from opening the form to a saved storage (SC-001: under 2 minutes) and the duration of the test (under 10 s) **Note** (2026-10-06): §1–§3 green (`cargo test --lib remote_storage identity::migrations`, `--test vault_upgrade`). §4 against RustFS (`rustfs/rustfs:latest` in Docker): holzi's `S3Store` and test directly: passed in 154 ms, wrong secret `accessDenied`, missing bucket `bucketMissing`, no test object left; through the settings in the e2e scene `storage-two-devices`: wrong credentials named and nothing saved, test and save 436 ms, entry "S3: RustFS" owned by `storage`, removal takes storage and credentials. The 2-minute goal of SC-001 for a person filling the form was not timed by hand (the scene fills it in about 2 s)

**Checkpoint**: US1 works on its own (MVP).

---

## Phase 4: User Story 4 - Speicher auf allen eigenen Geräten (Priority: P2, ships with PR C)

**Goal**: Connections, storages and their credentials reach every own device through the data sync.

**Independent Test**: quickstart §5 step 5 / an e2e scene with two devices.

- [x] T030 [US4] Test in `src-tauri/src/identity/migrations_storage_tests.rs` that both tables are synced (in `SYNCED_TABLES`) and `storage_tests_no_sync` is device-local, and in `src-tauri/src/remote_storage/store_tests.rs` that removing a storage on one replica removes its permissions in the same change set **Note**: synced/device-local in `migrations_storage_tests.rs`; the removal of a storage and its permissions leave their delete markers in the same write (`store_tests.rs`)
- [x] T031 [P] [US4] e2e scene `scripts/e2e/scenarios/storage-two-devices.test.ts` (rig of spec 033): create a storage on device 1, after sync `storage_list` on device 2 shows it with `credentials: present`; mark the RustFS-dependent step as skipped when no container is available **Note**: passes locally in 24 s; RustFS from `scripts/e2e/lib/rustfs.ts`, scenario option `needs.container` skips it without Docker. It also found that Enter in a password field of a `<form>` toggles visibility (haex-ui's eye button has no `type="button"`), so the storage forms handled Enter themselves until haex-ui got the fix (haex-space/haextension#71, pin `83fdb67`); they are native forms again
- [x] T032 [US4] Mobile: `cargo check` for an Android target in CI or locally (`scripts/` mobile check if present) to prove nothing is `cfg(desktop)` (FR-016); note the result **Note** (2026-10-06): not run: the devshell has no Android std, no rustup and no NDK for the C dependencies. By the code: `remote_storage/` has no `cfg` besides tests, and `rusty-s3`, reqwest/rustls and `tokio::net::lookup_host` are platform-neutral; the check belongs to the mobile build spec **Note** (2026-10-07): handed over to spec 043 (holzi for Android): its T010 confirms `cargo check --target aarch64-linux-android` for both feature sets, and its CI job `android-build` (T016) runs clippy for that target on every PR, which covers `remote_storage/` and the bridge from then on
- [x] T033 Run CI parity (`cargo fmt --check`, `pnpm lint:rust`, `cargo test` targeted, `pnpm typecheck`, `pnpm lint`, `pnpm format:check`), commit, open PR C **Note**: PR C = haexmas/holzi#295

---

## Phase 5: User Story 2 - Eine Erweiterung nutzt einen freigegebenen Speicher (Priority: P1, PR D)

**Goal**: The five object methods and the name-only list through the bridge, per-storage permission, own key area.

**Independent Test**: bridge tests with the fake `ObjectStore`; quickstart §5 steps 2–4.

- [x] T034 [P] [US2] Key area (research R4, R5) in `src-tauri/src/extensions/remote_storage_keys.rs` + `remote_storage_keys_tests.rs`: prefix `holzi-ext/<vault_id>/<extension_id>/` from `extensions::ids::extension_id(public_key, name)` and `vault_id = UUIDv5(NS_VAULT, hex(vault_identity.pubkey))` (new `NS_VAULT` in `extensions/ids.rs`); a dev version gets `holzi-ext-dev/<vault_id>/<dev_extension_id>/`, never the installed prefix, also when its manifest names the publisher key and name of an installed extension (Review 2026-10-06); tests: two vault ids give disjoint prefixes, a dev version with an installed extension's key and name cannot list, read or delete the installed one's objects; key rules "nicht leer, nicht mit `/` beginnend, kein leerer Teil, kein `.`/`..`, kein Steuerzeichen U+0000–U+001F/U+007F, kein Rückstrich, mit Präfix höchstens 1024 Bytes"; list prefix may be empty and end in `/`; a corpus of escape attempts (SC-003) all refused; keys from the provider without the prefix dropped **Note**: `storage_vault_id` in `extensions/ids.rs`; `Area::of` takes extension and dev flag from the frame session; the vault id needs the vault identity (8001 without it)
- [x] T035 [US2] Bridge module `src-tauri/src/extensions/remote_storage.rs` + `remote_storage_tests.rs`: `list_backends` (only storages covered by a read permission, items `{id, type: "s3", name, providerName, bucket}` — FR-009a), `upload`, `download`, `list`, `delete` per `contracts/bridge.md`; permission check as in `extensions/web.rs` (`candidates` + temporary + `evaluate`, 1004/1002, an uncovered storage never answers 1001); limits from `sql::exec::limits_of` (upload base64 ≤ `max_response_bytes`, download ≤ `max_response_bytes / 4 * 3`, `timeout_ms`, list ≤ `max_rows`) → 7000; provider errors → 1001 / 2002 `{kind}`; `accessDenied` also writes `storage_tests_no_sync`; tests with an in-memory fake `ObjectStore` injected through `ExtensionHost` (same pattern as `FileDialogs`) **Note**: the host keeps provider and resolver in `ExtensionHost::storage` (`remote_storage_dialog::StorageState`, tests set fakes); `list_backends` asks for `read` on `*` only when no read permission exists at all; a refused call records `accessDenied` (`missingRight` for a missing right) in `storage_tests_no_sync`; a provider that times out is 7000 (R7)
- [x] T036 [US2] Register the nine method names in `src-tauri/src/extensions/bridge/dispatch.rs` (the four management methods answer 8001 until US3), remove `extension_remote_storage_` from `LATER`, update `dispatch_tests.rs`; `extension_bridge_contract` test passes with the names in `specs/017-extension-host/contracts/bridge.md` **Note**: `LATER` is empty now; the 017 contract names the nine methods (`tests/extension_bridge_contract.rs`)
- [x] T037 [US2] Credential leak corpus (SC-002) in `src-tauri/src/extensions/remote_storage_tests.rs`: no response or error of any of the nine methods contains the access key, the secret, the session token, the endpoint or the region (marker search, as `passwords_tests.rs`) **Note**: `no_answer_carries_credentials_endpoint_or_region` runs all nine methods three times (provider fine, refusing, unreachable)

**Checkpoint**: US2 works with storages created in the settings (US1).

---

## Phase 6: User Story 3 - Eine Erweiterung stößt einen neuen Speicher an (Priority: P2, PR D)

**Goal**: add/update/test/remove through a holzi dialog; credentials only typed in holzi.

**Independent Test**: bridge tests answering the dialog through the host; quickstart §5 step 1 and 3.

- [x] T038 [US3] Dialog wait (research R6) in `src-tauri/src/extensions/remote_storage_dialog.rs` (tests in `remote_storage_manage_tests.rs`): like `bridge/methods.rs::dialog_confirm` (`host.open_dialog`, event `extension-storage-request {requestId, frame, kind, proposal, extensionName, otherExtensions}`, `recv_timeout` 300 s, frame close/timeout = cancel → 1002); command `storage_dialog_resolve(requestId, answer)` in `src-tauri/src/remote_storage/commands.rs`; a call whose `config` has `accessKeyId`, `secretAccessKey` or `sessionToken` → 3001 before any dialog (FR-013a); the answer carries `credentials` only from the app-wide modal (T041) **Note**: the dialog state is `StorageState` next to the confirmation dialogs; a closed frame ends both (`drop_dialogs_of`); holzi sends `extension-storage-request-ended` when a request ends so its windows close; `StorageAnswer` is a ts-rs binding
- [x] T039 [US3] Management methods in `src-tauri/src/extensions/remote_storage.rs`: `add_backend` (proposal `{name, type, config: {endpoint?, region?, bucket, pathStyle?}, sameProviderAs?}`; with `sameProviderAs` endpoint, region and addressing come from that connection and `config` with `endpoint`, `region` or `pathStyle` → 3001 before the dialog, without it `region` is required; a proposed endpoint needs `remoteStorage`/`add` for its host (FR-009b: 1004 in the state ask, naming the host, else 1002, before the dialog), with it a local endpoint and `http` to it are allowed, link-local and `http` to a public host → 3001 before the dialog; the new connection stores `endpoint_scope` (T020); dialog; on confirm create connection or reuse one, test, store, grant the extension `readWrite` on the new storage via `commands::permissions::set`, return the list item), `update_backend` (`readWrite` required, dialog, new credentials only from the dialog answer, test on new bucket or credentials), `test_backend` and `remove_backend` (`readWrite`, dialog; remove names other extensions with permissions); tests for confirm, cancel, failed test (nothing created), credentials in the call refused, `sameProviderAs` with `endpoint`/`region`/`pathStyle` refused, `http://127.0.0.1:9000` proposed without `add` → 1004 and no dialog, with `add` for `127.0.0.1:9000` → dialog marks it local and unencrypted, `http://169.254.169.254` with `add` for `*` → 3001 **Note**: in `src-tauri/src/extensions/remote_storage_manage.rs` (500-line rule); the action `add` and the endpoint target are in `extensions/permissions` (model, target); a new connection from a proposal gets its scope from the resolved addresses, its provider name is the endpoint host ("AWS" without endpoint); a storage that fails after its new connection was saved removes that connection again; the new connection must keep the scope the dialog showed, else 3001 before the test (`StorageService::save_proposed_connection`, review of PR D); an AWS proposal by region alone needs `add` for `s3.<region>.amazonaws.com` (or `*`) as well, 1004 naming that host otherwise (operator on #285, review of PR D); `test_backend` answers 2002 with `kind` `accessDenied`, `network` or `bucketMissing`
- [x] T040 [P] [US3] vault-sdk PR (research R11) in `~/Projekte/vault-sdk` (worktree there, haexmas fork, no agent attribution): `AddBackendRequest` without credentials plus `sameProviderAs?`, `UpdateBackendRequest` without credentials, `StorageBackendInfo = {id, type, name, providerName, bucket}`, credential fields of `S3Config` deprecated; changelog entry; then bump the SDK pin in holzi if the e2e fixture needs it **Note**: haex-space/vault-sdk#62 (`S3Proposal`, `sameProviderAs`, `providerName`/`bucket`, credential fields deprecated, management calls without request timeout); no pin bump in holzi, the probe fixture calls the bridge directly
- [x] T041 [US3] Frontend, two steps (research R6, Review 2026-10-06): (1) `src/components/extensions/StorageDialog.vue` mounted in `src/components/extensions/ExtensionFrame.vue` next to `FrameDialog.vue`, listener in `src/composables/useExtensionFrame.ts` (like the `extension-dialog-request` listener): shows the extension name, the proposal (insecure badge for `http`), a choice of an existing connection with the same endpoint and region, and only confirm/cancel; it NEVER contains credential fields; (2) when the confirmation needs credentials (new connection or "new credentials"), `src/components/StorageCredentialsModal.vue` mounted at the app root (not inside `ExtensionFrame.vue`), covering the whole window including tab bar and toolbar, with the credential fields, a summary of the proposal and the hint that holzi asks for credentials only here; closes when the request is cancelled or the frame closes; both answer with `storage_dialog_resolve`; component test that `StorageDialog.vue` renders no input for a secret; i18n keys in `src/i18n/locales/{de,en}.json` **Note**: shared state in `src/composables/useStorageRequests.ts`; the modal is mounted in `src/components/wm/Desktop.vue` next to the permission request; the component test is `scripts/check-extensions-storage-dialog.ts` (template walk: no text or password field over the tab); the probe fixture is unchanged, the scene grants permissions with `extension_permission_set`
- [x] T042 [US3] e2e scene `scripts/e2e/scenarios/extension-storage.test.ts` with the probe fixture extended by `remoteStorage` (`src-tauri/tests/fixtures/extension_e2e/`, rebuilt by `src-tauri/tests/extension_e2e_fixtures.rs`): add with `sameProviderAs` without credentials → dialog without credential fields → id returned; update with "new credentials" → app-wide modal → credentials typed there; add with `accessKeyId` → 3001 and no dialog; add with `http://127.0.0.1:9000` without `add` → permission question naming the host, after granting → dialog marks the endpoint local; skip the object round trip when no RustFS container is available **Note**: passes locally against RustFS (`needs.container`); the probe fixture was not rebuilt, permissions are set by the scene
- [x] T043 Run CI parity, commit, open PR D **Note**: PR D = haexmas/holzi#304 (on main after #295)

**Checkpoint**: all stories work; 017 T106 can be ticked.

---

## Phase 7: Polish & Cross-Cutting Concerns

- [x] T044 Tick `specs/017-extension-host/tasks.md` T106 with a note pointing at PR D and this spec; update R21 if anything changed during implementation **Note**: ticked in 017 with PR D
- [ ] T045 [P] Run quickstart §5 manually against RustFS (all steps) and record the result here (T029 covers §1–§4) **Note** (2026-10-06): not by hand yet; the e2e scenes `extension-storage` and `storage-two-devices` cover §5 steps 1–3 and 5 against RustFS, step 4 (passwords with `*`) is covered by `extensions/passwords_owner_tests.rs`
- [x] T046 [P] Check every touched Rust file stays under 500 lines; split by responsibility where needed (constitution), not just for length **Note**: the bridge tests were split (`remote_storage_test_support.rs`); the check of a proposed endpoint moved to `remote_storage_endpoint.rs` in the review of PR D (`remote_storage_manage.rs` at 446 lines, `remote_storage/service.rs` at 496)
- [x] T047 [US3] A failed test of credentials typed in holzi's window keeps that window open (FR-013b, operator 2026-10-07): `storage_dialog_resolve` answers `StorageTrial`; `remote_storage_dialog.rs` keeps the dialog open (`Dialog`), `remote_storage_manage.rs` retries add and update with corrected credentials (`with_credentials`); `StorageCredentialsModal.vue` shows the outcome text of the settings; tests in `remote_storage_credentials_tests.rs`, e2e `extension-storage` types a wrong secret first

---

## Dependencies & Execution Order

### Phase Dependencies

- Phase 1 (PR A) → no dependencies.
- Phase 2a (PR B) → after PR A is merged; blocks everything that stores credentials.
- Phase 2b → after PR B; blocks US1–US4.
- US1 and US4 (PR C) → after Phase 2b.
- US2 and US3 (PR D) → after PR C (US2 needs storages and credentials, US3 needs the commands and the probe).
- T040 (vault-sdk) can run in parallel to PR D; holzi refuses credentials in calls regardless of the SDK version.
- Phase 7 → after PR D.

### User Story Dependencies

- US1: needs Phase 2. Standalone MVP.
- US4: needs US1 (something to sync).
- US2: needs US1 (a storage to use); independent of US3 (storages come from the settings).
- US3: needs US2's bridge module and US1's commands and probe.

### Within Each User Story

- Tests in the `*_tests.rs` file before or with the implementation; non-trivial logic leaves one runnable check.

### Parallel Opportunities

- T002–T005 (docs) in parallel.
- T011 and T013 in parallel to T010/T012.
- T019 and T020 in parallel; T020a after T020; T021 after T019 and T020a.
- T027 in parallel to T023–T026.
- T034 in parallel to T035's skeleton; T040 in parallel to all of PR D.

---

## Parallel Example: User Story 2

```text
Task: "T034 Key area and key rules in src-tauri/src/extensions/remote_storage_keys.rs"
Task: "T040 vault-sdk PR without credentials in AddBackendRequest/UpdateBackendRequest"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. PR A (docs), PR B (Z14), Phase 2b.
2. US1: storages in the settings, tested, credentials owned by `storage`.
3. Stop and validate with quickstart §1–§4.

### Incremental Delivery

1. PR C = Phase 2b + US1 + US4 → holzi speaks S3 for itself, on all devices.
2. PR D = US2 + US3 → extensions use and propose storages; 017 T106 done.

---

## Notes

- Never put a secret into a log line, an error message, a ts-rs type sent to the window (except the dialog/form input going _to_ Rust) or a test fixture committed to git.
- `cargo test` regenerates `src/types/bindings/*.ts` with trailing whitespace; strip it or check out unchanged bindings before committing (new bindings: strip, do not check out).
- Commit messages: Conventional Commits, no agent attribution.
