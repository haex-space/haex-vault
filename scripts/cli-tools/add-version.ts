// Maintainer script (spec 094 FR-006, FR-009): puts one release of a delegate tool into
// `src-tauri/src/cli_tools/catalog.json`, the tested-version list haex-vault ships.
//
//   node scripts/cli-tools/add-version.ts --vendor claude --version 2.1.311
//   node scripts/cli-tools/add-version.ts --vendor codex  --version 0.169.0
//   node scripts/cli-tools/add-version.ts --vendor abacusai --version 2.10.23 \
//        --platform linux-x64 --platform linux-arm64 [--measure-only]
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
// - Abacus AI: every archive holds the program and a Sigstore bundle, a signature over the
//   program's SHA-256. Each `--platform` (repeatable, at least one; only those are processed)
//   is downloaded into an operator-only temporary folder, its SHA-256 and size measured on the
//   stream, its entries listed (`tar -t` / the zip's central directory) for the unpacked size,
//   which refuses links, devices and paths that escape, and unpacked. The bundle is then checked
//   against the unpacked program with `sigstore verify identity` or `cosign verify-blob`
//   (whichever is on `PATH`; the script says which) and the pinned identity
//   `^https://github\.com/abacusai/codellm/\.github/workflows/release-cli-native\.yml@refs/tags/cli-v<version>$`
//   of GitHub Actions, so a file signed for another tag or workflow is refused. Note that a
//   signature by a private repository's workflow shows who released the file, not what it
//   contains. `requires_avx2` is written for the programs that need AVX2 (spec 096, D3).
//   `--measure-only` stops after the measurements and the signature check: it prints the entry,
//   keeps the unpacked platforms in the temporary folder (for the containment measurement by
//   hand) and never writes the catalog.
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
  createWriteStream,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { parseArgs } from 'node:util'
import { Readable, Transform } from 'node:stream'
import { pipeline } from 'node:stream/promises'

