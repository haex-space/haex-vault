# Implementation Plan: Project-scoped build environment

**Branch**: `077-project-build-environment` | **Date**: 2026-11-02 | **Spec**: [spec.md](spec.md)

## Summary

Make the existing Nix development environment sufficient for CUDA desktop startup, then remove the personal Android/CUDA build exports and the temporary CUDA layout. Extend the existing host bridge and linker configuration in place. Discover host metadata and runtime library directories instead of assuming a single Linux layout. Keep each selected libc with its matching loader, and keep the host GTK/WebKit libraries together. Validate the actual application startup before personal cleanup.

## Technical Context

**Language/Version**: Bash, the repository's pinned Rust toolchain, Node 22 TypeScript tests.

**Primary Dependencies**: Existing Nix devShell, host pkg-config/WebKitGTK, GCC, Tauri, llama.cpp CUDA backend; no new dependency.

**Storage**: Existing environment variables and build outputs; no application data changes.

**Testing**: Node subprocess tests of the real bridge with isolated tool fixtures; native GTK/WebKit probes; Cargo checks and CUDA development startup.

**Target Platform**: Existing x86-64 Linux Nix development path, including Arch-style and Debian/Ubuntu multiarch hosts. Existing non-Linux and non-Nix behavior is preserved.

**Project Type**: Desktop application build tooling.

**Performance Goals**: No persistent helper or additional application startup process; discovery runs only at the build-command boundary.

**Constraints**: No global host library path exported to unrelated Nix tools or host child processes; no machine-specific paths in versioned changes; host prerequisites remain required.

**Scale/Scope**: Existing host bridge, existing Rust linker function and startup comment, focused tests, startup documentation, and local shell cleanup after verification. The Vue rendering error is excluded.

## Constitution Check

- Spec approval received before planning; plan approval received before tasks and implementation.
- All repository changes are authored in the dedicated registered worktree; landing requires a topic-branch pull request.
- No secrets, personal shell contents, machine-local paths, or mutable external references enter the repository.
- No new dependency, public API, module, or production function is planned: extend the existing bridge and `configure_nix_devshell_linker`.
- Graphify consultations returned unrelated/truncated matches. Focused inspection identified the existing bridge, linker function, and startup cleanup as the relevant artifacts; no matching bridge test exists. No duplicate production implementation is introduced.
- Tests live separately from production code and exercise subprocess behavior, runtime-path ordering, failure propagation, and actual startup.
- Neither constitution nor instruction files are amended; this repair changes no core principle and requires no principle ADR.
- Post-design check: requirements remain within the approved scope; no unresolved gate violation.

## Design

1. In `scripts/with-nix-host-bridge.sh`, retain the Linux/Nix guard. Obtain the host utility search path through `getconf PATH`; query host pkg-config with inherited Nix library and pkg-config overrides cleared for that query only. Prepend its reported metadata search directories, with the existing conventional fallback when host discovery is unavailable.
2. Resolve the installed host WebKit/GTK library directory through metadata and fail clearly if a required library is missing. Export build-only runtime settings for the existing Rust build script.
3. Compare host and Nix glibc versions. Keep the newer libc together with its corresponding interpreter: retain the existing host interpreter when the host is at least as new; otherwise use the Nix compiler's reported libc and interpreter. Put the chosen libc directory before host GTK/WebKit directories in the application's runtime search path. Do not put the entire partial Nix GTK stack before the host stack.
4. Use the host pixbuf loader configuration for the selected host GTK stack. Remove the bridge's forced X11/CSD workaround; preserve the user's display/backend choice. Keep driver discovery consistent with the discovered host library layout.
5. In `src-tauri/build.rs`, consume the bridge's explicit interpreter/runtime-directory settings in the existing linker function and track those environment inputs for rebuilds. Preserve target gating and CUDA linker behavior. Keep `clear_inherited_nix_library_path` before application initialization so host WebKit children do not inherit the Nix library path; update its explanation to cover either selected loader.
6. Add isolated subprocess regression tests under `scripts/`, a package check command, and its CI invocation. Cover both host layouts, older/equal/newer host libc selection, bypass behavior, missing metadata, path handling, environment preservation, and the absence of a forced display backend.
7. Document `nix develop --command pnpm tauri:dev:cuda` and the equivalent interactive project-shell workflow. After successful verification, remove only the identified personal Android/CUDA build block and the temporary CUDA symlinks. Clear stale inherited exports in the verification process and explain the same step for existing terminals.

The build-only bridge values are internal wiring, not an additional user configuration interface. Existing Nix SDK, Java, and CUDA setup remains authoritative and is not duplicated in the personal shell.

## Validation

- First demonstrate the bridge regression tests failing against the current implementation; then make them pass.
- Run Bash syntax validation, the repository formatter/static checks for changed files, script type checking, and the new test command.
- Run the repository Rust formatter, `cargo check`, `cargo clippy`, and `cargo test` through the repaired Nix host bridge, using applicable features and reporting any unrelated failures.
- Verify fresh-shell Android SDK/NDK/Java and CUDA static library availability without the personal exports.
- Run the actual CUDA development command to successful compilation and process startup; inspect final runtime search ordering and the resolved loader. Distinguish unrelated frontend errors from build/startup failures.
- Recheck after personal shell cleanup. Stop the test process and leave the startup port free.

## Project Structure

```text
scripts/with-nix-host-bridge.sh
scripts/check-nix-host-bridge.ts
src-tauri/build.rs
src-tauri/src/lib.rs
package.json
.github/workflows/ci.yml
README.md
specs/077-project-build-environment/
  spec.md
  plan.md
  research.md
  data-model.md
  quickstart.md
  checklists/requirements.md
```

No external interface contracts or data migrations are needed for this internal build repair.
