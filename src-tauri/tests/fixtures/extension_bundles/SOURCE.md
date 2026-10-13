# Source of the bundle test vectors (spec 017, T012; spec 018, T012)

Copied unchanged from [haex-space/vault-sdk](https://github.com/haex-space/vault-sdk) at
`280cebf63eba72422d1ddd62e1edeb5ad7e7d3cc` (release 4.3.0, after haex-space/vault-sdk#64), path
`test-vectors/bundles/`. `README.md` and `expected.json` are part of the copy.

The vectors are generated there by `scripts/generate-bundle-vectors.mjs`; never edit them here. To
update, regenerate in vault-sdk, merge, copy the folder again and change the SHA above.

Every vector is signed with public test keys whose private halves are derived from public seeds (see
`README.md`). They are no secret and must never be trusted outside these tests.
