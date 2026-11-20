// Maintainer script (spec 094 FR-006, FR-009): puts one release of a delegate tool into
// `src-tauri/src/cli_tools/catalog.json`, the tested-version list haex-vault ships.
//
//   node scripts/cli-tools/add-version.ts --vendor claude --version 2.1.311
//   node scripts/cli-tools/add-version.ts --vendor codex  --version 0.169.0
//
// The app never asks a vendor what the latest version is and never trusts a vendor's server at
// install time: what it checks is the SHA-256 written here. So this script is where trust is
// established, and it refuses to write an entry it could not verify:
//
// - Claude Code: `manifest.json` of the release is checked against its detached signature
//   `manifest.json.sig` with `gpg`, in a throw-away keyring, against the pinned fingerprint of
//   the vendor's release key. The checksums come from that signed manifest.
// - Codex: the SHA-256 of every platform asset must equal the entry in the release's
//   `codex-package_SHA256SUMS`, the digest GitHub computed for the uploaded asset, and the hash
//   measured on the download itself. (A Sigstore bundle exists only for some assets; it is not
//   checked here, and this script says so.) The download is streamed through `tar -t`, which
//   yields the unpacked size and refuses archives with links, devices or paths that escape.
//
// Then the containment measurement of FR-021 (`check-containment.py`) is the second gate: the
// download of this device's platform is checked against the entry, started under `strace`, and a
// version whose writes outside the vault are not on `allowed-residuals.json` is not committed.
// It needs Linux; `--skip-containment` writes the entry without it and says so, for a maintainer
// who measures on another machine.
import { spawn, spawnSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import {
  chmodSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { parseArgs } from 'node:util'
import { Readable } from 'node:stream'

import {
  CLAUDE_DOWNLOAD_BASE,
  PLATFORM_KEYS,
  catalogProblems,
  claudeEntry,
  codexAssetName,
  codexAssetUrl,
  codexEntry,
  githubDigests,
  insertVersion,
  parseSha256Sums,
  type Artifact as CatalogArtifact,
  type Catalog,
  type CodexMeasured,
  type PlatformKey,
} from './lib/entry.ts'

/** The fingerprint of Anthropic's Claude Code release signing key (code.claude.com/docs/en/setup). */
const CLAUDE_KEY_FINGERPRINT = '31DDDE24DDFAB679F42D7BD2BAA929FF1A7ECACE'
const CLAUDE_KEY_URL = 'https://downloads.claude.ai/keys/claude-code.asc'
const CODEX_RELEASE_API =
  'https://api.github.com/repos/openai/codex/releases/tags'
const CATALOG = new URL(
  '../../src-tauri/src/cli_tools/catalog.json',
  import.meta.url,
)

async function fetchBytes(url: string): Promise<Buffer> {
  const response = await fetch(url)
  if (!response.ok) throw new Error(`${url}: HTTP ${response.status}`)
  return Buffer.from(await response.arrayBuffer())
}

function gpg(home: string, args: string[], input?: Buffer) {
  return spawnSync('gpg', ['--batch', '--no-tty', '--homedir', home, ...args], {
    input,
    encoding: 'utf8',
  })
}

/** Verifies the signature of a Claude Code manifest in an empty, throw-away keyring. */
async function claudeManifest(version: string): Promise<unknown> {
  const base = `${CLAUDE_DOWNLOAD_BASE}/${version}`
  const [manifest, signature, key] = await Promise.all([
    fetchBytes(`${base}/manifest.json`),
    fetchBytes(`${base}/manifest.json.sig`),
    fetchBytes(CLAUDE_KEY_URL),
  ])
  const home = mkdtempSync(join(tmpdir(), 'haex-gpg-'))
  try {
    const shown = gpg(home, ['--with-colons', '--show-keys'], key)
    const fingerprints = shown.stdout
      .split('\n')
      .filter((line) => line.startsWith('fpr:'))
      .map((line) => line.split(':')[9])
    if (!fingerprints.includes(CLAUDE_KEY_FINGERPRINT)) {
      throw new Error(
        `the published release key does not carry the pinned fingerprint (${fingerprints.join(', ')})`,
      )
    }
    const imported = gpg(home, ['--import'], key)
    if (imported.status !== 0)
      throw new Error(`gpg --import: ${imported.stderr}`)
    const files = join(home, 'files')
    writeFileSync(`${files}.json`, manifest)
    writeFileSync(`${files}.sig`, signature)
    const verified = gpg(home, [
      '--status-fd',
      '1',
      '--verify',
      `${files}.sig`,
      `${files}.json`,
    ])
    // `[GNUPG:] VALIDSIG <fpr> <date> <ts> <exp> <ver> <rsv> <pk> <hash> <class> <primary-fpr>`
    const valid = verified.stdout
      .split('\n')
      .find((line) => line.startsWith('[GNUPG:] VALIDSIG'))
    const primary = valid?.split(' ')[11]
    if (verified.status !== 0 || primary !== CLAUDE_KEY_FINGERPRINT) {
      throw new Error(
        `the manifest signature is not valid:\n${verified.stderr}`,
      )
    }
    console.log(`claude ${version}: manifest signature valid (${primary})`)
    return JSON.parse(manifest.toString('utf8'))
  } finally {
    rmSync(home, { recursive: true, force: true })
  }
}

/** Streams one Codex asset through `sha256` and `tar -t`: hash, size and listing in one pass. */
async function measureCodexAsset(url: string): Promise<CodexMeasured> {
  const response = await fetch(url)
  if (!response.ok || !response.body) {
    throw new Error(`${url}: HTTP ${response.status}`)
  }
  const tar = spawn('tar', ['-tvz'], { stdio: ['pipe', 'pipe', 'pipe'] })
  const hash = createHash('sha256')
  let size = 0
  let listing = ''
  let errors = ''
  tar.stdout.on('data', (chunk: Buffer) => (listing += chunk.toString('utf8')))
  tar.stderr.on('data', (chunk: Buffer) => (errors += chunk.toString('utf8')))
  const done = new Promise<void>((resolve, reject) => {
    tar.on('close', (code) =>
      code === 0 ? resolve() : reject(new Error(`tar -t: ${errors}`)),
    )
  })
  tar.stdin.on('error', () => {})
  for await (const chunk of Readable.fromWeb(response.body as never)) {
    const bytes = chunk as Buffer
    hash.update(bytes)
    size += bytes.length
    if (!tar.stdin.write(bytes)) {
      await new Promise((resolve) => tar.stdin.once('drain', resolve))
    }
  }
  tar.stdin.end()
  await done
  return { sha256: hash.digest('hex'), size, listing }
}

async function codexRelease(
  version: string,
  executables: Record<PlatformKey, string>,
) {
  const tag = `rust-v${version}`
  const base = `https://github.com/openai/codex/releases/download/${tag}`
  const sums = parseSha256Sums(
    (await fetchBytes(`${base}/codex-package_SHA256SUMS`)).toString('utf8'),
  )
  const release = JSON.parse(
    (await fetchBytes(`${CODEX_RELEASE_API}/${tag}`)).toString('utf8'),
  )
  const digests = githubDigests(release)
  const measured = {} as Record<PlatformKey, CodexMeasured>
  for (const key of PLATFORM_KEYS) {
    console.log(`codex ${version}: measuring ${codexAssetName(key)} ...`)
    measured[key] = await measureCodexAsset(codexAssetUrl(version, key))
  }
  console.log(
    'codex: the Sigstore bundle is not checked by this script (not published for every asset)',
  )
  return codexEntry(version, sums, digests, measured, executables)
}

/** What each tool is started with for the measurement: its start-up, with no login and no turn. */
const CONTAINMENT_PROBES: Record<string, string[]> = {
  claude: ['-p', 'ping'],
  codex: ['app-server', '--stdio'],
}

/**
 * Gate of FR-021: starts the download for this device under `strace` with the environment of the
 * base layer and refuses the version when something outside the vault is written that
 * `allowed-residuals.json` does not list.
 */
async function measureContainment(
  vendor: string,
  entry: { platforms: Partial<Record<PlatformKey, CatalogArtifact>> },
  executable: Record<string, string>,
) {
  const key = `${process.platform}-${process.arch === 'arm64' ? 'arm64' : 'x64'}`
  const artifact = entry.platforms[key as PlatformKey]
  if (process.platform !== 'linux' || !artifact) {
    throw new Error(
      `the containment measurement needs Linux and an entry for ${key}; measure on a Linux machine or pass --skip-containment`,
    )
  }
  console.log(`${vendor}: containment measurement with ${key} ...`)
  const body = await fetchBytes(artifact.url)
  const sha = createHash('sha256').update(body).digest('hex')
  if (sha !== artifact.sha256) {
    throw new Error(`the download for ${key} does not match its catalog entry`)
  }
  const directory = mkdtempSync(join(tmpdir(), 'haex-containment-'))
  try {
    const program = join(directory, executable[key]!)
    if (artifact.kind === 'tar.gz') {
      writeFileSync(join(directory, 'package.tar.gz'), body)
      const unpacked = spawnSync(
        'tar',
        ['-xzf', join(directory, 'package.tar.gz'), '-C', directory],
        { stdio: 'inherit' },
      )
      if (unpacked.status !== 0) throw new Error('the package did not unpack')
    } else {
      writeFileSync(program, body)
      chmodSync(program, 0o755)
    }
    const run = spawnSync(
      'python3',
      [
        '-I',
        new URL('./check-containment.py', import.meta.url).pathname,
        '--vendor',
        vendor,
        '--program',
        program,
        '--',
        ...CONTAINMENT_PROBES[vendor]!,
      ],
      { stdio: 'inherit' },
    )
    if (run.status !== 0) {
      throw new Error(
        `${vendor}: the tool writes outside the vault beyond the listed residuals; the version is not added`,
      )
    }
  } finally {
    rmSync(directory, { recursive: true, force: true })
  }
}

async function main() {
  const { values } = parseArgs({
    options: {
      vendor: { type: 'string' },
      version: { type: 'string' },
      'skip-containment': { type: 'boolean' },
    },
  })
  const { vendor, version } = values
  if (!vendor || !version || !['claude', 'codex'].includes(vendor)) {
    console.error(
      'usage: add-version.ts --vendor claude|codex --version <x.y.z> [--skip-containment]',
    )
    process.exit(2)
  }
  const catalog = JSON.parse(readFileSync(CATALOG, 'utf8')) as Catalog
  const definition = catalog.vendors.find((v) => v.vendor === vendor)
  if (!definition) throw new Error(`the catalog has no vendor ${vendor}`)

  const entry =
    vendor === 'claude'
      ? claudeEntry(version, (await claudeManifest(version)) as never)
      : await codexRelease(
          version,
          definition.executable as Record<PlatformKey, string>,
        )

  if (values['skip-containment']) {
    console.warn(
      `${vendor} ${version}: containment NOT measured (--skip-containment); measure it on Linux before this entry ships`,
    )
  } else {
    await measureContainment(
      vendor,
      entry,
      definition.executable as Record<string, string>,
    )
  }

  const next = insertVersion(catalog, vendor, entry)
  // Only the edited vendor: the other one may still be waiting for its first entry.
  const problems = catalogProblems(next, vendor)
  if (problems.length > 0) throw new Error(problems.join('\n'))
  writeFileSync(CATALOG, `${JSON.stringify(next, null, 2)}\n`)
  spawnSync('pnpm', ['exec', 'prettier', '--write', CATALOG.pathname], {
    stdio: 'inherit',
  })
  console.log(`${vendor} ${version} added to the catalog`)
}

await main()
