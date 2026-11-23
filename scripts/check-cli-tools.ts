// `pnpm check:cli-tools` (spec 094): what the Settings page and the chat say about the per-vault
// CLI tools. The pure module is loaded with Node type stripping; the composable, the row and the
// language files are read as text, because they need Vue and Tauri to run.
import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { runInNewContext } from 'node:vm'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'
import { fileURLToPath } from 'node:url'
import ts from 'typescript'

import {
  ABACUS_OIDC_ISSUER,
  PLATFORM_KEYS,
  abacusAssetUrl,
  abacusEntry,
  abacusIdentity,
  abacusIdentityPattern,
  abacusVerifyCommand,
  catalogToWrite,
  selectPlatforms,
  summariseTarListing,
  summariseZipListing,
  type AbacusMeasured,
  type Catalog,
  type CatalogPlatformKey,
} from './cli-tools/lib/entry.ts'

import { downloadSpeed, formatSpeed } from '../src/lib/transferSpeed.ts'
import {
  cliToolErrorCode,
  formatBytes,
  installable,
  installing,
  isCancelledInstall,
  isolationKey,
  isNotInstalledEntry,
  phaseKey,
  progressFraction,
  revertible,
  stateKey,
  unavailableTool,
  updatable,
  usable,
} from '../src/lib/cli-tools.ts'

const read = (path: string) =>
  readFileSync(new URL(`../${path}`, import.meta.url), 'utf8')

const locales = {
  en: JSON.parse(read('src/i18n/locales/en.json')),
  de: JSON.parse(read('src/i18n/locales/de.json')),
}

const lookup = (messages: unknown, key: string): unknown =>
  key
    .split('.')
    .reduce<unknown>(
      (node, part) => (node as Record<string, unknown> | undefined)?.[part],
      messages,
    )

test('the install is offered in exactly two states', () => {
  const states = [
    'not_installed',
    'installed',
    'update_available',
    'installing',
    'needs_reinstall',
    'unsupported',
  ] as const
  assert.deepEqual(
    states.filter((state) => installable({ state })),
    ['not_installed', 'needs_reinstall'],
  )
  assert.deepEqual(
    states.filter((state) => installing({ state })),
    ['installing'],
  )
})

test('progress is a fraction of the download, and none without a size or outside the download', () => {
  assert.equal(
    progressFraction({ phase: 'downloading', bytesDone: 50, bytesTotal: 200 }),
    0.25,
  )
  assert.equal(
    progressFraction({ phase: 'downloading', bytesDone: 500, bytesTotal: 200 }),
    1,
  )
  assert.equal(
    progressFraction({ phase: 'downloading', bytesDone: 5, bytesTotal: 0 }),
    null,
  )
  assert.equal(
    progressFraction({ phase: 'unpacking', bytesDone: 0, bytesTotal: 0 }),
    null,
  )
  assert.equal(progressFraction(undefined), null)
})

test('sizes are written in decimal units', () => {
  assert.equal(formatBytes(293_776_752), '294 MB')
  assert.equal(formatBytes(497_056_070), '497 MB')
  assert.equal(formatBytes(1_200_000_000), '1.2 GB')
})

test('the speed is the average of the last seconds, and none without enough of them', () => {
  const at = 100_000
  assert.equal(downloadSpeed([], at), null)
  assert.equal(downloadSpeed([{ at, bytes: 10 }], at), null)
  // 300 ms apart is too short to say anything.
  assert.equal(
    downloadSpeed(
      [
        { at: at - 300, bytes: 0 },
        { at, bytes: 3_000_000 },
      ],
      at,
    ),
    null,
  )
  assert.equal(
    downloadSpeed(
      [
        { at: at - 2000, bytes: 1_000_000 },
        { at: at - 1000, bytes: 5_000_000 },
        { at, bytes: 9_000_000 },
      ],
      at,
    ),
    4_000_000,
  )
  // Samples older than the window do not count, so a stalled download shows no speed.
  assert.equal(
    downloadSpeed(
      [
        { at: at - 9000, bytes: 0 },
        { at: at - 8000, bytes: 8_000_000 },
      ],
      at,
    ),
    null,
  )
})

