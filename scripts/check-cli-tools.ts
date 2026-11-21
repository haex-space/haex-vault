// `pnpm check:cli-tools` (spec 094): what the Settings page and the chat say about the per-vault
// CLI tools. The pure module is loaded with Node type stripping; the composable, the row and the
// language files are read as text, because they need Vue and Tauri to run.
import assert from 'node:assert/strict'
import { runInNewContext } from 'node:vm'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'
import ts from 'typescript'

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
