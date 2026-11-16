// Pure helpers of `scripts/cli-tools/add-version.ts` (spec 094): turn a vendor's published
// release data into a catalog entry, and insert it into `catalog.json`. No network, no process
// is started here, so `scripts/check-cli-tools-catalog.ts` can test all of it.

export type PlatformKey =
  | 'linux-x64'
  | 'linux-arm64'
  | 'darwin-x64'
  | 'darwin-arm64'
  | 'win32-x64'
  | 'win32-arm64'

export const PLATFORM_KEYS: readonly PlatformKey[] = [
  'linux-x64',
  'linux-arm64',
  'darwin-x64',
  'darwin-arm64',
  'win32-x64',
  'win32-arm64',
]

export interface Artifact {
  url: string
  sha256: string
  size: number
  kind: 'file' | 'tar.gz'
  unpacked_size?: number
}

export interface CatalogVersion {
  version: string
  platforms: Partial<Record<PlatformKey, Artifact>>
}

export interface VendorDefinition {
  vendor: string
  versions: CatalogVersion[]
  [field: string]: unknown
}

export interface Catalog {
  schema: number
  vendors: VendorDefinition[]
}

const SHA256 = /^[0-9a-f]{64}$/
const SEMVER = /^(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?$/

export const isSha256 = (value: unknown): value is string =>
  typeof value === 'string' && SHA256.test(value)

/** Orders `a` and `b` by semantic version, a pre-release before its release. */
export function compareSemver(a: string, b: string): number {
  const left = SEMVER.exec(a)
  const right = SEMVER.exec(b)
  if (!left || !right) throw new Error(`not a semver version: ${left ? b : a}`)
  for (let i = 1; i <= 3; i++) {
    const diff = Number(left[i]) - Number(right[i])
    if (diff !== 0) return Math.sign(diff)
  }
  if (left[4] === right[4]) return 0
  if (left[4] === undefined) return 1
  if (right[4] === undefined) return -1
  return left[4] < right[4] ? -1 : 1
}

// ---- Claude Code: `manifest.json` of a release -------------------------------------------

export const CLAUDE_DOWNLOAD_BASE =
  'https://downloads.claude.ai/claude-code-releases'

interface ClaudeManifest {
  version?: string
  platforms?: Record<
    string,
    { binary?: string; checksum?: string; size?: number }
  >
}

/**
 * The catalog entry for a Claude Code release, from its (signature-checked) manifest. Each of
 * the six platform keys must be in the manifest; a missing one is an error, not a silent gap.
 */
export function claudeEntry(
  version: string,
  manifest: ClaudeManifest,
  base: string = CLAUDE_DOWNLOAD_BASE,
): CatalogVersion {
  if (manifest.version !== version) {
    throw new Error(
      `the manifest is for ${String(manifest.version)}, not ${version}`,
    )
  }
  const platforms: CatalogVersion['platforms'] = {}
  for (const key of PLATFORM_KEYS) {
    const item = manifest.platforms?.[key]
    if (!item) throw new Error(`the manifest has no ${key}`)
    if (!isSha256(item.checksum)) {
      throw new Error(`${key}: the manifest checksum is not a SHA-256`)
    }
    if (!Number.isSafeInteger(item.size) || (item.size as number) <= 0) {
      throw new Error(`${key}: the manifest size is not a positive integer`)
    }
    if (!item.binary || /[\\/]/.test(item.binary)) {
      throw new Error(`${key}: the manifest binary name is not a file name`)
    }
    platforms[key] = {
      url: `${base}/${version}/${key}/${item.binary}`,
      sha256: item.checksum,
      size: item.size as number,
      kind: 'file',
    }
  }
  return { version, platforms }
}

// ---- Codex: `codex-package_SHA256SUMS` and the GitHub asset digests -------------------------

export const CODEX_DOWNLOAD_BASE =
  'https://github.com/openai/codex/releases/download'

/** The release asset target of each platform key. */
export const CODEX_TARGETS: Record<PlatformKey, string> = {
  'linux-x64': 'x86_64-unknown-linux-musl',
  'linux-arm64': 'aarch64-unknown-linux-musl',
  'darwin-x64': 'x86_64-apple-darwin',
  'darwin-arm64': 'aarch64-apple-darwin',
  'win32-x64': 'x86_64-pc-windows-msvc',
  'win32-arm64': 'aarch64-pc-windows-msvc',
}

export const codexAssetName = (key: PlatformKey): string =>
  `codex-package-${CODEX_TARGETS[key]}.tar.gz`

export const codexAssetUrl = (version: string, key: PlatformKey): string =>
  `${CODEX_DOWNLOAD_BASE}/rust-v${version}/${codexAssetName(key)}`

/** `<hash>  <file name>` lines (`sha256sum` output) as a map from file name to hash. */
export function parseSha256Sums(text: string): Map<string, string> {
  const sums = new Map<string, string>()
  for (const line of text.split('\n')) {
    const match = /^([0-9a-f]{64}) [ *](\S+)$/.exec(line.trim())
    if (match) sums.set(match[2]!, match[1]!)
  }
  return sums
}

/** The asset digests of a GitHub release (`sha256:<hex>`), by asset name. */
export function githubDigests(release: {
  assets?: { name?: string; digest?: string | null }[]
}): Map<string, string> {
  const digests = new Map<string, string>()
  for (const asset of release.assets ?? []) {
    const match = /^sha256:([0-9a-f]{64})$/.exec(asset.digest ?? '')
    if (asset.name && match) digests.set(asset.name, match[1]!)
  }
  return digests
}

/**
 * The hash of one Codex asset: it must be in the release's SHA256SUMS file, equal to the digest
 * GitHub computed for the uploaded asset, and equal to what was measured on the download.
 */
export function codexHash(
  asset: string,
  sums: Map<string, string>,
  digests: Map<string, string>,
  measured: string,
): string {
  const listed = sums.get(asset)
  const github = digests.get(asset)
  if (!listed) throw new Error(`${asset}: not in SHA256SUMS`)
  if (!github) throw new Error(`${asset}: GitHub shows no digest`)
  if (listed !== github) {
    throw new Error(`${asset}: SHA256SUMS and the GitHub digest differ`)
  }
  if (measured !== listed) {
    throw new Error(`${asset}: the download does not match SHA256SUMS`)
  }
  return listed
}

/** What a `tar -tv` listing says: the bytes of the regular files and every path. */
export function summariseTarListing(listing: string): {
  unpackedSize: number
  paths: string[]
  unsafe: string[]
} {
  let unpackedSize = 0
  const paths: string[] = []
  const unsafe: string[] = []
  for (const line of listing.split('\n')) {
    // `-rwxr-xr-x 0/0 294000000 1970-01-01 00:00 bin/codex`
    const match = /^(\S)\S*\s+\S+\s+(\d+)\s+\S+\s+\S+\s+(.+)$/.exec(line)
    if (!match) continue
    const [, type, size, rawPath] = match
    const path = rawPath!.replace(/ -> .*$/, '').replace(/ link to .*$/, '')
    paths.push(path)
    if (type === '-') unpackedSize += Number(size)
    else if (type !== 'd') unsafe.push(`${type} ${path}`)
    if (path.startsWith('/') || path.split('/').includes('..')) {
      unsafe.push(`path ${path}`)
    }
  }
  return { unpackedSize, paths, unsafe }
}

export interface CodexMeasured {
  sha256: string
  size: number
  listing: string
}

/** The catalog entry for a Codex release; every platform asset was downloaded and measured. */
export function codexEntry(
  version: string,
  sums: Map<string, string>,
  digests: Map<string, string>,
  measured: Record<PlatformKey, CodexMeasured>,
  executables: Record<PlatformKey, string>,
): CatalogVersion {
  const platforms: CatalogVersion['platforms'] = {}
  for (const key of PLATFORM_KEYS) {
    const asset = codexAssetName(key)
    const item = measured[key]
    const sha256 = codexHash(asset, sums, digests, item.sha256)
    const summary = summariseTarListing(item.listing)
    if (summary.unsafe.length > 0) {
      throw new Error(
        `${asset}: the archive holds entries holzi refuses: ${summary.unsafe.slice(0, 3).join(', ')}`,
      )
    }
    const executable = executables[key]
    if (!summary.paths.some((p) => p.replace(/^\.\//, '') === executable)) {
      throw new Error(`${asset}: the archive does not hold ${executable}`)
    }
    platforms[key] = {
      url: codexAssetUrl(version, key),
      sha256,
      size: item.size,
      kind: 'tar.gz',
      unpacked_size: summary.unpackedSize,
    }
  }
  return { version, platforms }
}

// ---- catalog.json --------------------------------------------------------------------------

/**
 * Puts `entry` into the vendor's list, newest first. A version that is already listed is an
 * error: a published release's hash must never change silently.
 */
export function insertVersion(
  catalog: Catalog,
  vendorId: string,
  entry: CatalogVersion,
): Catalog {
  const vendor = catalog.vendors.find((v) => v.vendor === vendorId)
  if (!vendor) throw new Error(`the catalog has no vendor ${vendorId}`)
  if (vendor.versions.some((v) => v.version === entry.version)) {
    throw new Error(`${vendorId} ${entry.version} is already listed`)
  }
  const versions = [...vendor.versions, entry].sort((a, b) =>
    compareSemver(b.version, a.version),
  )
  return {
    ...catalog,
    vendors: catalog.vendors.map((v) =>
      v === vendor ? { ...v, versions } : v,
    ),
  }
}

/** The problems of a catalog, in the terms of the Rust validation (`catalog.rs`). */
export function catalogProblems(catalog: Catalog, only?: string): string[] {
  const problems: string[] = []
  for (const vendor of catalog.vendors) {
    if (only !== undefined && vendor.vendor !== only) continue
    if (vendor.versions.length === 0) {
      problems.push(`${vendor.vendor}: no version`)
    }
    vendor.versions.forEach((entry, index) => {
      const previous = vendor.versions[index - 1]
      if (previous && compareSemver(previous.version, entry.version) <= 0) {
        problems.push(`${vendor.vendor}: ${entry.version} is not descending`)
      }
      for (const [key, artifact] of Object.entries(entry.platforms)) {
        const at = `${vendor.vendor} ${entry.version} ${key}`
        if (!isSha256(artifact?.sha256)) problems.push(`${at}: sha256`)
        if (!artifact?.url.startsWith('https://')) problems.push(`${at}: url`)
        if (
          artifact?.kind === 'tar.gz' &&
          artifact.unpacked_size === undefined
        ) {
          problems.push(`${at}: unpacked_size`)
        }
      }
    })
  }
  return problems
}
