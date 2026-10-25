# Feature Specification: Frontend Onboarding — Landing, Create, Open, Connect

**Feature Branch**: `001-frontend-onboarding`
**Created**: 2026-09-04
**Status**: Draft
**Input**: Brainstorming session captured in this repository's conversation on 2026-09-04. Design orientation follows `haex-vault 2.x (Legacy)`'s landing/onboarding pattern, adapted to holzi's identity, storage, and pairing model.

**Relationship to prior docs**:

- Consumes decisions from [`docs/design/founding.md`](../../docs/design/founding.md) and [`docs/plans/2026-09-04-v1-scope-design.md`](../../docs/plans/2026-09-04-v1-scope-design.md).
- Revises `founding.md` §2.2 ("Device equals relay equals Tauri application") to allow multiple SQLite database files on disk per install, with exactly one active at runtime. See **Assumptions** below.

**V1 supersession notice (2026-09-06)**: The paper-seed/federation-root
onboarding material retained in this draft is historical and non-normative. It
is superseded by `v1-scope-design.md` §4 and MUST NOT be implemented. V1
Genesis creates a fresh per-SQLite identity; `.db` backup recovery opens the
copied database directly, reusing or minting the local `known_devices` row
before any network endpoint starts. No rekey or restore pairing is performed.
The same notice applies to the shared-type, command, plan, quickstart, and
task documents in this feature directory.

## User Scenarios & Testing _(mandatory)_

### User Story 1 — Anlegen (Genesis of a new federation) (Priority: P1) 🎯 MVP

The operator installs holzi for the first time on a device and creates a fresh federation. On the landing page, they choose **Anlegen**, name the instance, set a passphrase, and are dropped into the running instance with fresh Nostr and iroh identities stored in the encrypted database.

**Why this priority**: Without Anlegen, no federation exists. It is the entry point for every subsequent scenario and must ship for the v1 walking-skeleton.

**Independent Test**: On a fresh install with an empty `instances/` directory, complete the Anlegen sheet with valid inputs; verify (1) a new `<name>.db` file appears in `<AppLocalData>/instances/`, (2) the encrypted database contains a `vault_identity` row and a `known_devices` row keyed by the local installation UUID, and (3) the app navigates to the federation surface with the Nostr relay and iroh peer running.

**Acceptance Scenarios**:

1. **Given** no existing instance and the Anlegen sheet open, **When** the operator submits name and passphrase, **Then** the instance file is created with fresh per-instance identities, the app unlocks it, and navigates to the federation view.
2. **Given** the Anlegen sheet open, **When** the operator submits a name that already exists in `instances/`, **Then** an inline error is shown and no file is created.
3. **Given** the Anlegen sheet is dismissed before submission, **Then** no instance file is created.

---

### User Story 2 — Verbinden (Join existing federation via pairing) (Priority: P1) 🎯 MVP

The operator has a parent instance whose `peer_instances` record carries `pairing-authority` and installs holzi on a second device (joiner). On the joiner's landing they choose **Verbinden** and are shown a QR scanner backed by the joiner's camera. On the parent device the operator opens the pairing surface, which displays a short-lived QR encoding the pairing token; the joiner scans it (or, as a fallback for hosts without a working camera, pastes the same token as text). The joiner sets a local passphrase; the joiner's new instance and mutually instance-signed `peer_instances` records are created, and federation state syncs from the parent via `haex-crdt`. QR is the primary flow because Verbinden's target platforms include mobile, where typing tokens is impractical; the text fallback exists so a webcam-less desktop or a broken camera driver does not block onboarding.

**Why this priority**: This is the second half of the v1 walking-skeleton (`v1-scope-design.md §11`: two devices paired into one federation exchanging a ping). Without Verbinden, holzi is single-device.

**Independent Test**: With a parent device running a registered instance and displaying a valid pairing token, on the joiner complete the Verbinden sheet; verify (1) a new `<name>.db` file appears on the joiner, (2) the `peer_instances` registry in `haex-crdt` contains both devices on both sides after sync, (3) the joiner's presence event appears in the parent's registry.

**Acceptance Scenarios**:

1. **Given** a valid, unexpired, unused pairing token from the parent, **When** the joiner submits it with a local passphrase, **Then** pairing succeeds and the joiner is dropped into the federation view.
2. **Given** an expired or already-consumed token, **When** the joiner submits it, **Then** an explicit error identifies the token state and no instance file is created.
3. **Given** a token with a mismatched federation epoch (e.g., stale token), **When** the joiner submits it, **Then** pairing fails with a clear "federation state changed" error.

