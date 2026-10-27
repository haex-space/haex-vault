// Run with `node scripts/check-vault-lifecycle.ts`. Frontend cases for spec 013 (vault lifecycle
// isolation, specs/013-vault-lifecycle-isolation): the lock flows, the passphrase lifetime in the
// unlock and create sheets, and the reads that must never fail. They run on the shared replay
// harness in `scripts/lib/chat-state-harness.ts`. New frontend cases for this feature go here,
// never into the oversized `scripts/check-chat-state.ts`.
import assert from 'node:assert/strict'
import { test } from 'node:test'
import {
  composablePath,
  createChatState,
  runComposable,
} from './lib/chat-state-harness.ts'
import { loadScriptSetup } from './lib/script-setup-sandbox.ts'

test('the shared harness boots the chat page against the IPC double', () => {
  const state = createChatState()
  assert.equal(state.busy.value, false)
  assert.equal(state.input.value, '')
})

// The lock flows (US1, FR-002): the lock control only asks the backend to close. The backend
// replaces the page with a spinner and ends the process, so `useVaultLock` keeps no closing state,
// navigates nowhere and does not clear the active instance itself. It provides neither
// `navigateTo` nor the instances store, so touching them fails the case.

/** Runs the real `useVaultLock` against doubles that record what it does, in order. */
function loadVaultLock(closeAsync: () => Promise<void>) {
  const calls: string[] = []
  const composable = runComposable(
    composablePath('useVaultLock'),
    (specifier) => {
      throw new Error(`useVaultLock sandbox: unexpected import '${specifier}'`)
    },
    {
      useWindowManagerStore: () => ({
        flushAsync: async () => {
          calls.push('flush')
        },
      }),
      useInstance: () => ({
        closeAsync: async () => {
          calls.push('close')
          await closeAsync()
        },
      }),
    },
  ) as { useVaultLock: () => { lock: () => Promise<void> } }
  return { calls, ...composable.useVaultLock() }
}

test('useVaultLock flushes the layout, then asks for the close once and does nothing else', async () => {
  const { calls, lock } = loadVaultLock(async () => {})

  await lock()

  assert.deepEqual(calls, ['flush', 'close'])
})

test('useVaultLock swallows a rejected close and shows no error', async () => {
  const { calls, lock } = loadVaultLock(async () => {
    throw new Error('the page is already gone')
  })

  await lock()

  assert.deepEqual(calls, ['flush', 'close'])
})

// US4 frontend errors (T066/T074, contracts/frontend-surface.md): `useErrorString` maps the three
// new fieldless kinds to their own `errors.*` keys instead of falling through to a raw
// `JSON.stringify(e)`.

function loadErrString(): { errString: (e: unknown) => string } {
  const req = (specifier: string) => {
    throw new Error(`useErrorString sandbox: unexpected import '${specifier}'`)
  }
  const mod = runComposable(composablePath('useErrorString'), req, {
    hfErrorKey: () => 'errors.hf.generic',
  }) as { useErrorString: () => { errString: (e: unknown) => string } }
  return mod.useErrorString()
}

test('useErrorString maps VaultAlreadyOpenElsewhere, VaultAlreadyActive and VaultClosed to their errors.* keys', () => {
  const { errString } = loadErrString()
  assert.equal(
    errString({ kind: 'VaultAlreadyOpenElsewhere' }),
    'errors.vaultAlreadyOpenElsewhere',
  )
  assert.equal(
    errString({ kind: 'VaultAlreadyActive' }),
    'errors.vaultAlreadyActive',
  )
  assert.equal(errString({ kind: 'VaultClosed' }), 'errors.vaultClosed')
})

// UnlockSheet: every open failure keeps its typed discriminator at the UI boundary, so the operator
// can distinguish a bad passphrase, a missing vault and an unreadable/incompatible vault file.

function loadUnlockSheet(openAsync: () => Promise<{ name: string }>) {
  return loadScriptSetup<{
    onSubmit: () => Promise<void>
    error: { value: string | null }
    passphrase: { value: string }
  }>(
    'src/components/onboarding/UnlockSheet.vue',
    ['onSubmit', 'error', 'passphrase'],
    {
      useInstance: () => ({ openAsync }),
      useFieldLabels: () => ({ password: { value: {} } }),
      props: { open: true, name: 'vault' },
    },
  )
}

for (const [kind, expected] of [
  ['VaultAlreadyOpenElsewhere', 'errors.vaultAlreadyOpenElsewhere'],
  ['WrongPassphrase', 'errors.wrongPassphrase'],
  ['NotFound', 'errors.vaultNotFound'],
  ['CrdtSqlite', 'errors.vaultUnreadable'],
  ['NotAValidInstance', 'errors.vaultUnreadable'],
  ['PathResolution', 'errors.vaultIo'],
  ['Io', 'errors.vaultIo'],
  ['CrdtInit', 'errors.vaultIncompatible'],
  ['MigrationMissingFromSource', 'errors.vaultNewerVersion'],
] as const) {
  test(`UnlockSheet maps ${kind} to a distinct open error`, async () => {
    const { onSubmit, error, passphrase } = loadUnlockSheet(() => {
      throw { kind }
    })
    passphrase.value = 'some-passphrase'

    await onSubmit()

    assert.equal(error.value, expected)
  })
}

