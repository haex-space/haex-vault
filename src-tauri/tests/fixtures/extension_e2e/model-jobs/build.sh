#!/usr/bin/env bash
# Bundles main.js with the released vault-sdk into app.js, the code of the test extension of spec
# 055 (T065). Run it after changing main.js or a pinned version, then rebuild the bundle
# with `HOLZI_WRITE_E2E_FIXTURES=1 cargo test --test extension_e2e_fixtures`. See SOURCE.md.
set -euo pipefail

SDK_VERSION=4.10.0
SDK_INTEGRITY=sha512-R6HcP2A92zg+0M5ZqR1F+WOqQNC7wvX7i0kbay4TWtdJ0PcxzIFRsYUA+d+uIn+1/iT3Xre/AVAtiAjgwyKgEA==
DRIZZLE_VERSION=0.45.4
ESBUILD_VERSION=0.28.2

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../../../.." && pwd)
esbuild="$root/node_modules/.pnpm/esbuild@$ESBUILD_VERSION/node_modules/esbuild/bin/esbuild"
[ -x "$esbuild" ] || { echo "esbuild $ESBUILD_VERSION not found; run pnpm install" >&2; exit 1; }

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cd "$work"
echo '{"name":"model-jobs-build","private":true}' > package.json
# Peers (vue, react, …) belong to the SDK's framework entries, not to the one bundled here.
npm install --ignore-scripts --no-audit --no-fund --legacy-peer-deps --save-exact \
  "@haex-space/vault-sdk@$SDK_VERSION" "drizzle-orm@$DRIZZLE_VERSION" >/dev/null
integrity=$(node -e 'const l=require("./package-lock.json");console.log(l.packages["node_modules/@haex-space/vault-sdk"].integrity)')
[ "$integrity" = "$SDK_INTEGRITY" ] || { echo "vault-sdk integrity mismatch: $integrity" >&2; exit 1; }

# The SDK's entry imports `path` and `url` for its Node helpers only; the frame gets empty modules.
echo 'export default {}' > empty.js
cp "$here/main.js" "$here/manifest.json" "$here/page-format.json" .
"$esbuild" main.js --bundle --format=esm --platform=browser --target=es2022 \
  --alias:path=./empty.js --alias:url=./empty.js \
  --legal-comments=eof --outfile="$here/app.js"
