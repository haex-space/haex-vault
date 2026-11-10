# Research: Project-scoped build environment

## Host library discovery

**Decision**: Ask the host pkg-config for its search directories, then derive runtime library paths from the installed host WebKit metadata.

**Evidence**: The current bridge's pkg-config lookup fails for both WebKitGTK and JavaScriptCore. Adding the host-reported metadata search path makes both report version 2.52.6. The host uses a multiarch library directory, which the existing single-directory assumption omits.

**Rationale**: This follows the host's installed layout and supports the existing layout without embedding an architecture or developer-specific path.

**Alternatives considered**: Hardcoding one additional multiarch path would fix only one architecture/layout. Replacing host WebKit with Nix WebKit expands the change and contradicts the existing deliberate host-WebKit choice.

## Existing project prerequisites

**Decision**: Reuse the current Nix SDK/NDK/Java setup and CUDA static-library view.

**Evidence**: The development shell exposes the Android SDK and NDK directories and all four required CUDA static archives. Its CUDA view already has the layout expected by `find_cuda_helper`.

**Rationale**: The personal shell exports and temporary CUDA view duplicate project configuration. The earlier direct non-Nix startup succeeded after removing global Android compiler selection and providing the CUDA library path.

**Alternatives considered**: Keeping global exports preserves hidden dependencies between unrelated projects; copying the temporary local CUDA layout into versioned configuration violates portability requirements.

## Runtime libc and library ordering

**Decision**: Select a libc and matching interpreter together; prefer the newer available libc. Order its directory before the host GTK/WebKit library directories, and preserve cleanup of inherited Nix library paths before host subprocesses start.

**Evidence**:

- This environment has Nix glibc 2.44 and host glibc 2.39.
- The current host interpreter plus the existing runtime directory fails a small native probe with a `GLIBC_PRIVATE` symbol error.
- Adding the host multiarch runtime directory allows a GTK/WebKit probe to render HTML with the host loader.
- A Nix-compiled `cfgetospeed` probe requires `GLIBC_2.42` and fails with the older host loader, proving that host-loader selection alone has an ABI ceiling.
- The Nix loader with its own libc first and the host GTK/WebKit directory after it renders HTML successfully.
- Placing every Nix runtime library directory first instead mixes Pango libraries and fails on `pango_font_description_set_features`.

**Rationale**: A small successful rendering probe does not prove all application's native dependencies can use an older libc. The full application build/start remains the acceptance check; search order alone cannot guarantee compatibility of arbitrary host/Nix library combinations.

**Alternatives considered**: Always forcing the host interpreter retains the demonstrated older-libc failure. Always preferring all Nix libraries retains the demonstrated mixed-GTK failure. Exporting the host library directory globally contaminates unrelated Nix tools and host subprocesses.

## Display and pixbuf defaults

**Decision**: Use the host GTK loader configuration and preserve the user's display/backend choice instead of forcing X11.

**Evidence**: A GTK/WebKit HTML-rendering probe with Nix libc and the exact bridge environment crashes under forced X11 in the NVIDIA EGL/Wayland shim. Removing only the forced `GDK_BACKEND` succeeds. It also succeeds without the combined Nix pixbuf cache and forced CSD/GBM settings.

**Rationale**: The historical graphics workarounds were written for a partially Nix-provided GTK stack. They are unnecessary for the selected host GTK stack and can break startup on this host.

**Alternatives considered**: Retaining the forced backend would make the newer-libc path fail despite a successful link. Choosing a different backend globally would impose another host-specific workaround.

## Test boundary

**Decision**: Exercise the real shell bridge as a subprocess with fixture executables, supplemented by real native/runtime and application-start checks.

**Rationale**: Tests of command strings alone cannot catch the search-order and environment-leak bugs. Fixtures cover alternative host layouts and libc ordering without depending on the developer's machine; actual startup checks cover the ABI interactions fixtures cannot model.

Graphify consultations were inconclusive; direct inspection found no existing bridge test. Existing production functions are modified in place.
