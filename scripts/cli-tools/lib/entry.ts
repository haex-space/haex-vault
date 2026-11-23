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

/** The six keys Claude Code and Codex list; their entry builders produce exactly these. */
export const PLATFORM_KEYS: readonly PlatformKey[] = [
  'linux-x64',
  'linux-arm64',
  'darwin-x64',
  'darwin-arm64',
  'win32-x64',
  'win32-arm64',
]

/** A key a catalog entry may list: the six, and the x64 build without AVX2 (spec 096, D3). */
export type CatalogPlatformKey = PlatformKey | 'linux-x64-baseline'

export interface Artifact {
  url: string
  sha256: string
  size: number
  kind: 'file' | 'tar.gz' | 'zip'
  unpacked_size?: number
  /** The program needs a CPU with AVX2; absent means false. */
  requires_avx2?: boolean
}

export interface CatalogVersion {
  version: string
  platforms: Partial<Record<CatalogPlatformKey, Artifact>>
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
    const l = BigInt(left[i]!)
    const r = BigInt(right[i]!)
    if (l !== r) return l < r ? -1 : 1
  }
  if (left[4] === right[4]) return 0
  if (left[4] === undefined) return 1
  if (right[4] === undefined) return -1
  const leftIds = left[4].split('.')
  const rightIds = right[4].split('.')
  for (let i = 0; i < Math.min(leftIds.length, rightIds.length); i++) {
    const l = leftIds[i]!
    const r = rightIds[i]!
    if (l === r) continue
    const lNumeric = /^\d+$/.test(l)
    const rNumeric = /^\d+$/.test(r)
    if (lNumeric && rNumeric) {
      const lNumber = BigInt(l)
      const rNumber = BigInt(r)
      return lNumber < rNumber ? -1 : 1
    }
    if (lNumeric !== rNumeric) return lNumeric ? -1 : 1
    return l < r ? -1 : 1
  }
  return Math.sign(leftIds.length - rightIds.length)
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
    if (line.trim() === '') continue
    // GNU tar: `-rwxr-xr-x 0/0 294000000 1970-01-01 00:00 bin/codex`
    // BSD tar: `-rwxr-xr-x 0 root wheel 294000000 Jan 1 1970 bin/codex`
    const gnu = /^(\S)\S*\s+\S+\s+(\d+)\s+\S+\s+\S+\s+(.+)$/.exec(line)
    const bsd =
      /^(\S)\S*\s+\d+\s+\S+\s+\S+\s+(\d+)\s+\S+\s+\d+\s+\S+\s+(.+)$/.exec(line)
    const match = gnu ?? bsd
    if (!match) {
      unsafe.push(`unrecognized tar listing row: ${line}`)
      continue
    }
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
        `${asset}: the archive holds entries HaexVault refuses: ${summary.unsafe.slice(0, 3).join(', ')}`,
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

// ---- Abacus AI: release archives with a Sigstore bundle ------------------------------------

export const ABACUS_DOWNLOAD_BASE = 'https://static.abacus.ai/cli/releases'

/** The keys the vendor builds and the app can offer (musl and `win32-x64-baseline` are not). */
export const ABACUS_PLATFORM_KEYS: readonly CatalogPlatformKey[] = [
  'linux-x64',
  'linux-x64-baseline',
  'linux-arm64',
  'darwin-x64',
  'darwin-arm64',
  'win32-x64',
  'win32-arm64',
]

/**
 * The programs that need AVX2. The vendor's `-baseline` sibling of `darwin-x64` and `win32-x64`
 * is not offered (spec 096, D3), and a `linux-x64` host without AVX2 would otherwise get a
 * program for a CPU it does not have: it waits for the `linux-x64-baseline` entry.
 */
const ABACUS_REQUIRES_AVX2: readonly CatalogPlatformKey[] = [
  'linux-x64',
  'darwin-x64',
  'win32-x64',
]

export const ABACUS_OIDC_ISSUER = 'https://token.actions.githubusercontent.com'

const abacusKind = (key: CatalogPlatformKey): 'tar.gz' | 'zip' =>
  key.startsWith('win32') ? 'zip' : 'tar.gz'

