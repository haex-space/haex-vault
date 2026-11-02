# Validation: Project-scoped build environment

## Prerequisites

Use an x86-64 Linux host with Nix, the documented host WebKitGTK libraries, and a working NVIDIA driver. Run commands from the feature worktree.

## Automated checks

```bash
nix develop --command pnpm install --frozen-lockfile
nix develop --command pnpm check:nix-host-bridge
nix develop --command pnpm typecheck:scripts
nix develop --command scripts/with-nix-host-bridge.sh pkg-config --modversion webkit2gtk-4.1 javascriptcoregtk-4.1
```

The bridge tests cover both host layouts, libc selection, bypass behavior, and missing prerequisites. The metadata command must print installed versions for both libraries.

## CUDA startup

Clear stale inherited build exports in the test terminal, then run:

```bash
unset CC CXX AS AR LD RANLIB STRIP CUDA_LIBRARY_PATH CUDA_PATH CUDA_ROOT
nix develop --command pnpm tauri:dev:cuda
```

Expected: compilation succeeds and the application process starts. Confirm the selected interpreter and runtime library directories in the produced executable. Record any frontend runtime error separately from build-environment failures. Stop the development process after verification.

The equivalent interactive workflow is:

```bash
nix develop
pnpm tauri:dev:cuda
```

## Personal shell cleanup

After the project-scoped startup succeeds, remove only the Android/CUDA build exports identified in this session and the temporary personal CUDA symlinks. Preserve unrelated shell configuration. In a clean terminal, verify that those global settings are absent and repeat the project-scoped CUDA startup.

Verify that the project environment still supplies Android SDK, NDK, Java, and all required CUDA static archives. An Android application rebuild is a separate compatibility check if changes affect target selection; this repair does not replace its existing project setup.

## Verification record

- The initial seven subprocess cases produced five expected failures against the old bridge and two successful bypass cases. All eleven final cases pass after the fix, including numeric version ordering, missing loaders, empty library directories and an empty inherited metadata path.
- Bash syntax, changed-script ESLint, script TypeScript checks, Rust formatting and repository-wide Prettier checks pass.
- Host metadata resolves WebKitGTK and JavaScriptCoreGTK 2.52.6. On the verification host, Nix libc 2.44 is selected ahead of host libc 2.39, with the matching Nix interpreter and the host multiarch GTK/WebKit directory.
- With all personal Android/CUDA variables and Android PATH entries removed, the devShell supplies native GCC/G++, Android SDK/NDK, Java and the four required CUDA static archives.
- Reusing an old native build initially exposed llama.cpp's cached host CMake executable. Removing that crate's stale CMake build output allows configuration with the Nix tools; README documents the one-time crate cleanup when changing toolchains.

- The clean CUDA development build finished successfully in 10m 58s and launched the application with both WebKit child processes. ELF inspection confirms the selected Nix interpreter and RPATH beginning with its libc followed by the host multiarch directory.
- Frontend validation separately reports a Vue/Vite import failure for `@/components/shadcn/button/Button.vue` from the remote UI layer. This is outside this build-environment repair; successful process startup does not establish a working frontend.
- After successful startup, the identified personal build-export block, Android PATH additions and the two temporary CUDA symlinks were removed. Unrelated shell configuration was preserved.

- A fresh interactive Bash reports none of the removed personal build exports. The actual CUDA development command was repeated after cleanup and again launched the application and both WebKit processes. Runtime mappings confirm Nix libc with host GTK and WebKit.
- `cargo check --manifest-path src-tauri/Cargo.toml` passed through the repaired bridge.

- Both repository Clippy variants passed with `--all-targets -- -D warnings`: default features and `--no-default-features`.
- The development server was stopped after the repeated startup verification; port 3030 was free.

## Full-suite resource limit

The local full `cargo test` build was stopped before completion: this package has 92 integration-test executables, and the full-debug build had produced 26 executables totaling about 57 GiB, with only 86 GiB left on the filesystem. No full local integration-suite pass is claimed. The executables identified by this change's runtime paths were removed after stopping that build; application binaries and dependencies were retained.

The already-built Nix unit-test executable passed separately with the sherpa library directory that Cargo normally supplies at runtime: 4,928 passed, 0 failed, 14 ignored, in 719 seconds. Its generated binding files differed only in trailing whitespace and were restored after the run. The complete test step passed in both the default and no-default-feature PR CI jobs. For a future local full-suite run, use the same reduced debug information as CI and one build job:

```bash
CARGO_PROFILE_DEV_DEBUG=line-tables-only nix develop --command \
  scripts/with-nix-host-bridge.sh cargo test \
  --manifest-path src-tauri/Cargo.toml --jobs 1
```

The initial PR CI passed documentation/script checks, the frontend build, the Android build, Android Clippy with default features, Rust checks without default features and the separate mistralrs check. The default Rust job completed its test step successfully and proceeded to Clippy. Android Clippy without default features failed during SDK installation because the NDK download was not a valid ZIP archive, before project compilation.