test('a speed is written in MB/s, or kB/s below one', () => {
  assert.equal(formatSpeed(8_340_000), '8.3 MB/s')
  assert.equal(formatSpeed(640_000), '640 kB/s')
})

test('every state and phase has words in English and German', () => {
  const keys = [
    stateKey({ state: 'not_installed', reason: null }),
    stateKey({ state: 'installed', reason: null }),
    stateKey({ state: 'update_available', reason: null }),
    stateKey({ state: 'installing', reason: null }),
    stateKey({ state: 'needs_reinstall', reason: 'platform' }),
    stateKey({ state: 'needs_reinstall', reason: 'incomplete' }),
    stateKey({ state: 'unsupported', reason: null }),
    'settings.cliTools.state.installedVersion',
    'settings.cliTools.downloadProgress',
    isolationKey({ isolation: 'best_effort' }),
    isolationKey({ isolation: 'enforced' }),
    ...(['downloading', 'verifying', 'unpacking', 'publishing'] as const).map(
      (phase) => phaseKey({ phase }),
    ),
    'settings.cliTools.title',
    'settings.cliTools.install',
    'settings.cliTools.update',
    'settings.cliTools.revert',
    'settings.cliTools.updateAvailable',
    'settings.cliTools.unconfirmed',
    'settings.cliTools.untested',
    'settings.cliTools.outdated',
    'settings.cliTools.cancel',
    'settings.cliTools.size',
    'chat.model.delegateNotInstalled',
  ]
  for (const [name, messages] of Object.entries(locales)) {
    for (const key of keys) {
      assert.equal(typeof lookup(messages, key), 'string', `${name}: ${key}`)
    }
  }
})

test('every error code of the tool manager has a message in both languages', () => {
  const codes = [
    'unknown_vendor',
    'unknown_version',
    'unsupported_platform',
    'locked',
    'no_space',
    'offline',
    'server_error',
    'integrity_mismatch',
    'unpack_refused',
    'smoke_failed',
    'cancelled',
    'unconfirmed_update',
    'version_in_use',
    'nothing_to_revert',
    'in_use',
    'io_error',
    'failed',
    'notInstalled',
    'needsReinstall',
    'notRunnable',
  ]
  for (const [name, messages] of Object.entries(locales)) {
    for (const code of codes) {
      assert.equal(
        typeof lookup(messages, `errors.cliTools.${code}`),
        'string',
        `${name}: errors.cliTools.${code}`,
      )
    }
  }
})

test('the codes the backend sends are found in an error text', () => {
  assert.equal(
    cliToolErrorCode(
      'cli_tools.integrity_mismatch: the download does not match the listed checksum',
    ),
    'cli_tools.integrity_mismatch',
  )
  assert.equal(
    cliToolErrorCode('cli_tools.no_space: 100 bytes needed, 5 free'),
    'cli_tools.no_space',
  )
  assert.equal(cliToolErrorCode('some other failure'), null)
})

test('a delegate that cannot start names the tool the vault lacks', () => {
  assert.deepEqual(
    unavailableTool('backend unavailable: not installed in this vault: claude'),
    { kind: 'not_installed', vendor: 'claude' },
  )
  assert.deepEqual(unavailableTool('adapter start: needs reinstall: codex'), {
    kind: 'needs_reinstall',
    vendor: 'codex',
  })
  assert.deepEqual(
    unavailableTool('adapter start: not runnable: codex: permission denied'),
    { kind: 'not_runnable', vendor: 'codex' },
  )
  assert.equal(unavailableTool('provider rejected credentials'), null)
})

test("only the user's own cancel of an install is not reported", () => {
  assert.equal(
    isCancelledInstall({
      kind: 'InvalidInput',
      reason: 'cli_tools.cancelled: the install was cancelled',
    }),
    true,
  )
  assert.equal(isCancelledInstall('cli_tools.cancelled: x'), true)
  assert.equal(
    isCancelledInstall({
      kind: 'InvalidInput',
      reason: 'cli_tools.offline: x',
    }),
    false,
  )
  assert.equal(isCancelledInstall(null), false)
  assert.equal(isCancelledInstall(new Error('x')), false)
})

