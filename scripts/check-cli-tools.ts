// `pnpm check:cli-tools` (spec 094): what the Settings page and the chat say about the per-vault
// CLI tools. The pure module is loaded with Node type stripping; the composable, the row and the
// language files are read as text, because they need Vue and Tauri to run.
import assert from 'node:assert/strict'
import { runInNewContext } from 'node:vm'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'
import ts from 'typescript'

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
  stateKey,
  unavailableTool,
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

test('every state and phase has words in English and German', () => {
  const keys = [
    stateKey({ state: 'not_installed', reason: null }),
    stateKey({ state: 'installed', reason: null }),
    stateKey({ state: 'installing', reason: null }),
    stateKey({ state: 'needs_reinstall', reason: 'platform' }),
    stateKey({ state: 'needs_reinstall', reason: 'incomplete' }),
    stateKey({ state: 'unsupported', reason: null }),
    'settings.cliTools.state.installedVersion',
    isolationKey({ isolation: 'best_effort' }),
    isolationKey({ isolation: 'enforced' }),
    ...(['downloading', 'verifying', 'unpacking', 'publishing'] as const).map(
      (phase) => phaseKey({ phase }),
    ),
    'settings.cliTools.title',
    'settings.cliTools.install',
    'settings.cliTools.cancel',
    'settings.cliTools.size',
    'settings.cliTools.connectNeedsTool',
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
    'io_error',
    'failed',
    'notInstalled',
    'needsReinstall',
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
  const row = read('src/components/settings/CliToolRow.vue')
  assert.match(row, /settings\.cliTools\.size/)
  assert.match(row, /settings\.cliTools\.install/)
  assert.match(row, /settings\.cliTools\.cancel/)
  assert.ok(!/toast\.success/.test(row))
})

test('the row says how far the tool is kept in the vault, and the two levels differ', () => {
  const row = read('src/components/settings/CliToolRow.vue')
  assert.match(row, /isolationKey\(status\)/)
  assert.notEqual(
    isolationKey({ isolation: 'best_effort' }),
    isolationKey({ isolation: 'enforced' }),
  )
})

test('the providers page lists the tool row before each connect row', () => {
  const page = read('src/components/settings/ConnectDelegateProvider.vue')
  assert.ok(
    page.indexOf('<CliToolRow') < page.indexOf(':title="vendorLabel(vendor)"'),
  )
  assert.match(page, /toolReady\(vendor\)/)
})
