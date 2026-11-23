# Fixtures of the maintainer script

`abacusai-2.10.23-linux-x64.sigstore` and `abacusai-2.10.23-linux-x64.sha256` come from the Abacus AI CLI release 2.10.23
for `linux-x64` (spec 096, research §1):

- The `.sigstore` file is the 10 411-byte Sigstore bundle (`application/vnd.dev.sigstore.bundle.v0.3+json`) that sits next
  to the program inside `https://static.abacus.ai/cli/releases/2.10.23/abacusai-linux-x64.tar.gz` (as
  `abacusai-linux-x64.sigstore`). It is a message signature over the program's SHA-256 with a Rekor entry.
- The `.sha256` file is the SHA-256 of that archive's program `abacusai` (157 563 056 bytes), measured on 2026-11-22.

The bundle verifies against the bare digest, so the 157 MB program is not committed:

```sh
sigstore verify identity --bundle abacusai-2.10.23-linux-x64.sigstore \
  --cert-identity https://github.com/abacusai/codellm/.github/workflows/release-cli-native.yml@refs/tags/cli-v2.10.23 \
  --cert-oidc-issuer https://token.actions.githubusercontent.com \
  sha256:$(cat abacusai-2.10.23-linux-x64.sha256)
```

`pnpm check:cli-tools` runs this when `sigstore` is on `PATH` (or named by `HAEX_SIGSTORE_BIN`) and checks that a changed
digest fails.