export const abacusAssetName = (key: CatalogPlatformKey): string =>
  `abacusai-${key}.${abacusKind(key)}`

export const abacusAssetUrl = (
  version: string,
  key: CatalogPlatformKey,
): string => `${ABACUS_DOWNLOAD_BASE}/${version}/${abacusAssetName(key)}`

/** The Sigstore bundle that sits next to the program inside the archive. */
export const abacusBundleName = (key: CatalogPlatformKey): string =>
  `abacusai-${key}.sigstore`

function assertSemver(version: string): void {
  if (!SEMVER.test(version)) throw new Error(`not a semver version: ${version}`)
}

/** The signing identity of a release: its workflow at its own tag, nothing else. */
export function abacusIdentity(version: string): string {
  assertSemver(version)
  return `https://github.com/abacusai/codellm/.github/workflows/release-cli-native.yml@refs/tags/cli-v${version}`
}

const escapeRegExp = (text: string): string =>
  text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')

/** `abacusIdentity` as the anchored pattern `cosign verify-blob` takes. */
export const abacusIdentityPattern = (version: string): string =>
  `^${escapeRegExp(abacusIdentity(version))}$`

export type SigstoreTool = 'sigstore' | 'cosign'

/**
 * The command that checks a bundle against the pinned identity and issuer. `sigstore` takes the
 * program's SHA-256 (its `identity` check matches the identity exactly); `cosign verify-blob`
 * takes the program file and hashes it itself, with the identity as a pattern.
 */
export function abacusVerifyCommand(
  tool: SigstoreTool,
  version: string,
  bundle: string,
  subject: { sha256: string; file: string },
): string[] {
  if (tool === 'sigstore') {
    return [
      'sigstore',
      'verify',
      'identity',
      '--bundle',
      bundle,
      '--cert-identity',
      abacusIdentity(version),
      '--cert-oidc-issuer',
      ABACUS_OIDC_ISSUER,
      `sha256:${subject.sha256}`,
    ]
  }
  return [
    'cosign',
    'verify-blob',
    '--new-bundle-format',
    '--bundle',
    bundle,
    '--certificate-identity-regexp',
    abacusIdentityPattern(version),
    '--certificate-oidc-issuer',
    ABACUS_OIDC_ISSUER,
    subject.file,
  ]
}

/**
 * The platform keys a run works on. Abacus needs at least one `--platform` and only those are
 * processed; the other vendors list all six platforms and take no selector.
 */
export function selectPlatforms(
  vendor: string,
  requested: readonly string[],
): CatalogPlatformKey[] {
  if (vendor !== 'abacusai') {
    if (requested.length > 0) {
      throw new Error(`--platform is only for abacusai, not ${vendor}`)
    }
    return [...PLATFORM_KEYS]
  }
  if (requested.length === 0) {
    throw new Error(
      `abacusai needs at least one --platform of ${ABACUS_PLATFORM_KEYS.join(', ')}`,
    )
  }
  const unknown = requested.filter(
    (key) => !ABACUS_PLATFORM_KEYS.includes(key as CatalogPlatformKey),
  )
  if (unknown.length > 0) {
    throw new Error(`no such abacusai platform: ${unknown.join(', ')}`)
  }
  return ABACUS_PLATFORM_KEYS.filter((key) => requested.includes(key))
}

/** One row of a zip's central directory: `mode` is its Unix mode, 0 when the zip has none. */
export interface ZipListingEntry {
  name: string
  size: number
  mode: number
}

/** What an archive listing says: the bytes of the regular files, every path, what is refused. */
export interface ArchiveSummary {
  unpackedSize: number
  paths: string[]
  unsafe: string[]
}

