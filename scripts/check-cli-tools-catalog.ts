// `pnpm check:cli-tools-catalog` (spec 094, T007): the helpers of `scripts/cli-tools/add-version.ts`
// turn vendor release data into catalog entries, and the shipped `catalog.json` is what they
// would write. The Rust side validates the same file again (`catalog_tests.rs`); this keeps the
// maintainer script honest without the network.
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'

import {
  PLATFORM_KEYS,
  catalogProblems,
  claudeEntry,
  codexAssetName,
  codexEntry,
  codexHash,
  compareSemver,
  githubDigests,
  insertVersion,
  parseSha256Sums,
  summariseTarListing,
  type Catalog,
  type CodexMeasured,
  type PlatformKey,
} from './cli-tools/lib/entry.ts'

const HASH = (n: number) => n.toString(16).padStart(64, '0')

const manifest = (version = '1.2.3') => ({
  version,
  platforms: Object.fromEntries(
    PLATFORM_KEYS.map((key, index) => [
      key,
      {
        binary: key.startsWith('win32') ? 'claude.exe' : 'claude',
        checksum: HASH(index + 1),
        size: 1000 + index,
      },
    ]),
  ),
})

test('semantic versions are ordered numerically, a pre-release before its release', () => {
  assert.equal(compareSemver('2.10.0', '2.9.0'), 1)
  assert.equal(compareSemver('2.9.0', '2.10.0'), -1)
  assert.equal(compareSemver('1.0.0', '1.0.0'), 0)
  assert.equal(compareSemver('1.0.0-rc.1', '1.0.0'), -1)
  assert.equal(compareSemver('1.0.0', '1.0.0-rc.1'), 1)
  assert.equal(compareSemver('1.0.0-rc.2', '1.0.0-rc.10'), -1)
  assert.equal(compareSemver('1.0.0-rc.10', '1.0.0-rc.2'), 1)
  assert.throws(() => compareSemver('latest', '1.0.0'))
})

test('a Claude Code entry takes every platform from the manifest and builds the download URL', () => {
  const entry = claudeEntry('1.2.3', manifest())
  assert.deepEqual(Object.keys(entry.platforms), [...PLATFORM_KEYS])
  assert.deepEqual(entry.platforms['linux-x64'], {
    url: 'https://downloads.claude.ai/claude-code-releases/1.2.3/linux-x64/claude',
    sha256: HASH(1),
    size: 1000,
    kind: 'file',
  })
  assert.equal(
    entry.platforms['win32-x64']?.url,
    'https://downloads.claude.ai/claude-code-releases/1.2.3/win32-x64/claude.exe',
  )
})

test('a Claude Code manifest that is for another version or misses a platform is refused', () => {
  assert.throws(() => claudeEntry('1.2.4', manifest('1.2.3')), /not 1\.2\.4/)
  const missing = manifest()
  delete missing.platforms['darwin-arm64']
  assert.throws(() => claudeEntry('1.2.3', missing), /no darwin-arm64/)
  const bad = manifest()
  bad.platforms['linux-x64']!.checksum = 'ABC'
  assert.throws(() => claudeEntry('1.2.3', bad), /not a SHA-256/)
  const zero = manifest()
  zero.platforms['linux-x64']!.size = 0
  assert.throws(() => claudeEntry('1.2.3', zero), /size/)
  const path = manifest()
  path.platforms['linux-x64']!.binary = '../claude'
  assert.throws(() => claudeEntry('1.2.3', path), /file name/)
})

test('SHA256SUMS lines and GitHub digests are read into maps', () => {
  const sums = parseSha256Sums(
    `${HASH(1)}  codex-package-a.tar.gz\n${HASH(2)} *codex-package-b.tar.gz\nnot a line\n`,
  )
  assert.equal(sums.get('codex-package-a.tar.gz'), HASH(1))
  assert.equal(sums.get('codex-package-b.tar.gz'), HASH(2))
  assert.equal(sums.size, 2)
  const digests = githubDigests({
    assets: [
      { name: 'a', digest: `sha256:${HASH(1)}` },
      { name: 'b', digest: null },
      { name: 'c', digest: 'sha1:abc' },
    ],
  })
  assert.deepEqual([...digests], [['a', HASH(1)]])
})

test('a Codex hash needs the sums file, the GitHub digest and the measured download to agree', () => {
  const sums = new Map([['a', HASH(1)]])
  const digests = new Map([['a', HASH(1)]])
  assert.equal(codexHash('a', sums, digests, HASH(1)), HASH(1))
  assert.throws(() => codexHash('a', new Map(), digests, HASH(1)), /SHA256SUMS/)
  assert.throws(() => codexHash('a', sums, new Map(), HASH(1)), /no digest/)
  assert.throws(
    () => codexHash('a', sums, new Map([['a', HASH(2)]]), HASH(1)),
    /differ/,
  )
  assert.throws(() => codexHash('a', sums, digests, HASH(3)), /does not match/)
})