import {
  CLAUDE_DOWNLOAD_BASE,
  PLATFORM_KEYS,
  abacusAssetName,
  abacusAssetUrl,
  abacusBundleName,
  abacusEntry,
  abacusIdentityPattern,
  abacusVerifyCommand,
  catalogToWrite,
  claudeEntry,
  codexAssetName,
  codexAssetUrl,
  codexEntry,
  githubDigests,
  parseSha256Sums,
  selectPlatforms,
  summariseTarListing,
  summariseZipListing,
  type AbacusMeasured,
  type ArchiveSummary,
  type Artifact as CatalogArtifact,
  type Catalog,
  type CatalogPlatformKey,
  type CodexMeasured,
  type PlatformKey,
  type SigstoreTool,
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

const ZIP_LISTING = `import json, sys, zipfile
with zipfile.ZipFile(sys.argv[1]) as z:
    print(json.dumps([{"name": i.filename, "size": i.file_size, "mode": i.external_attr >> 16} for i in z.infolist()]))`
const ZIP_EXTRACT = `import sys, zipfile
zipfile.ZipFile(sys.argv[1]).extractall(sys.argv[2])`

function run(command: string, args: string[], what: string): string {
  const done = spawnSync(command, args, {
    encoding: 'utf8',
    maxBuffer: 1 << 26,
  })
  if (done.status !== 0) {
    throw new Error(`${what}: ${done.error?.message ?? done.stderr}`)
  }
  return done.stdout
}

/** Downloads `url` to `file`; the SHA-256 and the size are measured on the stream. */
async function downloadMeasured(url: string, file: string) {
  const response = await fetch(url)
  if (!response.ok || !response.body) {
    throw new Error(`${url}: HTTP ${response.status}`)
  }
  const hash = createHash('sha256')
  let size = 0
  const tap = new Transform({
    transform(chunk: Buffer, _encoding, done) {
      hash.update(chunk)
      size += chunk.length
      done(null, chunk)
    },
  })
  await pipeline(
    Readable.fromWeb(response.body as never),
    tap,
    createWriteStream(file),
  )
  return { sha256: hash.digest('hex'), size }
}

/** The entries of an archive: `tar -t` for a tar.gz, the central directory for a zip. */
function listArchive(zip: boolean, file: string): ArchiveSummary {
  if (!zip) return summariseTarListing(run('tar', ['-tvzf', file], 'tar -t'))
  const rows = JSON.parse(
    run('python3', ['-I', '-c', ZIP_LISTING, file], 'zip listing'),
  )
  return summariseZipListing(rows)
}

function unpackArchive(zip: boolean, file: string, into: string) {
  mkdirSync(into)
  if (zip) run('python3', ['-I', '-c', ZIP_EXTRACT, file, into], 'unzip')
  else run('tar', ['-xzf', file, '-C', into], 'tar -x')
}

/** `HAEX_SIGSTORE_BIN` names the `sigstore` program when it is not on `PATH` (a virtual environment). */
const sigstoreBinary = (tool: SigstoreTool): string =>
  tool === 'sigstore' ? (process.env.HAEX_SIGSTORE_BIN ?? tool) : tool

/** The Sigstore verifier on `PATH`: `sigstore` first, then `cosign`; none is an error. */
function sigstoreTool(): SigstoreTool {
  for (const tool of ['sigstore', 'cosign'] as const) {
    const probe = spawnSync(sigstoreBinary(tool), [
      tool === 'cosign' ? 'version' : '--version',
    ])
    if (probe.error === undefined) return tool
  }
  throw new Error(
    'neither `sigstore` (sigstore-python) nor `cosign` is on PATH; one is needed to verify the vendor bundle',
  )
}

/** Verifies the bundle of the unpacked platform against its program; `tool` says what was used. */
function verifyBundle(
  tool: SigstoreTool,
  version: string,
  folder: string,
  key: CatalogPlatformKey,
  executable: string,
): AbacusMeasured['signature'] {
  const file = join(folder, executable)
  const sha256 = createHash('sha256').update(readFileSync(file)).digest('hex')
  const [, ...args] = abacusVerifyCommand(
    tool,
    version,
    join(folder, abacusBundleName(key)),
    { sha256, file },
  )
  const done = spawnSync(sigstoreBinary(tool), args, { encoding: 'utf8' })
  if (done.status !== 0) {
    console.error(`${key}: ${tool} says:\n${done.stderr || done.stdout}`)
  }
  return { verified: done.status === 0, tool }
}

/**
 * Downloads, lists, unpacks and signature-checks each selected Abacus platform below `folder`.
 * Returns the entry and where each program was unpacked.
 */
async function abacusRelease(
  version: string,
  keys: CatalogPlatformKey[],
  executables: Partial<Record<CatalogPlatformKey, string>>,
  folder: string,
) {
  const tool = sigstoreTool()
  console.log(
    `abacusai ${version}: bundles are verified with ${tool} against ${abacusIdentityPattern(version)}`,
  )
  const measured: Partial<Record<CatalogPlatformKey, AbacusMeasured>> = {}
  const programs: Partial<Record<CatalogPlatformKey, string>> = {}
  for (const key of keys) {
    const executable = executables[key]
    if (!executable) throw new Error(`the catalog has no executable for ${key}`)
    const zip = key.startsWith('win32')
    const archive = join(folder, abacusAssetName(key))
    console.log(`abacusai ${version}: measuring ${abacusAssetName(key)} ...`)
    const { sha256, size } = await downloadMeasured(
      abacusAssetUrl(version, key),
      archive,
    )
    const summary = listArchive(zip, archive)
    if (summary.unsafe.length > 0) {
      throw new Error(
        `${key}: the archive holds entries HaexVault refuses: ${summary.unsafe.slice(0, 3).join(', ')}`,
      )
    }
    const unpacked = join(folder, key)
    unpackArchive(zip, archive, unpacked)
    const signature = verifyBundle(tool, version, unpacked, key, executable)
    console.log(
      `${key}: sha256 ${sha256}, ${size} bytes, ${summary.unpackedSize} bytes unpacked, bundle ${signature.verified ? 'verified' : 'NOT verified'} (${signature.tool})`,
    )
    measured[key] = { sha256, size, summary, signature }
    programs[key] = join(unpacked, executable)
  }
  console.log(
    "abacusai: a signature by a private repository's workflow shows who released the file, not what it contains",
  )
  return { entry: abacusEntry(version, measured, executables), programs }
}

/** What each tool is started with for the measurement: its start-up, with no login and no turn. */
const CONTAINMENT_PROBES: Record<string, string[]> = {
  claude: ['-p', 'ping'],
  codex: ['app-server', '--stdio'],
  abacusai: ['--version'],
}

/** Starts `program` under `check-containment.py`; a write outside the vault refuses the version. */
function runContainmentCheck(vendor: string, program: string) {
  const done = spawnSync(
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
  if (done.status !== 0) {
    throw new Error(
      `${vendor}: the tool writes outside the vault beyond the listed residuals; the version is not added`,
    )
  }
}

/**
 * Gate of FR-021: starts the download for this device under `strace` with the environment of the
 * base layer and refuses the version when something outside the vault is written that
 * `allowed-residuals.json` does not list.
 */
async function measureContainment(
  vendor: string,
  entry: { platforms: Partial<Record<CatalogPlatformKey, CatalogArtifact>> },
  executable: Record<string, string>,
  unpacked?: Partial<Record<CatalogPlatformKey, string>>,
) {
  const key = `${process.platform}-${process.arch === 'arm64' ? 'arm64' : 'x64'}`
  const artifact = entry.platforms[key as CatalogPlatformKey]
  if (process.platform !== 'linux' || !artifact) {
    throw new Error(
      `the containment measurement needs Linux and an entry for ${key}; measure on a Linux machine or pass --skip-containment`,
    )
  }
  console.log(`${vendor}: containment measurement with ${key} ...`)
  const measuredProgram = unpacked?.[key as CatalogPlatformKey]
  if (measuredProgram) {
    // The platform was downloaded, verified and unpacked for the entry: measure that program.
    runContainmentCheck(vendor, measuredProgram)
    return
  }
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
    runContainmentCheck(vendor, program)
  } finally {
    rmSync(directory, { recursive: true, force: true })
  }
}

async function main() {
  const { values } = parseArgs({
    options: {
      vendor: { type: 'string' },
      version: { type: 'string' },
      platform: { type: 'string', multiple: true },
      'measure-only': { type: 'boolean' },
      'skip-containment': { type: 'boolean' },
    },
  })
  const { vendor, version } = values
  const measureOnly = values['measure-only'] === true
  const usage =
    'usage: add-version.ts --vendor claude|codex --version <x.y.z> [--skip-containment]\n' +
    '       add-version.ts --vendor abacusai --version <x.y.z> --platform <key> [--platform <key> ...] [--measure-only] [--skip-containment]'
  if (
    !vendor ||
    !version ||
    !['claude', 'codex', 'abacusai'].includes(vendor) ||
    (measureOnly && vendor !== 'abacusai')
  ) {
    console.error(usage)
    process.exit(2)
  }
  let keys: CatalogPlatformKey[]
  try {
    keys = selectPlatforms(vendor, values.platform ?? [])
  } catch (error) {
    console.error(`${(error as Error).message}\n${usage}`)
    process.exit(2)
  }
  const catalog = JSON.parse(readFileSync(CATALOG, 'utf8')) as Catalog
  const definition = catalog.vendors.find((v) => v.vendor === vendor)
  if (!definition) throw new Error(`the catalog has no vendor ${vendor}`)
  const executables = definition.executable as Record<string, string>

  // Abacus is measured in an operator-only folder (mode 0700) that `--measure-only` keeps.
  const folder =
    vendor === 'abacusai'
      ? mkdtempSync(join(tmpdir(), 'haex-abacus-'))
      : undefined
  let keep = false
  try {
    let unpacked: Partial<Record<CatalogPlatformKey, string>> | undefined
    let entry
    if (vendor === 'claude') {
      entry = claudeEntry(version, (await claudeManifest(version)) as never)
    } else if (vendor === 'codex') {
      entry = await codexRelease(
        version,
        definition.executable as Record<PlatformKey, string>,
      )
    } else {
      const release = await abacusRelease(version, keys, executables, folder!)
      entry = release.entry
      unpacked = release.programs
    }

    // Checked before the slow containment run; `null` is the measure-only run.
    const next = catalogToWrite(catalog, vendor, entry, measureOnly)
    if (next === null) {
      keep = true
      console.log(JSON.stringify(entry, null, 2))
      console.log(
        `measure-only: the catalog is not written; the unpacked platforms are in ${folder}`,
      )
      console.log(
        `containment by hand: python3 -I scripts/cli-tools/check-containment.py --vendor ${vendor} --program <program below> -- ${CONTAINMENT_PROBES[vendor]!.join(' ')}`,
      )
      for (const [key, program] of Object.entries(unpacked ?? {})) {
        console.log(`  ${key}: ${program}`)
      }
      return
    }

    if (values['skip-containment']) {
      console.warn(
        `${vendor} ${version}: containment NOT measured (--skip-containment); measure it on Linux before this entry ships`,
      )
    } else {
      await measureContainment(vendor, entry, executables, unpacked)
    }

    writeFileSync(CATALOG, `${JSON.stringify(next, null, 2)}\n`)
    spawnSync('pnpm', ['exec', 'prettier', '--write', CATALOG.pathname], {
      stdio: 'inherit',
    })
    console.log(`${vendor} ${version} added to the catalog`)
  } finally {
    if (folder && !keep) rmSync(folder, { recursive: true, force: true })
  }
}

await main()
