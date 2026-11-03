// Run with `node scripts/check-vault-passphrase-lifetime.ts`. Frontend cases for spec 013 Stage 6
// (specs/013-vault-lifecycle-isolation, US3/US5): the passphrase lifetime in the unlock and create
// sheets, and the model-store reads that must never fail. tasks.md's T078/T079 name
// `check-vault-lifecycle.ts` as the target file, but adding this content there would push it past
// the spaex 500-LoC boundary (it is a from-scratch addition, not a pre-existing oversized file with
// its own documented exception like `check-chat-state.ts`), so it lives here instead — the
// constitution's line "This constitution supersedes local per-spec preferences" applies. `test:
// (ui): cover passphrase lifetime and overlapping model reads` (T082) notes the deviation.
import assert from 'node:assert/strict'
import { dirname, resolve as resolvePath } from 'node:path'
import { test } from 'node:test'
import { fileURLToPath } from 'node:url'
import * as pinia from 'pinia'
import { nextTick, ref } from 'vue'
import {
  createChatState,
  flush,
  runComposable,
} from './lib/chat-state-harness.ts'
import { loadScriptSetup } from './lib/script-setup-sandbox.ts'

const repoRoot = resolvePath(dirname(fileURLToPath(import.meta.url)), '..')

// Passphrase lifetime (T078, US3, FR-016, SC-004/SC-005, quickstart scenario 5): the field is kept
// only until the unlock/create succeeds, the sheet is dismissed, or a close begins (a close needs
// no case here — the backend discards the page). A failed attempt keeps it so the user can correct
// a typo.

const PASSPHRASE_MARKER = 'zz-quickstart5-marker-9f3c-zz'

/** The sheets show a failed attempt as a toast; these cases only look at the passphrase. */
const sonner = { toast: { error: () => {} } }

function loadUnlockSheet(
  openAsync: (args?: unknown) => Promise<{ name: string }>,
  emit: (event: string, ...args: unknown[]) => void = () => {},
  useInstance: () => {
    openAsync: (args?: unknown) => Promise<{ name: string }>
  } = () => ({ openAsync }),
) {
  return loadScriptSetup<{
    onSubmit: () => Promise<void>
    passphrase: { value: string }
  }>('src/components/onboarding/UnlockSheet.vue', ['onSubmit', 'passphrase'], {
    imports: { 'vue-sonner': sonner },
    useInstance,
    useFieldLabels: () => ({
      input: { value: {} },
      password: { value: {} },
    }),
    props: { open: true, name: 'vault' },
    emit,
  })
}

function loadCreateSheet(
  createAsync: (args?: unknown) => Promise<{ info: { name: string } }>,
  emit: (event: string, ...args: unknown[]) => void = () => {},
  useInstance: () => {
    createAsync: (args?: unknown) => Promise<{ info: { name: string } }>
  } = () => ({ createAsync }),
) {
  return loadScriptSetup<{
    onSubmit: () => Promise<void>
    name: { value: string }
    passphrase: { value: string }
    passphraseConfirm: { value: string }
  }>(
    'src/components/onboarding/CreateSheet.vue',
    ['onSubmit', 'name', 'passphrase', 'passphraseConfirm'],
    {
      imports: { 'vue-sonner': sonner },
      useInstance,
      useFieldLabels: () => ({
        input: { value: {} },
        password: { value: {} },
      }),
      props: { open: true },
      emit,
    },
  )
}

/** Loads the real `useInstancesStore` (the one store any onboarding flow runs alongside, on
 * `pages/index.vue`) against a genuine, fresh Pinia instance, so a case can assert nothing ever
 * lands in its serialized state. */
function loadInstancesStore() {
  const req = (specifier: string): unknown => {
    if (specifier === 'pinia') return pinia
    if (specifier === '@tauri-apps/api/event')
      return { listen: async () => () => {} }
    throw new Error(`instances store sandbox: unexpected import '${specifier}'`)
  }
  const mod = runComposable(
    resolvePath(repoRoot, 'src/stores/instances.ts'),
    req,
    {
      ref,
      useInstance: () => ({ listAsync: async () => [] }),
      useErrorString: () => ({ errString: (e: unknown) => String(e) }),
    },
  ) as {
    useInstancesStore: () => { setActiveInstance: (n: string | null) => void }
  }
  return mod.useInstancesStore()
}