test('only the placeholder of a delegate without its tool opens the install', () => {
  assert.equal(isNotInstalledEntry('delegate-claude:not-installed'), true)
  assert.equal(isNotInstalledEntry('delegate-claude:not-connected'), false)
  assert.equal(isNotInstalledEntry('claude-opus-5'), false)
  assert.equal(
    isNotInstalledEntry('3f0c0f2e-0c0e-4d0a-9f55-0c1e9c7d9a11:not-installed'),
    false,
  )
})

test('the composable hides cancellation and gives other install errors a fixed toast id', async () => {
  const calls: Array<{ method: string; message: string; options: unknown }> = []
  let installError: unknown = 'cli_tools.cancelled: user cancelled'
  const exports = {}
  const compiled = ts.transpileModule(read('src/composables/useCliTools.ts'), {
    compilerOptions: {
      module: ts.ModuleKind.CommonJS,
      target: ts.ScriptTarget.ES2022,
    },
  }).outputText
  const mocks = {
    '@tauri-apps/api/core': {
      invoke: async (command: string) => {
        if (command === 'cli_tools_install') throw installError
        if (command === 'cli_tools_status') return []
        return undefined
      },
    },
    '@tauri-apps/api/event': { listen: async () => () => undefined },
    'vue-sonner': {
      toast: {
        error: (message: string, options: unknown) =>
          calls.push({ method: 'error', message, options }),
      },
    },
    '~/lib/cli-tools': {
      isCancelledInstall: (error: unknown) =>
        typeof error === 'string' && error.startsWith('cli_tools.cancelled'),
    },
  }
  runInNewContext(compiled, {
    exports,
    require: (id: string) => mocks[id as keyof typeof mocks],
    ref: (value: unknown) => ({ value }),
    reactive: (value: unknown) => value,
    useErrorString: () => ({ errString: String }),
  })
  const useCliTools = (
    exports as {
      useCliTools: () => { installAsync: (vendor: string) => Promise<void> }
    }
  ).useCliTools
  const tools = useCliTools()

  await tools.installAsync('claude')
  assert.equal(calls.length, 0, 'cancellation does not show an error toast')

  installError = 'cli_tools.offline: release host unavailable'
  await tools.installAsync('claude')
  assert.equal(calls.length, 1)
  assert.equal(calls[0]?.message, installError)
  assert.equal(
    (calls[0]?.options as { id?: string } | undefined)?.id,
    'cli-tool-progress',
  )

  // The UI never names a version: the newest listed one is installed.
  const source = read('src/composables/useCliTools.ts')
  assert.ok(!/version/.test(source.replace(/\/\*[\s\S]*?\*\//g, '')))
})

test('the row offers the install with its size and has no inline success text', () => {
  const row = read('src/components/settings/DelegateRow.vue')
  assert.match(row, /settings\.cliTools\.size/)
  assert.match(row, /settings\.cliTools\.install/)
  assert.match(row, /settings\.cliTools\.cancel/)
  assert.ok(!/toast\.success/.test(row))
})

const tool = (
  state: Parameters<typeof usable>[0]['state'],
  previousVersion: string | null = null,
  installedVersion: string | null = '1.0.0',
) => ({ state, previousVersion, installedVersion })

test('a tool with a version in use counts as usable through an update on offer and one running', () => {
  assert.equal(usable(tool('installed')), true)
  assert.equal(usable(tool('update_available')), true)
  assert.equal(usable(tool('installing')), true, 'an update keeps the old one')
  assert.equal(usable(tool('installing', null, null)), false, 'a first install')
  assert.equal(usable(tool('not_installed', null, null)), false)
  assert.equal(usable(tool('needs_reinstall', null, null)), false)
  assert.equal(usable(tool('unsupported', null, null)), false)
})

test('the update is offered only when no update still waits for its first finished run', () => {
  assert.equal(updatable(tool('update_available')), true)
  assert.equal(updatable(tool('update_available', '0.9.0')), false)
  assert.equal(updatable(tool('installed')), false)
  assert.equal(updatable(tool('installing')), false)
})

test('going back is offered exactly while there is a version to go back to', () => {
  assert.equal(revertible(tool('installed', '0.9.0')), true)
  assert.equal(revertible(tool('update_available', '0.9.0')), true)
  assert.equal(revertible(tool('installed')), false)
  assert.equal(
    revertible(tool('installing', '0.9.0')),
    false,
    'not while installing',
  )
  assert.equal(
    revertible(tool('update_available')),
    false,
    'an update on offer alone has nothing to go back to',
  )
})

test('the update, the way back and the notes are in the row and the composable, and nothing is silent', () => {
  const row = read('src/components/settings/DelegateRow.vue')
  assert.match(row, /updatable\(status\)/)
  assert.match(row, /revertible\(status\)/)
  assert.match(row, /settings\.cliTools\.update/)
  assert.match(row, /settings\.cliTools\.revert/)
  assert.match(row, /emit\('revert'\)/)
  assert.ok(!/toast\.success/.test(row))
  const composable = read('src/composables/useCliTools.ts')
  assert.match(composable, /'cli_tools_revert'/)
  // A change is a click: no timer, no interval, no update by itself.
  assert.ok(!/setInterval|setTimeout/.test(composable))
})

test('the chat picker and the connect flow use the same readiness as the row', () => {
  const page = read('src/components/settings/ConnectDelegateProvider.vue')
  const row = read('src/components/settings/DelegateRow.vue')
  assert.match(row, /usable\(status\)/)
  assert.match(page, /@revert="cliTools\.revertAsync\(vendor\)"/)
  const store = read('src/stores/models.ts')
  assert.match(store, /usable\(tool\)/)
  assert.ok(!/state === 'installed'/.test(store))
})

test('the row says how far the tool is kept in the vault, and the two levels differ', () => {
  const row = read('src/components/settings/DelegateRow.vue')
  assert.match(row, /isolationKey\(status\)/)
  assert.notEqual(
    isolationKey({ isolation: 'best_effort' }),
    isolationKey({ isolation: 'enforced' }),
  )
})

test('a provider is one row with the program and its sign-in together', () => {
  const page = read('src/components/settings/ConnectDelegateProvider.vue')
  const row = read('src/components/settings/DelegateRow.vue')
  // One row per vendor: no second row for the connect, and the sign-in does not grow the row.
  assert.equal(page.match(/<SettingsDelegateRow/g)?.length, 1)
  assert.ok(!/<SettingsRow/.test(page))
  assert.ok(!/#below/.test(row))
  assert.match(page, /<SettingsDelegateSignInDialog/)
  // Without the program the sign-in and the model refresh are not offered, and the way out is.
  // (a stored delegate whose program status is not known yet follows its connection instead)
  assert.match(row, /v-if="status \? usable\(status\) : connected"/)
  assert.match(row, /cli-tool-install-/)
  assert.match(row, /cli-tool-remove-/)
  // What else there is to know sits behind the info button.
  assert.match(row, /cli-tool-info-/)
  assert.match(row, /UiDrawerModal/)
})

test('removing a provider takes the program and the connection, and only the user can ask', () => {
  const actions = read('src/lib/actions/settingsActions.ts')
  assert.match(
    actions,
    /id: 'settings\.delegate\.remove'[\s\S]*?scope: 'guardrails'/,
  )
  const handlers = read('src/stores/settingsActionHandlers.ts')
  assert.match(handlers, /'cli_tools_remove'/)
  assert.match(handlers, /providers\.deleteAsync\(connected\.id\)/)
})

// ---- scripts/cli-tools/add-version.ts, Abacus AI (spec 096 T012) ---------------------------

const HASH = (n: number) => n.toString(16).padStart(64, '0')

test('the signing identity accepts the tag of its version and no other', () => {
  const pattern = new RegExp(abacusIdentityPattern('2.10.23'))
  const identity = (tag: string, workflow = 'release-cli-native.yml') =>
    `https://github.com/abacusai/codellm/.github/workflows/${workflow}@refs/tags/${tag}`
  assert.ok(pattern.test(identity('cli-v2.10.23')))
  assert.equal(abacusIdentity('2.10.23'), identity('cli-v2.10.23'))
  assert.ok(!pattern.test(identity('cli-v2.10.230')))
  assert.ok(!pattern.test(identity('cli-v12.10.23')))
  assert.ok(!pattern.test(identity('cli-v2.10.2')))
  assert.ok(!pattern.test(identity('cli-v2x10x23')), 'the dots are literal')
  assert.ok(!pattern.test(identity('cli-v2.10.24')))
  assert.ok(!pattern.test(identity('cli-v2.10.23', 'other.yml')))
  assert.ok(!pattern.test(`${identity('cli-v2.10.23')}/extra`))
  assert.ok(!pattern.test(identity('v2.10.23')))
  assert.throws(() => abacusIdentityPattern('2.10.23|.*'), /semver/)
  assert.equal(
    abacusIdentityPattern('2.10.23'),
    '^https://github\\.com/abacusai/codellm/\\.github/workflows/release-cli-native\\.yml@refs/tags/cli-v2\\.10\\.23$',
  )
})

test('the verify command pins identity and issuer for sigstore and for cosign', () => {
  const subject = { sha256: HASH(7), file: '/tmp/x/abacusai' }
  assert.deepEqual(
    abacusVerifyCommand('sigstore', '2.10.23', 'b.sigstore', subject),
    [
      'sigstore',
      'verify',
      'identity',
      '--bundle',
      'b.sigstore',
      '--cert-identity',
      abacusIdentity('2.10.23'),
      '--cert-oidc-issuer',
      'https://token.actions.githubusercontent.com',
      `sha256:${HASH(7)}`,
    ],
  )
  const cosign = abacusVerifyCommand('cosign', '2.10.23', 'b.sigstore', subject)
  assert.deepEqual(cosign.slice(0, 2), ['cosign', 'verify-blob'])
  assert.equal(
    cosign[cosign.indexOf('--certificate-identity-regexp') + 1],
    abacusIdentityPattern('2.10.23'),
  )
  assert.equal(
    cosign[cosign.indexOf('--certificate-oidc-issuer') + 1],
    ABACUS_OIDC_ISSUER,
  )
  assert.equal(cosign.at(-1), subject.file)
})

test('Abacus needs a platform selector and processes only the selected keys; the others keep all six', () => {
  assert.deepEqual(selectPlatforms('abacusai', ['win32-x64', 'linux-x64']), [
    'linux-x64',
    'win32-x64',
  ])
  assert.deepEqual(selectPlatforms('abacusai', ['linux-x64', 'linux-x64']), [
    'linux-x64',
  ])
  assert.deepEqual(selectPlatforms('abacusai', ['linux-x64-baseline']), [
    'linux-x64-baseline',
  ])
  assert.throws(
    () => selectPlatforms('abacusai', []),
    /at least one --platform/,
  )
  assert.throws(
    () => selectPlatforms('abacusai', ['linux-x64-musl']),
    /no such abacusai platform: linux-x64-musl/,
  )
  assert.deepEqual(selectPlatforms('claude', []), [...PLATFORM_KEYS])
  assert.deepEqual(selectPlatforms('codex', []), [...PLATFORM_KEYS])
  assert.throws(
    () => selectPlatforms('claude', ['linux-x64']),
    /only for abacusai/,
  )
})

const tarSummary = (program: string, bundle: string, size = 1000) =>
  summariseTarListing(
    [
      'drwxr-xr-x runner/runner 0 2026-11-20 15:18 ./',
      `-rwxr-xr-x runner/runner ${size} 2026-11-20 15:18 ./${program}`,
      `-rw------- runner/runner 11 2026-11-20 15:18 ./${bundle}`,
    ].join('\n'),
  )

const zipSummary = (program: string, bundle: string) =>
  summariseZipListing([
    { name: program, size: 2000, mode: 0 },
    { name: bundle, size: 11, mode: 0o100644 },
  ])

const measuredFor = (
  key: CatalogPlatformKey,
  index: number,
  verified = true,
): AbacusMeasured => ({
  sha256: HASH(index + 1),
  size: 500 + index,
  summary: key.startsWith('win32')
    ? zipSummary('abacusai.exe', `abacusai-${key}.sigstore`)
    : tarSummary('abacusai', `abacusai-${key}.sigstore`),
  signature: { verified, tool: 'sigstore' },
})

const ABACUS_EXECUTABLES: Partial<Record<CatalogPlatformKey, string>> = {
  'linux-x64': 'abacusai',
  'linux-x64-baseline': 'abacusai',
  'linux-arm64': 'abacusai',
  'darwin-x64': 'abacusai',
  'darwin-arm64': 'abacusai',
  'win32-x64': 'abacusai.exe',
  'win32-arm64': 'abacusai.exe',
}

test('an Abacus entry lists only measured keys, with zip for Windows and requires_avx2 where AVX2 is needed', () => {
  const keys: CatalogPlatformKey[] = [
    'linux-x64',
    'linux-x64-baseline',
    'linux-arm64',
    'darwin-x64',
    'darwin-arm64',
    'win32-x64',
    'win32-arm64',
  ]
  const entry = abacusEntry(
    '2.10.23',
    Object.fromEntries(keys.map((key, i) => [key, measuredFor(key, i)])),
    ABACUS_EXECUTABLES,
  )
  assert.deepEqual(Object.keys(entry.platforms), keys)
  assert.deepEqual(entry.platforms['linux-x64'], {
    url: 'https://static.abacus.ai/cli/releases/2.10.23/abacusai-linux-x64.tar.gz',
    sha256: HASH(1),
    size: 500,
    kind: 'tar.gz',
    unpacked_size: 1011,
    requires_avx2: true,
  })
  assert.equal(
    entry.platforms['linux-x64-baseline']?.url,
    abacusAssetUrl('2.10.23', 'linux-x64-baseline'),
  )
  assert.equal(entry.platforms['win32-arm64']?.kind, 'zip')
  assert.equal(entry.platforms['win32-arm64']?.unpacked_size, 2011)
  assert.equal(
    entry.platforms['win32-arm64']?.url,
    'https://static.abacus.ai/cli/releases/2.10.23/abacusai-win32-arm64.zip',
  )
  const needsAvx2 = Object.entries(entry.platforms)
    .filter(([, artifact]) => artifact?.requires_avx2 === true)
    .map(([key]) => key)
  assert.deepEqual(needsAvx2, ['linux-x64', 'darwin-x64', 'win32-x64'])
  assert.ok(
    !('requires_avx2' in entry.platforms['linux-arm64']!),
    'false is not written',
  )

  const only = abacusEntry(
    '2.10.23',
    { 'darwin-arm64': measuredFor('darwin-arm64', 4) },
    ABACUS_EXECUTABLES,
  )
  assert.deepEqual(Object.keys(only.platforms), ['darwin-arm64'])
  assert.throws(
    () => abacusEntry('2.10.23', {}, ABACUS_EXECUTABLES),
    /no platform/,
  )
})

test('an Abacus entry is refused for a bundle that did not verify, unsafe entries and a missing program or bundle', () => {
  const entry = (item: AbacusMeasured) =>
    abacusEntry('2.10.23', { 'linux-x64': item }, ABACUS_EXECUTABLES)
  assert.throws(
    () => entry(measuredFor('linux-x64', 0, false)),
    /did not verify.*who released the file, not what it contains/,
  )
  const good = measuredFor('linux-x64', 0)
  assert.throws(
    () =>
      entry({
        ...good,
        summary: summariseTarListing(
          'lrwxrwxrwx runner/runner 0 2026-11-20 15:18 ./abacusai -> /bin/sh',
        ),
      }),
    /refuses/,
  )
  assert.throws(
    () =>
      entry({
        ...good,
        summary: tarSummary('other', 'abacusai-linux-x64.sigstore'),
      }),
    /does not hold abacusai/,
  )
  assert.throws(
    () => entry({ ...good, summary: tarSummary('abacusai', 'other.sigstore') }),
    /does not hold its Sigstore bundle/,
  )
  assert.throws(() => entry({ ...good, sha256: 'ABC' }), /SHA-256/)
  assert.throws(
    () => abacusEntry('2.10.23|x', { 'linux-x64': good }, ABACUS_EXECUTABLES),
    /semver/,
  )
})

test('the archive listing of the real linux-x64 release adds up to the unpacked size of its entry', () => {
  const summary = summariseTarListing(
    [
      'drwxr-xr-x runner/runner 0 2026-11-20 15:18 ./',
      '-rwxr-xr-x runner/runner 157563056 2026-11-20 15:18 ./abacusai',
      '-rw------- runner/runner 10411 2026-11-20 15:18 ./abacusai-linux-x64.sigstore',
    ].join('\n'),
  )
  assert.equal(summary.unpackedSize, 157_573_467)
  assert.deepEqual(summary.unsafe, [])
})

test('a zip listing gives the unpacked size and flags links, devices and escaping paths', () => {
  assert.deepEqual(
    summariseZipListing([
      { name: 'bin/', size: 0, mode: 0o040755 },
      { name: 'abacusai.exe', size: 2000, mode: 0 },
      { name: 'notes.txt', size: 24, mode: 0o100644 },
    ]),
    {
      unpackedSize: 2024,
      paths: ['bin/', 'abacusai.exe', 'notes.txt'],
      unsafe: [],
    },
  )
  const bad = summariseZipListing([
    { name: 'link', size: 5, mode: 0o120777 },
    { name: 'dev', size: 0, mode: 0o020666 },
    { name: '../escape', size: 1, mode: 0o100644 },
    { name: '/absolute', size: 1, mode: 0o100644 },
    { name: 'a\\..\\b', size: 1, mode: 0 },
    { name: 'C:/drive', size: 1, mode: 0 },
  ])
  assert.equal(bad.unsafe.length, 6)
})

test('measure-only never gives a catalog to write; a run that writes is checked first', () => {
  const catalog: Catalog = {
    schema: 1,
    vendors: [{ vendor: 'abacusai', versions: [] }],
  }
  const before = structuredClone(catalog)
  const entry = abacusEntry(
    '2.10.23',
    { 'linux-x64': measuredFor('linux-x64', 0) },
    ABACUS_EXECUTABLES,
  )
  assert.equal(catalogToWrite(catalog, 'abacusai', entry, true), null)
  assert.deepEqual(catalog, before, 'the catalog is untouched')
  const next = catalogToWrite(catalog, 'abacusai', entry, false)
  assert.deepEqual(
    next?.vendors[0]?.versions.map((v) => v.version),
    ['2.10.23'],
  )
  assert.deepEqual(catalog, before, 'the input is not changed either way')
  assert.throws(
    () =>
      catalogToWrite(
        catalog,
        'abacusai',
        {
          version: '2.10.23',
          platforms: {
            'win32-x64': {
              url: 'https://static.abacus.ai/x.zip',
              sha256: HASH(1),
              size: 1,
              kind: 'zip',
            },
          },
        },
        false,
      ),
    /unpacked_size/,
  )
})

const FIXTURE = new URL('./cli-tools/fixtures/', import.meta.url)
const SIGSTORE_BIN = process.env.HAEX_SIGSTORE_BIN ?? 'sigstore'
const sigstoreMissing =
  spawnSync(SIGSTORE_BIN, ['--version']).error === undefined
    ? false
    : 'sigstore is not on PATH and HAEX_SIGSTORE_BIN is not set: the signed fixture is not verified'

const verifyFixture = (sha256: string, version = '2.10.23') => {
  const [, ...args] = abacusVerifyCommand(
    'sigstore',
    version,
    fileURLToPath(new URL('abacusai-2.10.23-linux-x64.sigstore', FIXTURE)),
    { sha256, file: '' },
  )
  return spawnSync(SIGSTORE_BIN, args, { encoding: 'utf8' })
}

test(
  'the signed fixture verifies against the pinned identity and a changed digest or tag fails',
  { skip: sigstoreMissing },
  () => {
    const sha256 = readFileSync(
      new URL('abacusai-2.10.23-linux-x64.sha256', FIXTURE),
      'utf8',
    ).trim()
    assert.equal(
      sha256,
      '108c4ba6e4076fc911959169bb7b94e64106af89ff83c3a8e32a050f91591d56',
    )
    assert.equal(verifyFixture(sha256).status, 0)
    assert.notEqual(verifyFixture(HASH(1)).status, 0, 'another digest')
    assert.notEqual(verifyFixture(sha256, '2.10.24').status, 0, 'another tag')
  },
)
