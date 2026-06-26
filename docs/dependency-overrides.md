# pnpm overrides — rationale

This file documents why `package.json` `pnpm.overrides` pins the versions it pins. Re-evaluate quarterly: bump the floor when the upstream package owners ship the same fix in a non-major release, or remove the override entirely if it is no longer needed.

Most of the floors listed here were added in commit `7c81cc2d` ("fix: patch critical+high security vulnerabilities", 2026-04-12) and are CVE-driven. The remaining two (`vue-router`, `zod`) are alignment pins so the transitive resolution matches what the direct dependencies use.

## Active overrides

### `vue-router: ^5.0.6`

- **Why**: Without the override the resolver was pulling in vue-router v4.x via a transitive dependency, while the project's direct vue-router pin (and Nuxt 4 / Vue 3.5 ecosystem) is on v5. The pin forces a single resolved copy so router-related types and runtime stay consistent. Introduced in commit `6bda8522` (2026-04-28, "Fix vue-router override to v5 + remove redundant devDependency").
- **Lift when**: All transitive consumers of vue-router publish releases with peer ranges that include v5 (or when Nuxt's bundled router upgrades cleanly without the manual pin).

### `zod: ^3.25.76`

- **Why**: Several transitive consumers (Nuxt UI, nuxt-zod-i18n, etc.) bring in older zod 3.x lines; the project uses zod 3.25 features in shared schemas. Pinning the floor avoids two zod copies in the bundle and keeps schema types unified. Originally introduced in commit `0a7de8b7` (2025-09-11, "switch to nuxt ui") and re-synced in commit `76a888f9` (2025-12-08, "Sync zod version in pnpm overrides with dependencies").
- **Lift when**: All zod consumers in the dependency tree publish releases that depend on `^3.25` (or higher) so dedup happens naturally.

### `defu: >=6.1.5`

- **Why**: CVE patch floor. Introduced in commit `7c81cc2d` (2026-04-12, "patch critical+high security vulnerabilities") alongside the other transitive bumps. Older defu versions were flagged in the Nuxt/Nitro dependency chain.
- **Lift when**: All defu consumers in the lock file already depend on `>= 6.1.5`. Verify with `pnpm why defu` and remove when no caller resolves below the floor.

### `picomatch: >=4.0.4`

- **Why**: CVE patch floor (`7c81cc2d`, 2026-04-12). picomatch ships in many tool chains (chokidar, micromatch, vite plugins); older lines had a ReDoS-class advisory.
- **Lift when**: `pnpm why picomatch` shows every consumer already requires `>= 4.0.4`.

### `node-forge: >=1.4.0`

- **Why**: CVE patch floor (`7c81cc2d`, 2026-04-12). node-forge older versions have multiple GHSA entries (signature spoofing / prototype pollution). Forge enters the tree via tooling, not direct use.
- **Lift when**: No transitive consumer below `1.4.0` remains. Alternatively, audit whether node-forge is still reachable at all and remove the pin if it's gone from the lock file.

### `lodash: >=4.18.0`

- **Why**: CVE patch floor (`7c81cc2d`, 2026-04-12). The `>=4.18.0` floor sits above the historical prototype-pollution advisories (GHSA-jf85-cpcp-j695 and friends). Lodash enters transitively — the project itself does not import it directly.
- **Lift when**: lodash drops out of the dependency tree (preferred), or every remaining consumer already pins above 4.17.x.

### `axios: >=1.15.0`

- **Why**: CVE patch floor (`7c81cc2d`, 2026-04-12). Older axios lines carry SSRF / DoS advisories. axios is transitive (likely via tooling or test fixtures); production runtime uses Tauri's HTTP plugin.
- **Lift when**: `pnpm why axios` shows the dep is gone, or all callers require `>= 1.15`.

### `happy-dom: >=20.8.9`

- **Why**: CVE patch floor (`7c81cc2d`, 2026-04-12). happy-dom is a Vitest peer; older versions had prototype-pollution / sandbox-escape issues. The floor matches what the current Vitest line uses.
- **Lift when**: Vitest's bundled happy-dom version exceeds 20.8.9 across all install paths — verify with `pnpm why happy-dom`.

## When adding a new override

1. Add the pin in `package.json` `pnpm.overrides`.
2. Add a `### <package>` section here with the same shape as above. State **why** and **lift when**.
3. If the pin is CVE-driven, link the GHSA / advisory ID in the "Why" line.
4. Re-evaluate at every quarterly audit; remove the pin once it's redundant.