test('UnlockSheet clears the passphrase before emitting unlocked, and it reaches no Pinia store', async () => {
  const testPinia = pinia.createPinia()
  pinia.setActivePinia(testPinia)
  const instancesStore = loadInstancesStore()
  const realInstance = runComposable(
    resolvePath(repoRoot, 'src/composables/useInstance.ts'),
    (specifier) => {
      if (specifier === '@tauri-apps/api/core') {
        return {
          invoke: async (command: string, args: unknown) => {
            assert.equal(command, 'open_instance')
            assert.deepEqual(args, {
              args: { name: 'vault', passphrase: PASSPHRASE_MARKER },
            })
            return { name: 'vault' }
          },
        }
      }
      throw new Error(`useInstance sandbox: unexpected import '${specifier}'`)
    },
  ) as {
    useInstance: () => {
      openAsync: (args?: unknown) => Promise<{ name: string }>
    }
  }
  const emittedPassphrase: Record<string, string> = {}
  const passphraseRef: { current: { value: string } | null } = { current: null }
  const { onSubmit, passphrase } = loadUnlockSheet(
    async () => ({ name: 'vault' }),
    (event) => {
      emittedPassphrase[event] = passphraseRef.current!.value
    },
    realInstance.useInstance,
  )
  passphraseRef.current = passphrase
  passphrase.value = PASSPHRASE_MARKER

  await onSubmit()
  instancesStore.setActiveInstance('vault')

  assert.equal(
    emittedPassphrase.unlocked,
    '',
    'cleared before the unlocked event is emitted',
  )
  assert.doesNotMatch(
    JSON.stringify(testPinia.state.value),
    new RegExp(PASSPHRASE_MARKER),
  )
})

test('UnlockSheet keeps the passphrase after a failed attempt', async () => {
  const { onSubmit, passphrase } = loadUnlockSheet(() => {
    throw { kind: 'WrongPassphrase' }
  })
  passphrase.value = PASSPHRASE_MARKER

  await onSubmit()

  assert.equal(passphrase.value, PASSPHRASE_MARKER)
})

test('UnlockSheet clears the passphrase when the sheet is dismissed', async () => {
  const sheet = loadUnlockSheet(async () => ({ name: 'vault' }))
  sheet.passphrase.value = PASSPHRASE_MARKER

  sheet.props.open = false
  await nextTick()

  assert.equal(sheet.passphrase.value, '')
})

test('CreateSheet clears the passphrase before emitting created, and it reaches no Pinia store', async () => {
  const testPinia = pinia.createPinia()
  pinia.setActivePinia(testPinia)
  const instancesStore = loadInstancesStore()
  const realInstance = runComposable(
    resolvePath(repoRoot, 'src/composables/useInstance.ts'),
    (specifier) => {
      if (specifier === '@tauri-apps/api/core') {
        return {
          invoke: async (command: string, args: unknown) => {
            assert.equal(command, 'create_instance')
            assert.deepEqual(args, {
              args: {
                name: 'vault',
                passphrase: PASSPHRASE_MARKER,
              },
            })
            return { info: { name: 'vault' } }
          },
        }
      }
      throw new Error(`useInstance sandbox: unexpected import '${specifier}'`)
    },
  ) as {
    useInstance: () => {
      createAsync: (args?: unknown) => Promise<{ info: { name: string } }>
    }
  }
  const emittedPassphrase: Record<string, string> = {}
  const passphraseRef: { current: { value: string } | null } = { current: null }
  const { onSubmit, name, passphrase, passphraseConfirm } = loadCreateSheet(
    async () => ({ info: { name: 'vault' } }),
    (event) => {
      emittedPassphrase[event] = passphraseRef.current!.value
    },
    realInstance.useInstance,
  )
  passphraseRef.current = passphrase
  name.value = 'vault'
  passphrase.value = PASSPHRASE_MARKER
  passphraseConfirm.value = PASSPHRASE_MARKER

  await onSubmit()
  instancesStore.setActiveInstance('vault')

  assert.equal(
    emittedPassphrase.created,
    '',
    'cleared before the created event is emitted',
  )
  assert.equal(passphraseConfirm.value, '')
  assert.doesNotMatch(
    JSON.stringify(testPinia.state.value),
    new RegExp(PASSPHRASE_MARKER),
  )
})

