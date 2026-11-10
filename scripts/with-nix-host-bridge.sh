#!/usr/bin/env bash
# Build with Nix tools while using the host's WebKit, GTK and GPU drivers.
set -euo pipefail

if [ "$(uname -s)" != "Linux" ] || [ -z "${IN_NIX_SHELL:-}" ]; then
  exec "$@"
fi

# Query the host without injecting Nix libraries into its utilities. Keep
# Nix's metadata as a fallback for dependencies the host does not provide.
host_utility_path="$(getconf PATH)"
host_pc_path="$(env -u LD_LIBRARY_PATH -u PKG_CONFIG_PATH -u PKG_CONFIG_LIBDIR \
  PATH="$host_utility_path" pkg-config --variable=pc_path pkg-config 2>/dev/null || true)"
host_pc_path="${host_pc_path:-/usr/lib/pkgconfig:/usr/share/pkgconfig}"
export PKG_CONFIG_PATH="$host_pc_path${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
if ! pkg-config --exists webkit2gtk-4.1 javascriptcoregtk-4.1 gtk+-3.0 gdk-pixbuf-2.0; then
  echo "Host WebKit/GTK development metadata missing; install the host development packages for webkit2gtk-4.1 and gtk+-3.0." >&2
  exit 1
fi

# A newer Nix libc can supply symbols absent from an older host libc. The
# selected interpreter MUST belong to that same libc, otherwise GLIBC_PRIVATE
# symbols collide. Host GTK/WebKit must precede the partial Nix GTK stack.
nix_glibc="$(getconf GNU_LIBC_VERSION)"
host_glibc="$(env -u LD_LIBRARY_PATH PATH="$host_utility_path" getconf GNU_LIBC_VERSION)"
if [ "$(printf '%s\n' "$nix_glibc" "$host_glibc" | sort -V | tail -n 1)" = "$host_glibc" ]; then
  dynamic_linker="$(readlink -f /lib64/ld-linux-x86-64.so.2)"
  libc_dir="$(dirname "$dynamic_linker")"
else
  dynamic_linker="$(cc -print-file-name=ld-linux-x86-64.so.2)"
  libc_dir="$(dirname "$(cc -print-file-name=libc.so.6)")"
fi
if [ ! -f "$dynamic_linker" ] || [ ! -f "$libc_dir/libc.so.6" ]; then
  echo "Cannot locate a matching Linux x86-64 libc and dynamic linker." >&2
  exit 1
fi
runtime_path="$libc_dir"
for package in webkit2gtk-4.1 javascriptcoregtk-4.1 gtk+-3.0 gdk-pixbuf-2.0; do
  libdir="$(pkg-config --variable=libdir "$package")"
  if [ -z "$libdir" ] || [ ! -d "$libdir" ]; then
    echo "Host $package metadata has no usable library directory." >&2
    exit 1
  fi
  case ":$runtime_path:" in
    *":$libdir:"*) ;;
    *) runtime_path="$runtime_path:$libdir" ;;
  esac
done
# Internal build-script inputs, not global LD_LIBRARY_PATH overrides: Nix
# tools and host WebKit children must retain their own runtime environments.
export HOLZI_NIX_DYNAMIC_LINKER="$dynamic_linker"
export HOLZI_NIX_RUNTIME_LIBRARY_PATH="$runtime_path"

# Use loaders from the same host stack as GTK; do not force X11 or disable
# decorations to work around the old mixed host/Nix pixbuf configuration.
unset GDK_PIXBUF_MODULE_FILE GDK_PIXBUF_MODULEDIR
host_pixbuf_cache="$(pkg-config --variable=gdk_pixbuf_cache_file gdk-pixbuf-2.0)"
if [ -n "$host_pixbuf_cache" ] && [ -f "$host_pixbuf_cache" ]; then
  export GDK_PIXBUF_MODULE_FILE="$host_pixbuf_cache"
fi
export GBM_BACKENDS_PATH="$(pkg-config --variable=libdir gtk+-3.0)/gbm"
export __EGL_VENDOR_LIBRARY_DIRS=/usr/share/glvnd/egl_vendor.d

exec "$@"
