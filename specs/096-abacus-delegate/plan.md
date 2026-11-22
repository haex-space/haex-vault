# Implementation Plan: Abacus AI CLI delegate

**Branch**: `096-abacus-delegate` | **Date**: 2026-11-22 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/096-abacus-delegate/spec.md`

## Summary

The Abacus AI CLI (`abacusai`) becomes the third delegate, on the tool manager of spec 094. It is installed per vault from
the vendor's release host, verified against the SHA-256 held in the tested-version list (the maintainer script first
verifies the vendor's Sigstore signature), run with the vault's home through the existing containment layer, and answers
chat requests through the tool's JSON-lines mode, in which it asks the host to approve each tool call. The user connects
from the app's UI with their normal account credentials: HaexVault runs the vendor's own device sign-in (e-mail address, then
an address and a code confirmed in the browser), stores the resulting device code as an entry of the password manager, and
hands it to the process for one run only, never writing it to a file. An API key is not used (it is billed per request, the
sign-in unlocks free models). The tool's own interactive login is not used either: it needs a terminal and stores a
device-bound file.

Technical approach (details in [research.md](research.md)): a vendor definition in `catalog.json` plus four small manager
additions the release format and the measurement gate need (an x64 variant for CPUs without AVX2, a per-program requirement
of AVX2, zip archives for Windows, a vendor with no tested version yet); a new
`DelegateVendor::Abacus` and every match that names the vendors; a module that saves, reads and deletes a delegate credential
as an owned password-manager entry (generic for later vendors, used by Abacus only); a small client of the vendor's sign-in
service and the connect flow on top of it; an adapter that speaks the JSON-lines
protocol and calls the existing approval bridge; the connect dialog and vendor lists in the frontend; the maintainer script
learns the Sigstore check; the measurement and the first catalog entry follow the operator's authenticated runs.

## Technical Context

**Language/Version**: Rust 1.77.2 edition 2021 (`src-tauri`), TypeScript 5 / Vue 3 / Nuxt (frontend), TypeScript and Python
scripts under `scripts/cli-tools/`.

**Primary Dependencies**: all already declared and reused: `reqwest`, `sha2`, `tokio`, `tokio-util`, `serde_json`, `tar`,
`flate2`, `zip 8` (deflate; used for the Windows archives), `semver`, `ts-rs`, `wiremock` (dev). No new dependency in the app.
The maintainer script needs `sigstore` (Python, `sigstore verify identity`) or `cosign` on the maintainer's machine; the
app does not (as it needs no PGP library for Claude Code).

**Storage**: files and the password manager. Files: `<vault>/cli-tools/abacusai/<version>/` and `<vault>/home/` as in spec 094. The credential: one owned entry of the password manager (`haex_passwords_*` tables, synced). No new table, no change to
`providers` (a synced table; the row of the vendor holds `credentials = NULL`).

**Testing**: `cargo test` with sibling `*_tests.rs` files; `wiremock` stand-in release host and the existing unix stub
delegates in `src-tauri/tests/`, with a stub that speaks the JSON-lines protocol; the maintainer script against a recorded
Sigstore bundle and archive; the sign-in client against a `wiremock` double of the service; Node checks for frontend logic; the e2e suite with the stand-in host. Real-tool checks stay
manual ([quickstart.md](quickstart.md)).

**Target Platform**: desktop (Linux, macOS, Windows). Android and iOS: unavailable (`cli_delegates` capability). CI is Linux
only; the Windows zip path is covered by unit tests with a zip fixture and by the operator's run (quickstart §9).

**Project Type**: desktop app (Tauri backend plus Nuxt frontend).

**Performance Goals**: install under 5 minutes at 50 Mbit/s for 79 MB (spec SC-001); the 13 MB native library the program
extracts at each start is one extra write per request.

**Constraints**: nothing outside the vault folder is written (spec FR-013); the credential is never written to a file, logged
or sent to the frontend; no reliance on the host leaving any variable unused (the base layer removes the host's `ABACUS_*`,
`ABACUSAI_*`, `CODELLM_*` variables); each source file under about 500 lines; cargo with the memory-limited flags noted
for this machine (`-j 1`, 10 GB scope).

**Scale/Scope**: a third vendor; about 79 MB per version download, about 158 MB unpacked, per vault.

## Constitution Check

Spec Kit constitution (`.specify/memory/constitution.md` 1.4.0) and the spaex constitution (`.spaex/constitution.md`).

| Principle / rule                                                                               | Result | Note                                                                                                          |
| ---------------------------------------------------------------------------------------------- | ------ | ------------------------------------------------------------------------------------------------------------- |
| I. No secrets in git                                                                           | PASS   | The catalog holds public URLs and SHA-256 values; the credential lives in the vault; no key is used in tests. |
| II. No local absolute paths in versioned config                                                | PASS   | Paths are `<vault folder>`-relative; the catalog has URLs only.                                               |
| III. Identity device-independent                                                               | PASS   | The credential entry syncs with the vault; install state stays per device (spec 094 FR-018).                  |
| IV. Cross-repo references pin immutable revisions                                              | PASS   | The catalog pins a vendor version and checksums; the signing identity is pinned by pattern with the tag.      |
| V, VI, VII, VIII                                                                               | PASS   | No allowlist, instruction, relay or concealment change; a missing release host never blocks local work.       |
| spaex: smallest correct implementation, no unneeded dependency                                 | PASS   | An enum variant instead of a vendor trait; no new crate.                                                      |
| spaex: blocking work off the async executor                                                    | PASS   | hashing, unpacking and file removal stay in `spawn_blocking`; the stream reader is async.                     |
| spaex: tests as `*_tests.rs` beside the module, never inline                                   | PASS   | planned per module.                                                                                           |
| spaex: UI feedback (errors as toast, settings apply on selection, no inline success text)      | PASS   | The connect dialog reports errors as toasts like the other two vendors.                                       |
| spaex: run formatter, `cargo check`, `clippy`, `cargo test`, prettier on touched Markdown/JSON | PASS   | tasks include the commands and the Android `clippy` for cfg branches.                                         |
| spaex: no LLM-agent references in project artifacts                                            | PASS   | The tool is named by its product name only.                                                                   |

Re-check after design: unchanged. No violation needs the Complexity Tracking table.

## Decisions (resolved; the operator-open ones carry a gate)

- **D1 - Credential: RESOLVED 2026-11-22 (operator).** The account's own sign-in, never an API key: a key is billed per
  request, the sign-in unlocks models that cost nothing. The stored thing is the device code the vendor's sign-in returns
  (the value `CODELLM_DEVICE_CODE` carries), kept in the password manager. HaexVault runs the sign-in's two calls itself
  (research §3), like the Codex flow runs its device authentication, because the tool's own login needs a terminal. **Risk**:
  the two endpoints are private and undocumented, so a vendor change breaks connecting without a new program version.
  **Gate**: quickstart step 3.0 must show that the service answers a client other than the vendor's own and step 6 whether a
  code works on another device (otherwise each device connects once, data-model §3); if the service refuses other clients
  there is no UI route, and the operator decides what replaces it. An API key is not an alternative.
- **D2 - Vendor in the delegate layer: RESOLVED.** `DelegateVendor::Abacus` and extended matches (research §9). A vendor
  trait would serve three vendors with different protocols (a PTY OAuth flow, a device-auth subprocess, a JSON-lines
  process) and abstract nothing they share. The matches are the cost, and they are listed so none is missed.
- **D3 - Platform variants: OPEN for macOS x64.** `linux-x64-baseline` (x64 without AVX2, common in virtual machines) is
  added. Do not offer the macOS x64 archive on hosts without AVX2 until quickstart verifies the vendor archive's minimum
  CPU requirement or a baseline archive is added and measured. musl and `win32-x64-baseline` remain out of scope; a host the
  build does not run on is reported as `start_failed` with the system's reason.
- **D4 - Windows: RESOLVED.** The zip archive kind and the Windows keys are the last change; nothing of it can run in CI, so
  it is its own pull request with the operator's run (quickstart §9).
- **D5 - Finding the credential entry: RESOLVED.** Filter the headers an Internal caller sees (its own owned entries) by tag;
  no id on the provider row, so no schema change to a synced table. A duplicate from two devices is resolved by the newest
  `updated_at` and purged at the next save (research §10).
- **D6 - Permission posture: OPEN; release gate.** The adapter uses `--permission-mode supervise`, forwards every
  `can_use_tool` request to `request_approval`, never answers with a persistent variant (`allowAlways`, `allowYolo`, or rule
  variants), and refuses a mismatched mode in `system/init`. This does not prove that read-only tools reach the approval
  bridge. Before enabling Abacus in a user's `always ask` posture, quickstart §3.4 must demonstrate approval for both a
  read-only tool and a changing tool. If read-only tools bypass the bridge, the backend must be unavailable while `always
