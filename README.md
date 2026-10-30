<p align="center"><img src="branding/haex-vault-icon.png" alt="" width="160"></p>

# holzi

A portable, isolated personal-agent app. Runs as a Tauri application on any device the operator owns (desktop, server, iOS, Android).

- All LLM credentials and model configuration live inside holzi. Opens on any host, works even if the host has no LLM installed.
- Nostr relay endpoint plus iroh peer in one process, forming a closed federation with the operator's other holzi installations.
- Exposes an MCP server for external clients. Standard MCP auth applies.
- Local-first, user-chosen models. No automatic model routing.
- Mobile participates as a full peer while foreground; unreachable while backgrounded is a normal presence state, not an error.

## Design

The design corpus lives under [docs/](docs/) and the numbered specs under [specs/](specs/). Founding architecture first, then v1 scope revisions on top of it.

- [plans/README.md](plans/README.md) — advisory roadmap for the first desktop MVP; the numbered specs remain normative.
- [docs/design/founding.md](docs/design/founding.md) — founding architecture. Partially superseded by the v1 scope document below (see its front matter for the pointers).
- [docs/plans/2026-09-04-v1-scope-design.md](docs/plans/2026-09-04-v1-scope-design.md) — draws the v1 line and revises identity/pairing, storage, and mobile. Normative for v1 where it differs from founding.
- [docs/plans/2026-09-04-haex-crdt-extraction-plan.md](docs/plans/2026-09-04-haex-crdt-extraction-plan.md) — extraction of the SQLite + CRDT-sync layer into the standalone `haex-crdt` crate (shipped as v0.1.0; holzi consumes it as a Rust dependency).
- [docs/plans/2026-09-07-cross-user-sharing-deferred-design.md](docs/plans/2026-09-07-cross-user-sharing-deferred-design.md) — deferred design for cross-user shared spaces. Not v1; captured so closed-federation work does not foreclose it.
- [specs/001-frontend-onboarding/](specs/001-frontend-onboarding/) — first numbered spec (Landing / Anlegen / Öffnen / Verbinden / Unlock).

## Status

Etappe 1 (App und Instanzlebenszyklus) merged 2026-09-09. Etappe 2 (Anbieter und Modellkatalog) und Etappe 3 (Nutzbarer Chat) in Arbeit. Identity model settled on per-instance keys stored inside the encrypted SQLite (no federation-root, no paper-seed). External `haex-crdt` crate provides the SQLite + CRDT-sync foundation.

## Building holzi

holzi is a Tauri 2 app: a Nuxt frontend and a Rust backend. CI builds and
tests it on Ubuntu 24.04 (`.github/workflows/ci.yml`). The other systems
below follow the upstream requirements of Tauri and of holzi's native
dependencies but are not built in CI.

### Requirements on every system

- **Rust** 1.95 or newer through [rustup](https://rustup.rs) (`rust-version`
  in `src-tauri/Cargo.toml`; CI uses the current stable).
- **Node.js** `^22.19.0`, `^24.11.0` or `>=26` and **pnpm** 9.15 through
  Corepack: `corepack enable`.
- Native build tools for these crates, all compiled from source during the
  Rust build:
  - SQLCipher with a vendored OpenSSL (`rusqlite` from `haex-crdt`) needs a C
    compiler, **Perl**, **make** and **libclang** (its bindings are generated
    with bindgen).
  - `aws-lc-sys` (rustls) and the tokenizers of the local models
    (`onig_sys`, `esaxx-rs`) need a C/C++ compiler.

### Linux

**Debian / Ubuntu** (what CI installs):

```bash
sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file \
  libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev \
  libasound2-dev libclang-dev
```

**Arch**:

```bash
sudo pacman -S --needed webkit2gtk-4.1 base-devel curl wget file openssl \
  appmenu-gtk-module libappindicator-gtk3 librsvg xdotool alsa-lib clang perl
```

**Fedora**:

```bash
sudo dnf install webkit2gtk4.1-devel openssl-devel curl wget file \
  libappindicator-gtk3-devel librsvg2-devel libxdo-devel alsa-lib-devel \
  clang-devel perl-FindBin perl-IPC-Cmd
sudo dnf group install "c-development"
```

`libasound`/`alsa-lib` is for microphone input (the default `voice` feature).

Build with the host's toolchain, outside any Nix shell. Alternatively, the
repository's Nix devShell (`nix develop`, or `direnv` via `.envrc`) provides
the toolchain. WebKitGTK still comes from the host, and
`scripts/with-nix-host-bridge.sh` points the build at it. The script assumes
Arch paths (`/usr/lib/pkgconfig`, `/usr/lib/gbm`), so on other distributions
build without the devShell.

### macOS

- Xcode, or the Xcode Command Line Tools: `xcode-select --install`. They
  bring the C compiler, make and Perl.
- If bindgen cannot find libclang: `brew install llvm`, then set
  `LIBCLANG_PATH` to its `lib` directory (`$(brew --prefix llvm)/lib`).

### Windows

- [Microsoft C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)
  with the "Desktop development with C++" workload, and the MSVC Rust
  toolchain (rustup's default on Windows).
- WebView2 Runtime: part of Windows 10 (1803+) and 11. Otherwise install the
  Evergreen Bootstrapper.
- LLVM for libclang: `winget install LLVM.LLVM`, then set `LIBCLANG_PATH` to
  its `bin` directory (for example `C:\Program Files\LLVM\bin`).
- [Strawberry Perl](https://strawberryperl.com/) for the OpenSSL build.
  [NASM](https://www.nasm.us/) is optional; without it OpenSSL builds without
  assembly routines.

### Build and run

```bash
pnpm install
pnpm tauri:dev      # development build with hot reload
pnpm tauri:build    # release bundle
```

On Windows run `pnpm tauri dev` and `pnpm tauri build` instead. The
`tauri:*` scripts go through a Bash wrapper (`scripts/with-nix-host-bridge.sh`)
that only matters inside the Linux Nix devShell.

The first Rust build compiles the local-inference stack (mistralrs, candle)
and takes several minutes. GPU builds: `pnpm tauri:dev:cuda` (NVIDIA, CUDA
toolkit ≥ 12 with `nvcc`) and `pnpm tauri:dev:metal` (Apple). See
[Local inference build](#local-inference-build).

### End-to-end tests

The end-to-end suite runs on Linux only. It drives the app through
`tauri-driver` and WebKit's WebDriver on a virtual screen. Other platforms
are planned in [scripts/e2e/PLATFORMS.md](scripts/e2e/PLATFORMS.md).

```bash
sudo apt install webkit2gtk-driver xvfb    # Debian / Ubuntu
cargo install tauri-driver --locked --version 2.0.6
pnpm test:e2e
```

`WebKitWebDriver` must be the same version as the host's WebKitGTK. The
suite checks this before it starts.

The same suite also runs against the Android app in an emulator; see
[scripts/e2e/PLATFORMS.md](scripts/e2e/PLATFORMS.md).

### Android

holzi runs as an Android app from Android 8 (API 26) on
([spec 043](specs/043-android-build/spec.md)). The Nix devShell brings the
Android SDK, the NDK, Java 17 and a Rust toolchain with the Android targets;
it sets `ANDROID_HOME`, `NDK_HOME`, `ANDROID_NDK_ROOT` and `JAVA_HOME`.

```bash
nix develop --command pnpm tauri android build --debug --apk --split-per-abi --target aarch64 --target x86_64
adb install -r src-tauri/gen/android/app/build/outputs/apk/arm64/debug/app-arm64-debug.apk     # phone
adb install -r src-tauri/gen/android/app/build/outputs/apk/x86_64/debug/app-x86_64-debug.apk   # emulator
```

Without building: every pull request's CI run has the debug APKs as the
artifact `holzi-android-debug`. They are signed with the CI's debug key, so
uninstall a locally built holzi before installing one of them (and the other
way round); uninstalling deletes the vaults on the phone.

Development mode with hot reload: `nix develop --command pnpm tauri android dev`
for an emulator, add `--host` for a phone in the same network.

#### Release key

Release APKs are signed with holzi's own key. Every update must be signed
with the same key: if it is lost, installed apps can no longer be updated,
only uninstalled (which deletes their vaults). The key is never in the
repository. The person who maintains holzi creates it once and keeps the
keystore and its password safe outside GitHub:

```bash
keytool -genkeypair -keystore holzi-release.keystore -storetype PKCS12 \
  -alias holzi -keyalg RSA -keysize 4096 -validity 10000 -dname "CN=holzi"
keytool -list -v -keystore holzi-release.keystore -alias holzi   # SHA256 fingerprint
```

A PKCS12 keystore has one password for the store and the key. The workflow
`.github/workflows/android-release.yml` reads it from these repository
settings (Settings → Secrets and variables → Actions):

| Name                   | Kind     | Content                                                                |
| ---------------------- | -------- | ---------------------------------------------------------------------- |
| `ANDROID_KEY_BASE64`   | secret   | the keystore, `base64 -w0 holzi-release.keystore`                      |
| `ANDROID_KEY_ALIAS`    | secret   | the key alias (`holzi` above)                                          |
| `ANDROID_KEY_PASSWORD` | secret   | the keystore password                                                  |
| `ANDROID_CERT_SHA256`  | variable | the certificate's SHA-256 fingerprint (public), as `keytool` prints it |

A tag `v<version>` matching `version` in `src-tauri/tauri.conf.json` builds
the signed APKs, checks their certificate against `ANDROID_CERT_SHA256` and
attaches them to that tag's GitHub release. Raise the version for every
release: Android installs an APK over the previous one only with a higher
version code, which Tauri derives from the version. Starting the workflow
by hand builds and checks the APKs without a release (artifact
`holzi-android-release`). Without the secrets the workflow fails; it never
publishes an unsigned APK.

To sign a release build locally, put a `keystore.properties` (ignored by
git) next to `src-tauri/gen/android/build.gradle.kts`:

```properties
storeFile=/absolute/path/to/holzi-release.keystore
keyAlias=holzi
password=...
```

### Desktop releases

The same tag `v<version>` also starts `.github/workflows/desktop-release.yml`.
It builds the installers for Linux (`.deb`, `.rpm`, `.AppImage`), Windows
(`.exe` installer and `.msi`) and macOS (`.dmg` for Apple Silicon and for
Intel), each with local inference and voice, and attaches them to the tag's
release together with `SHA256SUMS-desktop.txt`. The tag must match `version`
in `src-tauri/tauri.conf.json` and `src-tauri/Cargo.toml`. Starting the
workflow by hand, or a pull request that changes it, builds the installers
without a release (artifacts `holzi-desktop-<platform>`).

The installers are not signed yet, so Windows SmartScreen and macOS Gatekeeper
warn when they are opened. macOS signing and notarization start on their own
once these repository secrets exist (they need a paid Apple Developer
account):

| Secret                       | Content                                                    |
| ---------------------------- | ---------------------------------------------------------- |
| `APPLE_CERTIFICATE`          | the "Developer ID Application" certificate, `.p12`, base64 |
| `APPLE_CERTIFICATE_PASSWORD` | the password of that `.p12`                                |
| `APPLE_SIGNING_IDENTITY`     | the certificate's name, `Developer ID Application: …`      |
| `APPLE_ID`                   | the Apple ID used for notarization                         |
| `APPLE_PASSWORD`             | an app-specific password of that Apple ID                  |
| `APPLE_TEAM_ID`              | the team ID                                                |

With a signed build, `bundle.macOS.entitlements` also has to grant
`com.apple.security.device.audio-input`, or the hardened runtime blocks the
microphone. Windows signing is not wired up.

## Einen Marketplace betreiben

HaexVault lädt Erweiterungen aus beliebig vielen Marketplaces. Wer einen eigenen betreiben will (für ein
Team, eine Firma oder zum Selbsthosten), implementiert das Protokoll in
[specs/069-marketplace/contracts/marketplace-api.md](specs/069-marketplace/contracts/marketplace-api.md):
`GET /health`, `/extensions`, `/extensions/{slug}`, `/extensions/{slug}/download` und `/categories`, mit
SHA-256 und Größe jeder Version. Es ist die einzige Quelle der Wahrheit; HaexVault nimmt darüber hinaus
nichts an. Hinzugefügt wird er unter Einstellungen → Erweiterungen → Marketplaces. Eine Anmeldung (Bearer
oder Basic) liegt als Eintrag im Passwortmanager und wird nur über `https` gesendet.

## Development setup

The repository uses the Claude speckit integration. After a fresh clone,
restore the ignored local `/speckit.*` entrypoints with:

```bash
specify init --here --ai claude --offline
```

### Local inference build

When using the Linux Nix devShell, run Rust/Tauri commands through
`scripts/with-nix-host-bridge.sh`. WebKitGTK 4.1 is intentionally provided by
the host system because its helper binaries must match the host runtime; the
host development package (for example `libwebkit2gtk-4.1-dev` on Debian/
Ubuntu) must be installed. Outside the Nix devShell, the wrapper is a no-op.

The default `cargo build` enables the `llm-cpu` feature, which pulls
`mistralrs = 0.8.1` and its candle/tokio dependency tree. The first
build downloads and compiles many crates; expect several minutes on a
cold cache. Iterating on non-LLM code can skip that path with
`cargo build --no-default-features` (the `llm` module then compiles as
an empty gate).

For GPU inference, opt into one of the stacked features:

```bash
# NVIDIA GPU — requires nvcc + CUDA toolkit >= 12.0 at build time.
# The driver alone is not enough (Etappe 0 finding #3).
cargo build --features llm-cuda

# Apple GPU.
cargo build --features llm-metal
```

**First-load warning on CUDA hosts.** Under CUDA the very first model
load per host takes 30-45 s while nvcc's JIT cache in
`~/.nv/ComputeCache` warms. Subsequent loads on the same hardware are
~4 s. The chat UI shows a dedicated "GPU wird für dieses Modell
optimiert" state for that first load (Etappe 0 finding #4).

### Running the local-inference integration test

The wrapper's real-runtime test is skipped by default because it needs
a GGUF file on disk. To exercise it, point `HOLZI_TEST_GGUF` at any
GGUF and add `--ignored`:

```bash
HOLZI_TEST_GGUF=~/path/to/model.gguf \
  cargo test --manifest-path src-tauri/Cargo.toml \
  --test local_inference -- --ignored
```

`HOLZI_TEST_GGUF_TOKENIZER` overrides the tokenizer repo id (default:
`Qwen/Qwen3-4B`).

### Local model presets

Holzi uses Qwen3 as its local model family: Qwen3-4B Q4_K_M is the desktop
preset, Qwen3-1.7B Q4_K_M is the smartphone preset, and Qwen3-0.6B Q4_K_M is
the low-memory fallback. See [ADR-0002](docs/adr/0002-local-model-profiles.md)
for the platform boundary and rationale.

### Running the real local CLI tool-loop test

The tool-loop suite contains one hardware/model-dependent acceptance test.
It loads the GGUF, verifies that the model requests `run_command`, approves
only the exact harmless fixture command, executes it through Holzi's real CLI
tool, and verifies that the output is included in the final model response:

```bash
HOLZI_TEST_GGUF=~/path/to/tool-capable-model.gguf \
  cargo test --manifest-path src-tauri/Cargo.toml \
  --test chat_tool_loop_permissions \
  a_real_local_model_can_request_and_process_a_cli_command \
  -- --ignored --nocapture
```

The model must support tool calling. The test is ignored by default so normal
developer test runs stay fast. CI runs the deterministic
`ci_e2e_loaded_model_can_request_and_process_a_cli_command` test in the
dedicated `local-cli-e2e` job; this keeps the CI gate independent of model
sampling and network-hosted weights. For CUDA, add `--features llm-cuda`
before `--test`.
