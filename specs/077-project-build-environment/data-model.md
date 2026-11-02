# Data Model: Project-scoped build environment

No persistent application entities or migrations are introduced.

The build boundary has three transient inputs:

- **Project environment**: pinned tools, Android SDK/NDK/Java, Nix libc, and the existing CUDA static-library view.
- **Host environment**: installed WebKit/GTK metadata, native library directories, libc version, and display preferences.
- **Application runtime selection**: the chosen libc/interpreter pair and ordered runtime library directories, emitted only for the application build.

The ordinary shell owns none of Holzi's build settings. Host subprocesses must not inherit the Nix runtime library path. Personal cleanup follows successful replacement verification.
