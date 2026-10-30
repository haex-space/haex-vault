/// Runs Tauri's compile-time configuration and code generation.
fn main() {
    let windows = tauri_build::WindowsAttributes::new_without_app_manifest();
    if let Err(error) =
        tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))
    {
        panic!("{error:#}");
    }
    embed_windows_manifest();
    configure_nix_devshell_linker();
}

/// Embeds the application manifest (Common Controls v6) into every linked target on Windows,
/// test binaries included. Tauri's own manifest reaches only the app binary
/// (`cargo:rustc-link-arg-bins`), so a test binary loaded the system's Common Controls v5, which
/// lacks `TaskDialogIndirect`, and stopped with `STATUS_ENTRYPOINT_NOT_FOUND` before any test ran.
fn embed_windows_manifest() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if target_os != "windows" || target_env != "msvc" {
        return;
    }
    let manifest = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("windows-app-manifest.xml");
    println!("cargo:rerun-if-changed={}", manifest.display());
    println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
    println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
}

/// Two Linux-only linker overrides needed only when the toolchain building
/// this crate is itself Nix-provided (the `com.github.haexmas.atoms.holzi`
/// devShell) — never on a native host toolchain (CI: ubuntu-24.04 via
/// `apt-get`/rustup, no Nix involved at all), where they'd actively break
/// things (wrong FHS `/lib64` path, no `lld` installed).
///
/// - `--dynamic-linker`/`-rpath`: this binary always links the *host's*
///   webkit2gtk (WebKit's own helper-process binaries hardcode their nix
///   store path internally, so a Nix-rebuilt WebKit would need a full
///   from-source rebuild just to relink two `patchelf` calls — not worth
///   it). A Nix-toolchain-built binary defaults to Nix's own glibc
///   interpreter, which then loads a second, ABI-clashing glibc alongside
///   the host's WebKit/driver libraries (`undefined symbol:
///   __pointer_chk_guard, version GLIBC_PRIVATE`). Pointing the binary's
///   own interpreter at the host's glibc (verified newer than Nix's here,
///   so backward-compatible) makes the whole process consistently
///   host-glibc-linked instead.
/// - `-fuse-ld=lld`, llm-cuda only: cudarc's extra crates reshuffle Cargo's
///   native-lib link-arg order enough to break GNU ld.bfd's order-sensitive
///   symbol resolution against libgtk-3/libwebkit2gtk (verified live: fails
///   with ld.bfd, succeeds with lld, on an otherwise-identical build).
fn configure_nix_devshell_linker() {
    let is_linux = std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux");
    if !is_linux || !is_nix_toolchain() {
        return;
    }
    // The interpreter path below is x86-64-specific. flake.nix's
    // `eachDefaultSystem` also exposes aarch64-linux, where it would embed
    // the wrong ELF interpreter into an AArch64 binary; fail loudly instead
    // of silently shipping a binary that can't start. Cargo's own target
    // arch, not the build host's -- moot for now since cross-compiling this
    // crate isn't set up, but correct if that ever changes.
    let target_arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    assert_eq!(
        target_arch, "x86_64",
        "configure_nix_devshell_linker: no known host dynamic-linker path for \
         CARGO_CFG_TARGET_ARCH={target_arch:?} (only x86_64 is supported)"
    );
    // FHS `/lib64` path (Arch/CachyOS/Fedora-style hosts, per Etappe-0
    // finding #3's target machine); Debian/Ubuntu-style hosts use
    // `/lib/x86_64-linux-gnu/ld-linux-x86-64.so.2` instead — moot here since
    // this whole function is gated on a Nix-provided rustc, which Debian/
    // Ubuntu CI never uses.
    println!("cargo:rustc-link-arg=-Wl,--dynamic-linker=/lib64/ld-linux-x86-64.so.2");
    // `--disable-new-dtags` forces legacy DT_RPATH instead of DT_RUNPATH:
    // DT_RUNPATH only applies to resolving *this* binary's own direct
    // NEEDED entries, but DT_RPATH set on the main executable is also
    // consulted for libraries *transitively* dlopen'd by other objects
    // (e.g. Nix's Mesa loading the host's Gallium driver by bare filename
    // for GBM/EGL support) — deliberately process-wide, unlike
    // LD_LIBRARY_PATH, which would leak into every unrelated child process
    // this devShell spawns (even plain `mkdir` crashes: it's Nix-provided
    // too, and now hits the same glibc clash pulling in a second libc.so.6
    // from the same directory).
    println!("cargo:rustc-link-arg=-Wl,--disable-new-dtags");
    println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib");

    if std::env::var_os("CARGO_FEATURE_LLM_CUDA").is_some() {
        println!("cargo:rustc-link-arg=-fuse-ld=lld");
    }

    // Tells `lib.rs` this binary needs the `LD_LIBRARY_PATH` the Nix devShell
    // exports (flake.nix's shellHook) to resolve the Nix-provided GTK stack
    // this interpreter override doesn't otherwise cover — and, precisely
    // because it does, must scrub that variable from its own environment
    // before Tauri/GTK/WebKit spawn any child process (see the call site).
    println!("cargo:rustc-env=HOLZI_NIX_DEVSHELL_LINKER=1");
}

/// `RUSTC` is set by Cargo for build scripts, but isn't guaranteed to be an
/// absolute path (observed here as the bare string `"rustc"`, resolved via
/// `PATH` rather than passed through already-resolved) — so resolve it
/// ourselves and check the result, rather than the raw env var.
fn is_nix_toolchain() -> bool {
    let Ok(rustc) = std::env::var("RUSTC") else {
        return false;
    };
    let rustc_path = std::path::Path::new(&rustc);
    if rustc_path.is_absolute() {
        return rustc_path.starts_with("/nix/store/");
    }
    let Ok(path_var) = std::env::var("PATH") else {
        return false;
    };
    std::env::split_paths(&path_var)
        .map(|dir| dir.join(&rustc))
        .find(|candidate| candidate.is_file())
        .is_some_and(|resolved| resolved.starts_with("/nix/store/"))
}