test('UnlockSheet maps a JSON-serialized backend error to its distinct open error', async () => {
  const { onSubmit, error, passphrase } = loadUnlockSheet(() => {
    throw JSON.stringify({ kind: 'WrongPassphrase' })
  })
  passphrase.value = 'some-passphrase'

  await onSubmit()

  assert.equal(error.value, 'errors.wrongPassphrase')
})

// pages/index.vue (T075): re-syncs the vault list on window focus, on document visibilitychange
// (only while actually visible), and when an instance is selected (opening the unlock sheet) —
// and removes both listeners on unmount.

/** A `window`/`document` double that records registered listeners by event type and lets a case
 * fire them, plus a settable `visibilityState`. */
function fakeDom() {
  const listeners = new Map<string, Set<() => void>>()
  const on = (type: string, fn: () => void) => {
    if (!listeners.has(type)) listeners.set(type, new Set())
    listeners.get(type)!.add(fn)
  }
  const off = (type: string, fn: () => void) => listeners.get(type)?.delete(fn)
  const fire = (type: string) => {
    for (const fn of listeners.get(type) ?? []) fn()
  }
  const documentDouble = {
    visibilityState: 'visible' as 'visible' | 'hidden',
    addEventListener: on,
    removeEventListener: off,
  }
  return {
    window: { addEventListener: on, removeEventListener: off },
    document: documentDouble,
    fire,
    setVisibility: (state: 'visible' | 'hidden') => {
      documentDouble.visibilityState = state
    },
    listenerCount: (type: string) => listeners.get(type)?.size ?? 0,
  }
}

function loadIndexPage(
  dom: ReturnType<typeof fakeDom>,
  activeName: string | null = null,
  languageLoad: () => Promise<void> = async () => {},
) {
  let synced = 0
  const navigations: string[] = []
  const events: string[] = []
  const colorScheme = { startSystem() {} }
  const store = {
    instances: [] as unknown[],
    lastError: null as string | null,
    async startListening() {},
    stopListening() {},
    async syncAsync() {
      synced += 1
    },
    setActiveInstance() {},
  }
  const page = loadScriptSetup<{
    onSelect: (name: string) => void
    onUnlocked: (name: string) => Promise<void>
    onCreated: (name: string) => Promise<void>
  }>('src/pages/index.vue', ['onSelect', 'onUnlocked', 'onCreated'], {
    useInstancesStore: () => store,
    useColorScheme: () => colorScheme,
    useInstance: () => ({
      activeNameAsync: async () => activeName,
    }),
    usePickedFile: () => ({
      pickOneAsync: async () => null,
      nameOfAsync: async () => '',
    }),
    useLanguage: () => ({
      language: { value: 'en' },
      options: [],
      showAsync: async () => {},
      loadAsync: async () => {
        events.push('language')
        await languageLoad()
      },
    }),
    navigateTo: (path: string) => {
      navigations.push(path)
      events.push(`navigate ${path}`)
    },
    window: dom.window,
    document: dom.document,
  })
  return { ...page, syncCount: () => synced, navigations, events }
}

// Spec 042 (FR-009): the vault's language applies before its workspace (or the alias wizard the
// workspace redirects to) is shown, and a failed read never keeps the vault from opening.
test('pages/index.vue applies the vault language before opening an unlocked or created vault', async () => {
  for (const open of ['onUnlocked', 'onCreated'] as const) {
    const page = loadIndexPage(fakeDom())
    await page[open]('vault-a')
    assert.deepEqual(
      page.events,
      ['language', 'navigate /workspace/vault-a'],
      open,
    )
  }
})

test('pages/index.vue opens the vault even when reading its language fails', async () => {
  const page = loadIndexPage(fakeDom(), null, async () => {
    throw new Error('get_pref failed')
  })
  const logged = console.error
  console.error = () => {}
  try {
    await page.onUnlocked('vault-a')
  } finally {
    console.error = logged
  }
  assert.deepEqual(page.navigations, ['/workspace/vault-a'])
})

test('pages/index.vue recovers the active vault after a frontend reload', async () => {
  const page = loadIndexPage(fakeDom(), 'vault-a')

  await page.mount()

  assert.deepEqual(page.navigations, ['/workspace/vault-a'])
})

test('pages/index.vue re-syncs on window focus and removes the listener on unmount', async () => {
  const dom = fakeDom()
  const page = loadIndexPage(dom)
  await page.mount()
  const before = page.syncCount() // the mount-time sync itself

  dom.fire('focus')
  assert.equal(page.syncCount(), before + 1)

  await page.unmount()
  dom.fire('focus')
  assert.equal(page.syncCount(), before + 1, 'no re-sync after unmount')
})

test('pages/index.vue re-syncs on visibilitychange only while actually visible', async () => {
  const dom = fakeDom()
  const page = loadIndexPage(dom)
  await page.mount()
  const before = page.syncCount()

  dom.setVisibility('hidden')
  dom.fire('visibilitychange')
  assert.equal(page.syncCount(), before, 'no re-sync while hidden')

  dom.setVisibility('visible')
  dom.fire('visibilitychange')
  assert.equal(page.syncCount(), before + 1)
})

test('pages/index.vue re-syncs when an instance is selected (opening the unlock sheet)', async () => {
  const dom = fakeDom()
  const page = loadIndexPage(dom)
  await page.mount()
  const before = page.syncCount()

  page.onSelect('vault-a')

  assert.equal(page.syncCount(), before + 1)
})