test('a tar listing gives the unpacked size and flags links, devices and escaping paths', () => {
  const listing = [
    'drwxr-xr-x 0/0 0 1970-01-01 00:00 bin/',
    '-rwxr-xr-x 0/0 1000 1970-01-01 00:00 bin/codex',
    '-rw-r--r-- 0/0 24 1970-01-01 00:00 codex-package.json',
  ].join('\n')
  assert.deepEqual(summariseTarListing(listing), {
    unpackedSize: 1024,
    paths: ['bin/', 'bin/codex', 'codex-package.json'],
    unsafe: [],
  })
  const bad = summariseTarListing(
    [
      'lrwxrwxrwx 0/0 0 1970-01-01 00:00 bin/link -> /etc/passwd',
      'hrwxr-xr-x 0/0 0 1970-01-01 00:00 bin/hard link to bin/codex',
      '-rw-r--r-- 0/0 1 1970-01-01 00:00 ../escape',
      '-rw-r--r-- 0/0 1 1970-01-01 00:00 /absolute',
    ].join('\n'),
  )
  assert.equal(bad.unsafe.length, 4)
  const bsd = summariseTarListing(
    [
      'drwxr-xr-x 0 root wheel 0 Jan 1 1970 bin/',
      '-rwxr-xr-x 0 root wheel 1000 Jan 1 1970 bin/codex',
      '-rw-r--r-- 0 root wheel 24 Jan 1 1970 codex-package.json',
    ].join('\n'),
  )
  assert.deepEqual(bsd, {
    unpackedSize: 1024,
    paths: ['bin/', 'bin/codex', 'codex-package.json'],
    unsafe: [],
  })
  assert.ok(
    summariseTarListing('unrecognized row').unsafe.length > 0,
    'unknown listing rows are refused instead of discarded',
  )
})

const measured = (listing: string): Record<PlatformKey, CodexMeasured> =>
  Object.fromEntries(
    PLATFORM_KEYS.map((key, index) => [
      key,
      { sha256: HASH(index + 1), size: 500 + index, listing },
    ]),
  ) as Record<PlatformKey, CodexMeasured>

const codexInputs = () => {
  const sums = new Map(
    PLATFORM_KEYS.map((key, i) => [codexAssetName(key), HASH(i + 1)]),
  )
  return { sums, digests: new Map(sums) }
}

test('a Codex entry needs its program inside every archive', () => {
  const { sums, digests } = codexInputs()
  const executables = Object.fromEntries(
    PLATFORM_KEYS.map((key) => [key, 'bin/codex']),
  ) as Record<PlatformKey, string>
  const good = '-rwxr-xr-x 0/0 2000 1970-01-01 00:00 bin/codex'
  const entry = codexEntry('0.1.0', sums, digests, measured(good), executables)
  assert.equal(entry.platforms['linux-x64']?.kind, 'tar.gz')
  assert.equal(entry.platforms['linux-x64']?.unpacked_size, 2000)
  assert.equal(
    entry.platforms['linux-x64']?.url,
    'https://github.com/openai/codex/releases/download/rust-v0.1.0/codex-package-x86_64-unknown-linux-musl.tar.gz',
  )
  assert.throws(
    () =>
      codexEntry(
        '0.1.0',
        sums,
        digests,
        measured('-rw-r--r-- 0/0 1 1970-01-01 00:00 other'),
        executables,
      ),
    /does not hold bin\/codex/,
  )
  assert.throws(
    () =>
      codexEntry(
        '0.1.0',
        sums,
        digests,
        measured('lrwxrwxrwx 0/0 0 1970-01-01 00:00 bin/codex -> x'),
        executables,
      ),
    /refuses/,
  )
})

const catalogWith = (versions: string[]): Catalog => ({
  schema: 1,
  vendors: [
    {
      vendor: 'claude',
      versions: versions.map((v) => claudeEntry(v, manifest(v))),
    },
  ],
})

test('a version is inserted newest first and never twice', () => {
  const catalog = catalogWith(['2.9.0', '2.0.0'])
  const next = insertVersion(
    catalog,
    'claude',
    claudeEntry('2.10.0', manifest('2.10.0')),
  )
  assert.deepEqual(
    next.vendors[0]!.versions.map((v) => v.version),
    ['2.10.0', '2.9.0', '2.0.0'],
  )
  assert.equal(
    catalog.vendors[0]!.versions.length,
    2,
    'the input is not changed',
  )
  assert.throws(
    () =>
      insertVersion(catalog, 'claude', claudeEntry('2.9.0', manifest('2.9.0'))),
    /already listed/,
  )
  assert.throws(
    () =>
      insertVersion(catalog, 'nobody', claudeEntry('1.0.0', manifest('1.0.0'))),
    /no vendor/,
  )
})

test('the problems of a catalog are found', () => {
  assert.deepEqual(catalogProblems(catalogWith(['2.0.0', '1.0.0'])), [])
  const unordered = catalogWith(['1.0.0', '2.0.0'])
  assert.match(catalogProblems(unordered).join(), /not descending/)
  const bad = catalogWith(['1.0.0'])
  bad.vendors[0]!.versions[0]!.platforms['linux-x64']!.sha256 = 'x'
  assert.match(catalogProblems(bad).join(), /sha256/)
  assert.deepEqual(
    catalogProblems(
      { schema: 1, vendors: [{ vendor: 'empty', versions: [] }] },
      'other',
    ),
    [],
    'only the named vendor is checked',
  )
})

test('the shipped catalog has no problems; Claude Code and Codex are listed, Abacus waits for its first measured version', () => {
  const shipped = JSON.parse(
    readFileSync(
      new URL('../src-tauri/src/cli_tools/catalog.json', import.meta.url),
      'utf8',
    ),
  ) as Catalog
  assert.deepEqual(catalogProblems(shipped), [])
  assert.deepEqual(
    shipped.vendors.map((v) => v.vendor),
    ['claude', 'codex', 'abacusai'],
  )
  for (const vendor of shipped.vendors.filter((v) => v.vendor !== 'abacusai')) {
    assert.ok(vendor.versions.length > 0, vendor.vendor)
  }
})