---

### User Story 3 — Öffnen (Import external `.db` file from filesystem) (Priority: P2)

The operator has an instance file (`.db`) on the filesystem — moved from another machine, restored from backup, or simply outside the managed directory — and wants holzi to manage it. On the landing they choose **Öffnen**, pick the file via OS file dialog, and holzi copies it into `<AppLocalData>/instances/` under its original name (or a de-duplicated variant). On next open, the `DatabaseBootstrap` hook looks up this installation's UUID in the copied DB's local `known_devices` column; it reuses the row when the copy came from the same installation, or inserts a fresh vault-device UUID row when it came from another installation. The returned UUID becomes HLC's node ID for that open. The DB unlocks with the original passphrase. That is the entire direct-copy flow: no rekey, no attestation, no restore-pairing handshake.

The vault identity in the DB is authoritative and already carried across by the copy, so other replicas of the same vault accept a connection from this new install via proof-of-possession of the vault key. A vault-identity rotation rejects copies with the old key. The vault-device UUID is per-installation: same-installation copies reuse the local row, while different installations get different HLC node IDs.

**Why this priority**: Local portability. Holzi has no export command because the `.db` file _is_ the export; Öffnen is the corresponding import. Adopting a database copied from another machine is a supported way to add a replica.

**Independent Test**: Import a valid throwaway database from the same installation and verify copy (not move), unchanged source, immediate list refresh, and reuse of the existing local `known_devices` row and vault-device UUID. Repeat under a second installation and verify that unlocking inserts a new local row with a vault-device UUID distinct from the source's. Verify that reopening under either installation reuses its row and UUID. No second database is created.

**Acceptance Scenarios**:

1. **Given** a valid `.db` file (from this machine or another install), **When** the operator imports and unlocks it, **Then** holzi reuses the matching local `known_devices` row or inserts one with a fresh vault-device UUID when no local row exists, opens the DB, and shows the federation view. No token is requested, no parent must be reachable.
2. **Given** a file whose name conflicts with an existing instance, **When** the operator confirms the import, **Then** the operator is prompted for either overwrite, rename, or cancel — default behavior is rename with a numeric suffix, no silent overwrite.
3. **Given** a structurally valid regular `.db` file that was imported successfully, **When** the operator attempts to unlock it with an incorrect passphrase, **Then** `open_instance` rejects the attempt with an explicit error, retains the imported copy for a later unlock attempt or explicit discard, and leaves any currently active instance unchanged.
4. **Given** an imported copy that has been opened once (so `known_devices` contains a row for this installation), **When** the operator closes and reopens it, **Then** the same vault-device UUID is used; no new row is inserted.

---

### User Story 4 — Zuletzt verwendet + Unlock (Priority: P1) 🎯 MVP

On every subsequent launch, the landing shows the operator's instances (from `<AppLocalData>/instances/`, sorted by last-access descending). Clicking one opens the Unlock sheet; entering the passphrase brings up the federation view.

**Why this priority**: Without Unlock, every launch would force re-Anlegen. This is the daily entry point and must ship for v1.

**Independent Test**: With at least one instance in `instances/`, launch the app; verify (1) the instance appears in the list with its correct alias and last-access timestamp, (2) clicking opens the Unlock sheet, (3) correct passphrase unlocks and navigates to the federation view, (4) wrong passphrase and missing vault show distinct inline errors.

**Acceptance Scenarios**:

1. **Given** one or more instances present, **When** the operator launches the app, **Then** the list is populated ordered by last-access desc.
2. **Given** the Unlock sheet with the correct passphrase, **When** the operator submits, **Then** the instance opens and navigation succeeds within 2 seconds on desktop hardware.
3. **Given** the Unlock sheet with a wrong passphrase, **When** the operator submits, **Then** an inline error appears and no state changes.
4. _(Superseded by spec 013 FR-010 and [ADR 0003](../../docs/adr/0003-one-vault-session-per-app-process.md): with an instance already active, `open_instance` refuses with `VaultAlreadyActive`; a different instance opens only in a new process after the close.)_

---

### User Story 5 — Aus Paper-Seed wiederherstellen (Recover) (Historical — superseded, not a v1 requirement)

This user story belongs to the superseded paper-seed/federation-root design and is retained only as historical context. V1 recovery is the Öffnen flow in User Story 3: import a `.db`; the local `known_devices` row is reused or freshly inserted on first open, and the vault identity in the copy authenticates the new install to any other replicas.