ask` is active until an enforcement path is implemented and verified. Do not treat `supervise` alone as satisfying FR-010.
- **D7 - Models: RESOLVED for the first delivery.** One entry, the account's default model (no `--model`), as Codex has its
  fixed entry. A list from the account (`get_model_catalog` control request) is added only if quickstart step 3.5 shows it
  works without spending credits.
- **D8 - When the first catalog entry lands: RESOLVED.** After the measurement of spec 094 FR-021 on a real device code (quickstart
  §3). Until then every change is developed against test vendor definitions; the operator measures with the same isolation
  the research used, so no unmeasured version is ever installable.
- **D9 - Claude Code and Codex credentials: OPEN for the operator.** The operator wants every delegate provider's credential in
  the password manager. This spec does it for Abacus with a generic module; moving the other two touches their connect
  flows, `build_adapter` and sync of the provider rows, and is out of scope ("changes to Claude Code and Codex"). Until agreed
  there are two places. Recorded as a follow-up for spec 094's T071 list.

## Project Structure

### Documentation (this feature)

```text
specs/096-abacus-delegate/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   └── tauri-commands.md
├── checklists/requirements.md
└── tasks.md              # created by the tasks command, not by this one
```

### Source Code (repository root)

```text
src-tauri/src/
├── cli_tools/
│   ├── catalog.json / catalog.rs      # vendor "abacusai" (entry after measurement); ArtifactKind::Zip; variant key validation
│   ├── platform.rs                    # LinuxX64Baseline, candidates(), AVX2 check
│   ├── unpack.rs / install.rs         # zip unpack (escape refusal, size accounting)
│   └── status.rs / installed.rs       # platform lookup through candidates()
├── adapters/cli_delegate/
│   ├── mod.rs                         # DelegateVendor::Abacus, matches in list_models / stream_chat
│   ├── abacus.rs                      # spawn, JSON-lines reader/writer, approval mapping, error mapping (new)
│   ├── abacus_tests.rs                # with the stub that speaks the protocol
│   ├── autonomy.rs, haex_mcp.rs, process.rs   # matches on the vendor
│   └── (claude.rs, codex.rs untouched except where a match names every vendor)
├── providers/
│   ├── delegate_credential.rs         # save / read / delete as an owned password-manager entry (new)
│   ├── abacus_signin.rs               # client of the two sign-in calls (reqwest), polling, cancel token (new)
│   ├── connect.rs                     # email arg, Abacus branch of connect_cli_delegate, cancel_cli_delegate_connect; add.rs, mod.rs: vendor parse, build_adapter
│   └── mod.rs                         # delete_provider also deletes the entry
├── chat/model_loading.rs              # the legacy-id check names vendors
├── agent_profiles/delegates.rs, mcp_server/{delegates,registry}.rs   # vendor matches
└── lib.rs                             # register cancel_cli_delegate_connect