/** `summariseTarListing` for a zip: refuses links, devices and paths that escape. */
export function summariseZipListing(
  entries: ZipListingEntry[],
): ArchiveSummary {
  let unpackedSize = 0
  const paths: string[] = []
  const unsafe: string[] = []
  for (const { name, size, mode } of entries) {
    paths.push(name)
    const type = mode & 0o170000
    if (type === 0 || type === 0o100000) {
      if (!name.endsWith('/')) unpackedSize += size
    } else if (type !== 0o040000) {
      unsafe.push(`mode ${mode.toString(8)} ${name}`)
    }
    if (
      name.startsWith('/') ||
      name.includes('\\') ||
      /^[A-Za-z]:/.test(name) ||
      name.split('/').includes('..')
    ) {
      unsafe.push(`path ${name}`)
    }
  }
  return { unpackedSize, paths, unsafe }
}

export interface AbacusMeasured {
  sha256: string
  size: number
  summary: ArchiveSummary
  /** Whether the bundle verified against the program, and with which tool. */
  signature: { verified: boolean; tool: string }
}

/**
 * The catalog entry for an Abacus release, from the platforms that were downloaded, listed and
 * signature-checked. Only the measured keys are in it.
 */
export function abacusEntry(
  version: string,
  measured: Partial<Record<CatalogPlatformKey, AbacusMeasured>>,
  executables: Partial<Record<CatalogPlatformKey, string>>,
): CatalogVersion {
  assertSemver(version)
  const platforms: CatalogVersion['platforms'] = {}
  for (const key of ABACUS_PLATFORM_KEYS) {
    const item = measured[key]
    if (!item) continue
    const asset = abacusAssetName(key)
    if (!item.signature.verified) {
      throw new Error(
        `${asset}: the Sigstore bundle did not verify (a signature by a private repository's workflow shows who released the file, not what it contains)`,
      )
    }
    if (!isSha256(item.sha256)) throw new Error(`${asset}: not a SHA-256`)
    if (!Number.isSafeInteger(item.size) || item.size <= 0) {
      throw new Error(`${asset}: the size is not a positive integer`)
    }
    if (item.summary.unsafe.length > 0) {
      throw new Error(
        `${asset}: the archive holds entries HaexVault refuses: ${item.summary.unsafe.slice(0, 3).join(', ')}`,
      )
    }
    const paths = item.summary.paths.map((p) => p.replace(/^\.\//, ''))
    const executable = executables[key]
    if (!executable || !paths.includes(executable)) {
      throw new Error(
        `${asset}: the archive does not hold ${String(executable)}`,
      )
    }
    if (!paths.includes(abacusBundleName(key))) {
      throw new Error(`${asset}: the archive does not hold its Sigstore bundle`)
    }
    platforms[key] = {
      url: abacusAssetUrl(version, key),
      sha256: item.sha256,
      size: item.size,
      kind: abacusKind(key),
      unpacked_size: item.summary.unpackedSize,
      ...(ABACUS_REQUIRES_AVX2.includes(key) ? { requires_avx2: true } : {}),
    }
  }
  if (Object.keys(platforms).length === 0) {
    throw new Error('no platform was measured')
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

/**
 * What a run does with its entry: the catalog to write, or `null` for `--measure-only`, which
 * never changes the catalog (the entry is only reported). Throws on a catalog with problems.
 */
export function catalogToWrite(
  catalog: Catalog,
  vendorId: string,
  entry: CatalogVersion,
  measureOnly: boolean,
): Catalog | null {
  if (measureOnly) return null
  const next = insertVersion(catalog, vendorId, entry)
  // Only the edited vendor: the others may still be waiting for their first entry.
  const problems = catalogProblems(next, vendorId)
  if (problems.length > 0) throw new Error(problems.join('\n'))
  return next
}

/** The problems of a catalog, in the terms of the Rust validation (`catalog.rs`). */
export function catalogProblems(catalog: Catalog, only?: string): string[] {
  const problems: string[] = []
  for (const vendor of catalog.vendors) {
    if (only !== undefined && vendor.vendor !== only) continue
    // No version is fine: a vendor's definition ships before its first measured version.
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
          (artifact?.kind === 'tar.gz' || artifact?.kind === 'zip') &&
          artifact.unpacked_size === undefined
        ) {
          problems.push(`${at}: unpacked_size`)
        }
      }
    })
  }
  return problems
}