**Why this priority**: Not applicable to v1. The historical flow is blocked; backup recovery is covered by User Story 3.

**Independent Test**: Not applicable; see the User Story 3 import test.

**Acceptance Scenarios**: None for v1; this historical flow MUST NOT be implemented.

---

### Edge Cases

- **Passphrase entered incorrectly during Create**: two-field entry with match check; submit disabled until match.
- **Passphrase field visibility**: masked by default, reveal on hold (mouse) or tap (mobile); never persisted in the browser autofill store.
- **Instance name collision with reserved filename** (`.trash`, files starting with `.`, path traversal): rejected client-side with clear error before any command is sent.
- **Instance directory changed outside holzi** (e.g., another process creates a file in `instances/`): direct external mutations are unsupported in v1 and do not emit `instance-list-changed`; the new file is discovered on the next app launch or explicit list refresh. Operators import external files through **Öffnen**, whose backend command emits the event.
- **App closed during Anlegen**: on next launch, an orphan Genesis file may exist. Startup cleanup deletes any Genesis file whose creation flag `.pending` still exists. Imports use a staged file (`.db.importing`) that is renamed atomically; startup deletes any stray staged file.
- **Two instances open concurrently**: _(superseded by spec 013 FR-010 and [ADR 0003](../../docs/adr/0003-one-vault-session-per-app-process.md): a second `open_instance`/`create_instance` is refused with `VaultAlreadyActive`, or `VaultClosed` while a close is under way; no in-process switch exists.)_
- **Mobile foreground/background** for Anlegen: if the app is backgrounded during Genesis, the same pending-flag mechanism applies. No changes to relay-lifetime rules for mobile beyond `v1-scope-design.md §7`.

## Requirements _(mandatory)_

### Functional Requirements

**Landing page (all first-launch and subsequent launch)**

- **FR-001**: The landing page MUST display three primary call-to-action buttons in fixed order: **Anlegen**, **Öffnen**, **Verbinden**.
- **FR-002**: The landing page MUST display a "Zuletzt verwendet" list of all instances in `<AppLocalData>/instances/`, sorted by last-access descending.
- **FR-003**: The landing page MUST render a version string sourced from `@tauri-apps/api/app::getVersion()`.
- **FR-004**: The landing page MUST work with an empty `instances/` directory (the list section MUST hide, not render an empty state that competes with the CTAs).

**Anlegen (Create)**

- **FR-005**: The Anlegen action MUST open a Sheet for a fresh Genesis instance. The historical Recover mode is not available in v1.
- **FR-006**: The Anlegen sheet MUST require an instance name (alphanumeric plus dash/underscore, ≤64 chars, unique within `instances/`) and a passphrase entered twice.
- **FR-007**: On successful submit, the backend MUST call `haex-crdt`'s `Database::open` with the passphrase, a `DatabaseConfig::bootstrap` hook, signature provider, migration source, and trigger version. After migrations and before HLC initialization or signed writes, that hook MUST atomically create the vault identity keypair and singleton `vault_identity` row, then look up or mint the local `known_devices` row, and return its vault-device UUID. `Database::open` then initializes HLC from that UUID and installs triggers; a later open failure does not undo a successfully committed bootstrap transaction, so Genesis cleanup removes the candidate database and marker. Genesis writes the self-`peer_instances` record after a successful open and activates the runtime. No paper-seed or confirmation step exists in v1.
- **FR-008**: If Anlegen is cancelled before submission, no instance file is created. If creation fails, the backend MUST remove any partial file and marker; startup cleanup MUST apply the same rule after a crash.

**Öffnen (Import external `.db` file)**

- **FR-010**: The Öffnen action MUST open a Sheet that invokes the OS file picker via `@tauri-apps/plugin-dialog`, restricted to `.db` extension.
- **FR-011**: Upon selection, the backend MUST validate the source as a regular `.db` file before copying. SQLCipher credential validation MUST occur in `open_instance`, using the passphrase entered in the Unlock sheet; a failed unlock attempt MUST return an explicit error without deleting the imported copy. On successful unlock, the `DatabaseBootstrap` hook looks up the local installation UUID in the `known_devices.installation_uuid` primary-key column; it reuses a matching row or inserts a fresh row and returns its vault-device UUID to the database opener. The primary-key value is carried as row identity in CRDT payloads (`row_pks`), while it cannot be changed through a separate CRDT column update. No rekey, no attestation, no restore-pairing handshake. The vault identity keypair in the copy is authoritative for authenticating to other replicas via proof-of-possession; rotation of that key rejects stale copies. Deletion of the imported file requires an explicit discard action or conclusive validation that the file is not a holzi instance.
- **FR-012**: The backend MUST copy (not move) the file into `<AppLocalData>/instances/` preserving its filename basename. Any `<AppLocalData>/installation-id` sidecar is installation-scoped and MUST NOT be created or modified per-vault; if it does not yet exist for this install, it is minted during the next `open_instance` (see FR-011). The source file is never modified or moved.
- **FR-013**: On name collision, the backend MUST prompt via return value; the frontend MUST offer overwrite / rename / cancel; default MUST be rename with numeric suffix (`<name>-2.db`). Silent overwrite is prohibited.
- **FR-014**: After successful copy, the `instances/` list MUST refresh (via `instance-list-changed` event) so the imported file appears immediately.