src-tauri/tests/cli_delegate_abacus_*.rs   # stub program, containment and log-search tests for the third vendor

scripts/cli-tools/
├── add-version.ts, lib/entry.ts       # Sigstore verification branch for abacusai
└── allowed-residuals.json             # only entries the operator agrees to after the authenticated series

src/
├── lib/cli-tools.ts                   # CliToolVendor includes 'abacusai'
├── composables/{useProviders,useModelInventory,useToolNotice}.ts, lib/actions/settingsActions.ts
├── components/settings/ConnectDelegateProvider.vue   # e-mail step, address and code display, per-vendor maps
├── components/chat/MessageList.vue, i18n/locales/{en,de}.json   # chat.model.delegate.abacusai, connect and error texts
└── types/bindings/                    # ts-rs: CliToolVendor values, ConnectCliDelegateArgs.email
```

**Structure Decision**: the manager gets data plus the four additions; the delegate layer gets one adapter file and an enum
variant; the credential gets one small module. Frontend changes stay in the existing providers settings page and the lists
that name the vendors.

### Delivery in stacked pull requests

A. Manager: platform variant, zip kind (with fixtures), vendor definition for tests, maintainer script. Independent of the
adapter. B. Credential module, sign-in client and connect flow, `delete_provider` extension, the vendor lists and the connect
dialog (gated by D1: step 3.0 of the quickstart). C. Adapter, approval mapping, model entry, stub-based tests. D. The operator's authenticated measurement,
the first catalog entry, residual entries, glossary entries in `CONTEXT.md`, the follow-up list (D9, model list). E. Windows (zip) with the operator's run.

## Complexity Tracking

None. The four manager additions follow from the vendor's release format and from the measurement gate (research §2, §8); the enum variant is the smallest
change (D2).
