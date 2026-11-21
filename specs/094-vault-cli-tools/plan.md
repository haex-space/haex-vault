# Implementation Plan: CLI tools per vault

**Branch**: `094-vault-cli-tools` | **Date**: 2026-11-16 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/094-vault-cli-tools/spec.md`

## Summary

Each vault gets its own installation of the delegate CLIs (Claude Code, Codex) under `<vault folder>/cli-tools/`, and its
own home folder `<vault folder>/home/`. Every delegate process is started through one containment interface
(`Containment`, spec FR-020). Its only implementation here is the portable base layer: it relocates only what a tool
needs relocated, in the child process's environment alone, passes through what the tool needs from the host (proxy,
certificates, `PATH`, locale) and reports the isolation level `best effort`. A later OS sandbox (Linux, macOS) plugs into
the same interface as a separate spec. haex-vault downloads each tool itself
from the vendor's release host, checks the SHA-256 held in a tested-version list that ships with haex-vault, unpacks into a
staging folder and publishes it with one rename. The user installs, updates and removes tools in Settings. The host
`PATH` lookup and the binary name in the provider row go away.

Technical approach (details in [research.md](research.md)): a new `cli_tools` module (catalog, platform key, install,
unpack, installed-state, runtime, record, commands); `VaultPaths` learns `cli_tools` and `home`; the four spawn sites
and the provider code take a `DelegateRuntime` instead of a binary name; per-run folders move from the OS temp folder
into `<vault>/home/run/`; a maintainer script creates catalog entries from signature-verified vendor data.

## Technical Context

**Language/Version**: Rust 1.77.2 edition 2021 (`src-tauri`), TypeScript 5 / Vue 3 / Nuxt (frontend), TypeScript scripts
under `scripts/`.

**Primary Dependencies**: already declared and reused: `reqwest 0.13` (rustls, stream), `sha2 0.11`, `tokio 1`,
`tokio-util 0.7` (`CancellationToken`), `tempfile 3`, `semver 1`, `sysinfo 0.39`, `ts-rs 12`, `wiremock 0.6` (dev).
New direct dependencies: `tar 0.4` and `flate2 1` (pure-Rust backend); both are already in `Cargo.lock` as transitive
dependencies at `tar 0.4.46` and `flate2 1.1.10`, so the dependency tree does not grow. No PGP library: signatures are
checked by the maintainer script, not the app.

**Storage**: files only. `<vault>/cli-tools/<vendor>/<version>/` (program plus `install.json`), `<vault>/home/` (tool home,
`run/<id>/` per run, `records/tools.log`). No database table and no change to `providers` (a synced table).

**Testing**: `cargo test` with sibling `*_tests.rs` files; `wiremock` fake release host; integration tests with the
existing unix stub delegates in `src-tauri/tests/`; Node checks for the composable; the e2e suite with a stand-in release
host. Real-tool checks stay manual ([quickstart.md](quickstart.md)).

**Target Platform**: desktop (Linux, macOS, Windows). Android and iOS: unavailable, as today (`cli_delegates` capability).
CI is Linux only.

**Project Type**: desktop app (Tauri backend plus Nuxt frontend).

**Performance Goals**: install of a Claude Code program (294 MB) under 5 minutes at 50 Mbit/s (spec SC-001), progress
updates at most every 100 ms (existing downloader), no hashing of the installed program at each start (hash is checked once at
install).

**Constraints**: nothing outside the vault folder is written except the residuals listed in the spec's Assumptions
(transient sockets); no reliance on the host leaving any variable unused: the child environment is built from the host
environment with a small, listed set of changes; no host `PATH` lookup; no logging of vault names, paths or request text through `log::`; each source file under about
500 lines; run cargo through the nix host bridge and with the memory-limited flags noted for this machine.

**Scale/Scope**: 2 vendors now, a third (spec 096) by data plus an adapter; about 0.3 GB (Claude) and 0.5 GB (Codex) per
installed version per vault.

## Constitution Check

Spec Kit constitution (`.specify/memory/constitution.md` 1.4.0) and the spaex constitution (`.spaex/constitution.md`).

| Principle / rule                                                                               | Result         | Note                                                                                                         |
| ---------------------------------------------------------------------------------------------- | -------------- | ------------------------------------------------------------------------------------------------------------ |
| I. No secrets in git                                                                           | PASS           | The catalog holds public SHA-256 values and URLs. Credentials stay in the vault database.                    |
| II. No local absolute paths in versioned config                                                | PASS           | Paths in this plan are `<vault folder>`-relative; the catalog has URLs only.                                 |
| III. Identity device-independent                                                               | PASS           | Install state is per device by design (spec FR-018) and never enters synced tables or identity.              |
| IV. Cross-repo references pin immutable revisions                                              | PASS           | The catalog pins vendor release versions and checksums; no harness content is consumed.                      |
| V, VI, VII, VIII                                                                               | PASS           | No allowlist, instruction, relay or concealment change. VII: a missing release host never blocks local work. |
| spaex: smallest correct implementation, no unneeded dependency                                 | PASS with note | `tar` and `flate2` are required for the Codex archive and already in the lock file.                          |
| spaex: blocking work off the async executor                                                    | PASS           | hashing and unpacking run in `spawn_blocking`.                                                               |
| spaex: tests as `*_tests.rs` beside the module, never inline                                   | PASS           | planned per module.                                                                                          |
| spaex: UI feedback (errors as toast, settings apply on selection, no inline success text)      | PASS           | see research §9.                                                                                             |
| spaex: run formatter, `cargo check`, `clippy`, `cargo test`, prettier on touched Markdown/JSON | PASS           | tasks include the commands and the Android `clippy` for cfg branches.                                        |
| spaex: no LLM-agent references in project artifacts                                            | PASS           | The vendor names the tools by their product names only.                                                      |

Re-check after design: unchanged. No violation needs the Complexity Tracking table.

## Decisions (all resolved, none open before `tasks.md`)

- **D1 – Residual host traces: RESOLVED 2026-11-16.** Measured: Claude Code creates `/tmp/cc-socks-<uid>/<pid>.sock` and
  no variable moves it; an approval socket for a long vault name cannot live in the vault folder (104-byte limit). Decision
  (operator): two isolation layers. This spec ships the base layer behind `Containment` and lists these two as residuals
  (spec Assumptions) with the isolation level shown as `best effort`; the OS sandbox (Linux namespaces set up by haex-vault
  itself, macOS Seatbelt) is a separate spec that makes them literal zero. Docker and Podman were rejected (host
  dependency, traces outside the vault folder, the helper and the socket cross the VM boundary on macOS and Windows).
  Windows stays `best effort`. A bundled `bwrap` or a host `bwrap` is not assumed: on Ubuntu 24.04 unprivileged
  user namespaces are restricted by default (not verifiable here), so the later spec needs a fallback.
- **D2 – Disk cost and versions: RESOLVED 2026-11-16.** Measured 294 MB (Claude) and 475 MB unpacked (Codex) per version
  (the spec assumed about 80 MB). Decision (operator): one version in use, always the newest; the previous version is
  kept as fallback until confirmation. Another update is refused while the current version is unconfirmed. Made concrete
  in spec FR-022 and research §6: a start check before an update becomes usable, confirmation by the first normally ended
  run, a manual "go back" while unconfirmed, and deletion only after no active run uses the older version. Pending
  deletion is retried after those runs end and at the next vault open; another update waits until the fallback is gone,
  keeping at most two version folders. There is no automatic switch to an older version at run time.
- **D3 – Start of a downloaded program: RESOLVED 2026-11-16 (operator approved the run).** Both downloaded programs start
  from an arbitrary folder in an empty environment; Claude's `DISABLE_UPDATES` is honoured from the process environment
  (`claude doctor`); Codex accepts `-c check_for_update_on_startup=false`. New finding: Codex clones a 25 MB plugin
  catalog with `git` at every start, avoided by four `--disable` feature flags (research §2). Still open for the
  quickstart: a real turn with a tool call (shell, patch) needs `rg`/`bwrap` or not, and the flags' effect on a real turn.

## Project Structure

### Documentation (this feature)

```text
specs/094-vault-cli-tools/
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
├── cli_tools/                         # new
│   ├── mod.rs                         # module docs, public surface
│   ├── catalog.rs / catalog.json      # vendor definitions + tested-version list (data, include_str!)
│   ├── platform.rs                    # (os, arch) -> catalog platform key
│   ├── installed.rs                   # read install.json, fit/complete checks, confirm, revert, prune, staging sweep
│   ├── install.rs                     # lock, space check, download, verify, unpack, start check, publish
│   ├── unpack.rs                      # raw file and tar.gz placement, escape refusal
│   ├── containment.rs                 # trait Containment + EnvContainment (base layer), IsolationLevel
│   ├── runtime.rs                     # DelegateRuntime: executable, run folders, the Containment to use
│   ├── record.rs                      # capped per-vault tools.log
│   ├── commands.rs                    # status, install(+update), cancel, revert, remove; events
│   └── *_tests.rs
├── instances/vault_paths.rs           # + cli_tools(name), home(name), run/records helpers
├── instances/open.rs / import.rs      # cli_tools::open_sweep: sweep *.staging and home/run, retry deferred deletions
├── adapters/cli_delegate/             # claude.rs, codex.rs, connect_*.rs, mod.rs, process.rs: DelegateRuntime
├── providers/{mod,add,connect}.rs     # base_url no longer read/required/written for delegates
├── mcp_server/{delegates,helper,endpoint}.rs   # explicit endpoint dir for the helper
├── platform/mod.rs                    # unchanged flag `cli_delegates`
└── lib.rs                             # register commands, event names

src-tauri/tests/cli_delegate_*.rs      # DelegateRuntime::for_stub; containment + log-search tests
src-tauri/Cargo.toml                   # + tar, flate2

scripts/
├── ci/check-vault-paths.py (+ test)   # names `cli-tools`, `home`
└── cli-tools/add-version.ts, check-containment.*   # maintainer tooling for catalog entries

src/
├── components/settings/ConnectDelegateProvider.vue   # tool row per vendor
├── components/settings/CliToolRow.vue                # new
├── composables/useCliTools.ts                        # new: status, actions, progress listener
├── composables/useModelInventory.ts                  # "not installed in this vault" entry
├── composables/useErrorString.ts                     # mapping for the new reasons
├── i18n/locales/{en,de}.json
└── types/bindings/                                   # ts-rs generated (CliToolStatus, CliToolProgress)
```

**Structure Decision**: one backend module `cli_tools` owns installing and resolving; the delegate adapters only receive a
`DelegateRuntime`. Frontend changes stay in the existing providers settings page plus one composable and one row
component. Nothing is added to the settings action catalog (research §9).

## Complexity Tracking

None. The two additions that might look heavy, a catalog maintained by script and two archive crates, follow from the
spec (FR-006, FR-009) and from the vendors' release formats.