**Verbinden (Join federation via pairing)**

- **FR-015**: The Verbinden action MUST open a Sheet whose primary control is a QR scanner reading the pairing token from the joiner's camera via the standard `navigator.mediaDevices.getUserMedia()` Web API on every platform (desktop webcam or mobile camera, whichever the Tauri WebView exposes). The Sheet MUST also offer a text-input fallback for the same token so hosts without a usable camera can complete pairing. The parent device MUST expose a matching "Pairing anbieten" surface that renders the token as a QR code (and, for symmetry, as copyable text) for the joiner to scan; that parent-side surface lives outside this spec's landing scope and is covered by the federation-view spec.
- **FR-016**: The Verbinden sheet MUST require: instance name (as FR-006), passphrase (as FR-006), and a non-empty pairing token supplied either by successful QR decoding or by the text-input fallback. Both acquisition paths MUST pass the same token string to `CreateMode::Join`.
- **FR-017**: On submit, the backend MUST create a new local instance, generate device-scoped Nostr and iroh keypairs, connect to the parent device's relay using the token's contact hint, sign the canonical pairing transcript, wait for the parent's co-signature, and persist mutually signed `peer_instances` records into `haex-crdt`.
- **FR-018**: _(historical — the superseded restore-pairing/rekey flow is out of scope; the QR/token Join flow in FR-015–FR-017 remains in MVP.)_

**Zuletzt verwendet + Unlock**

- **FR-019**: Clicking an instance in the list MUST open the Unlock sheet with only a passphrase field.
- **FR-020**: On correct passphrase, the backend MUST unlock SQLCipher, start the Nostr relay endpoint, start the iroh peer, and mark the instance as active in `AppState`. The frontend MUST navigate to `/federation/<instance-id>`.
- **FR-021**: On an incorrect passphrase, the sheet MUST show the localized wrong-passphrase error. If the selected vault file is missing, it MUST show a distinct missing-file error; storage, corruption, and compatibility failures MUST likewise retain their typed, localized errors. This intentionally exposes whether the selected vault exists so the operator can diagnose synchronization and file problems.
- **FR-022**: _(superseded by spec 013 FR-010 and [ADR 0003](../../docs/adr/0003-one-vault-session-per-app-process.md): if an instance is already active, `open_instance` now refuses with `VaultAlreadyActive` instead of switching to the requested one — the atomic switch described here is removed. The frontend still MUST NOT orchestrate a separate close/open sequence, but there is no in-process sequence to orchestrate any more: a different vault is only reachable after the active one is fully closed, in a new process.)_
- **FR-023**: The list MUST support a per-item context menu with "In Papierkorb verschieben". Removing an item while retaining its file is not supported in v1 because the list is a directory scan and no exclusion metadata is persisted. Silent hard-delete MUST NOT be an option.

**Cross-cutting**

- **FR-024**: All backend mutations to `instances/` (create, open, close, import, trash) MUST emit `instance-list-changed` events; the Pinia store MUST subscribe and re-sync. These producers cover all in-scope mutations, so no filesystem watcher is required.
- **FR-025**: The frontend MUST NEVER pass managed-instance paths to backend commands; all instance-management commands MUST use instance names (basename without `.db`). `import_instance_file` MAY receive the external `source_path` returned by the OS file picker, while destination resolution and source-file validation remain backend authority.
- **FR-026**: All UI copy MUST be locale-driven via `@nuxtjs/i18n` with `de` and `en` locales at minimum; `de` is the default.
- **FR-027**: All icons MUST be delivered from the local bundle (`@iconify-json/lucide` package installed offline); no runtime request to any external icon API is permitted.

### Key Entities