test('CreateSheet keeps the passphrase after a failed attempt', async () => {
  const { onSubmit, name, passphrase, passphraseConfirm } = loadCreateSheet(
    () => {
      throw { kind: 'openFailed' }
    },
  )
  name.value = 'vault'
  passphrase.value = PASSPHRASE_MARKER
  passphraseConfirm.value = PASSPHRASE_MARKER

  await onSubmit()

  assert.equal(passphrase.value, PASSPHRASE_MARKER)
  assert.equal(passphraseConfirm.value, PASSPHRASE_MARKER)
})

test('CreateSheet clears the passphrase when the sheet is dismissed', async () => {
  const sheet = loadCreateSheet(async () => ({ info: { name: 'vault' } }))
  sheet.name.value = 'vault'
  sheet.passphrase.value = PASSPHRASE_MARKER
  sheet.passphraseConfirm.value = PASSPHRASE_MARKER

  sheet.props.open = false
  await nextTick()

  assert.equal(sheet.passphrase.value, '')
  assert.equal(sheet.passphraseConfirm.value, '')
})

// Overlapping reads (T079, US5, FR-025, quickstart scenario 7): `active_model_info` deliberately
// does not take `ChatState`'s exclusive operation slot (fix fedcfa8, src-tauri/src/chat/model_
// loading.rs) — it used to, and a read that took it rejected a second overlapping read with "a
// chat or vault operation is still in progress", which aborted `initialize()` before
// `refreshProviders()` ran (the effort control stayed on "not yet known" forever). `initialize()`
// itself issues two overlapping reads on every mount: one fired-and-forgotten from
// `applyLoadStatus`'s 'ready' branch, one it awaits directly right after. This proves it completes,
// and refreshes the providers, even while the first is still in flight.

const modelStorePresets = (...ids: string[]) => ({
  reasoning: {
    kind: 'presets' as const,
    options: ids.map((id) => ({ id, label: id })),
  },
  acceptedAttachmentKinds: [],
  thinkingStyle: null,
})

test('modelStore.initialize() completes and refreshes providers despite an active_model_info read still in flight', async () => {
  let reads = 0
  let releaseFirstRead: () => void = () => {}
  const firstReadGate = new Promise<void>((resolve) => {
    releaseFirstRead = resolve
  })
  const invokeLog: string[] = []
  const state = createChatState(
    {
      modelLoadStatusAsync: async () => ({
        status: 'ready',
        modelId: 'model',
        modelName: 'Model',
      }),
      activeLocalModelInfoAsync: async () => null,
      activeModelInfoAsync: async () => {
        reads += 1
        if (reads === 1) await firstReadGate // the slow, still-in-flight first read
        return { modelId: 'model' }
      },
    },
    {},
    {
      useModels: () => ({
        listInstalledAsync: async () => [
          {
            id: 'model',
            name: 'Model',
            providerId: 'local',
            contextWindow: null,
            capabilities: modelStorePresets('low', 'high'),
          },
        ],
      }),
    },
    invokeLog,
  )

  const initializing = state.modelStore.initialize()
  const timeout = new Promise((_, reject) => {
    setTimeout(
      () =>
        reject(
          new Error(
            'initialize() did not resolve - something now awaits the still-open first read',
          ),
        ),
      500,
    )
  })
  await Promise.race([initializing, timeout])
  releaseFirstRead()
  await flush()

  assert.equal(reads, 2, 'both the fire-and-forget and the direct read ran')
  assert.ok(
    invokeLog.includes('list_providers'),
    'initialize() reached refreshProviders() instead of aborting on the second read',
  )
  assert.equal(state.modelStore.effortState, 'selectable')
  assert.deepEqual(
    state.modelStore.effortOptions.map((o: { id: string }) => o.id),
    ['low', 'high'],
  )
})