- **Instance**: a `<name>.db` file in `<AppLocalData>/instances/`, containing a SQLCipher-encrypted `haex-crdt` store with federation state (`peer_instances` registry, revocation epochs, capability grants, chat/session history). Each instance is one identity in one federation.
- **InstanceInfo**: metadata surface for the frontend list: `{ name: string, alias: string, lastAccess: ISO8601, sizeBytes: number }`. `name` is the filename basename without `.db`; `alias` is the non-secret user-visible label, initially defaulting to `name`. No secrets.
- **CreateMode**: `Genesis | Join { token: string }` — the two v1 initialization modes across Anlegen + Verbinden. Backup restore is handled by Öffnen (import + open), not by Recover.
- **PairingToken**: opaque short-lived value issued by a parent device, carrying (in encoded form) a Nostr contact hint, a one-time nonce, an expiry, and the current federation epoch.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: A fresh operator can complete Anlegen (Genesis) end-to-end in under 90 seconds on desktop hardware.
- **SC-002**: With a valid pairing token, Verbinden completes in under 30 seconds on the same local network, from token entry to federation view.
- **SC-003**: The landing page renders and becomes interactive within 500ms of app launch on desktop, 1500ms on mid-range Android hardware.
- **SC-004**: Unlock of a healthy instance succeeds within 2 seconds on desktop hardware, 4 seconds on mobile.
- **SC-005**: No user-facing UI text is displayed in a language other than the active locale (verified by i18n key coverage ≥100% for `de` and `en`).
- **SC-006**: No network request originates from the client to any icon or font CDN (verified by Playwright network-log assertion in the E2E suite).
- **SC-007**: The landing page renders correctly with 0, 1, and 10+ instances in the list (visual regression via Playwright screenshot).

## Assumptions

- The founding-doc assertion "device equals relay equals Tauri application" (`founding.md` §2.2) is refined to: **the _active_ instance equals the running Nostr relay endpoint equals the running iroh peer**. The Tauri application is a container that may hold multiple `.db` files on disk, with exactly one active at any given time. This revision was surfaced during the 2026-09-04 brainstorming and takes precedence for v1.
- `<AppLocalData>` on each platform is the app-private data directory Tauri resolves via `BaseDirectory::AppLocalData`: Linux `$XDG_DATA_HOME/<bundle_identifier>/` (normally `~/.local/share/<bundle_identifier>/`), macOS `~/Library/Application Support/<bundle_identifier>/`, Windows `%LOCALAPPDATA%\<bundle_identifier>\`, Android app-private storage, and iOS app-sandbox `Library/Application Support/<bundle_identifier>/`. The `<bundle_identifier>` is the identifier configured in `tauri.conf.json`; all platforms append it before `instances/`.
- [`haex-crdt` at `1c069ef0ea19143af2748f40fc41cba05c94dbe1` (`Cargo.toml`, package 0.4.0)](https://github.com/haexmas/haex-crdt/blob/1c069ef0ea19143af2748f40fc41cba05c94dbe1/Cargo.toml), extracted from `haex-vault 2.x` per `v1-scope-design.md §5`, provides the SQLite + CRDT layer with SQLCipher at-rest. Its single database entry point, `Database::open(DatabaseConfig)`, accepts the passphrase (via `SqlCipherKey`), `DatabaseConfig::bootstrap`, a `SignatureProvider`, and a `MigrationSource`. Mode-specific orchestration (Genesis writes a self-record; Join runs the pairing transcript and applies mutually signed peer records) is Holzi's concern above that entry point, not a `haex-crdt` API. See `contracts/tauri-commands.md`.
- Pairing offers QR scanning through the joiner's camera as the primary path, with text-token input as fallback. Both paths carry exactly the same encoded token; the QR is only a transport for that string. The scanner uses `html5-qrcode` on every platform (matching haex-vault 2.x); a mobile-native barcode plugin is documented as a contingency in [`research.md`](./research.md) and is not v1. The QR-rendering surface on the parent device (which produces the token the joiner scans) lives outside this spec's landing scope and is covered by the federation-view spec.
- shadcn-vue components are copy-in under `src/components/ui/`. The initial component set is: `button`, `card`, `sheet`, `dialog`, `input`, `label`, `radio-group`, `sonner` (toasts), plus a custom `stepper` composed from `progress` + `button`. Later stories may add more.
- No cross-device blob or stream transfer is required by any onboarding flow. Verbinden's initial `haex-crdt` sync uses whatever transport `haex-crdt` chooses internally — not `blob.offer` (post-v1 per `v1-scope-design.md §2`).
