// Component boundary checks: delayed actions must keep each model's settings and the last choice.
import assert from 'node:assert/strict'
import { test } from 'node:test'
import { setImmediate } from 'node:timers/promises'
import { ref, type Ref } from 'vue'

import * as context from '../src/lib/models/contextSetting.ts'
import { loadScriptSetup } from './lib/script-setup-sandbox.ts'

type Input = { modelId: string } & context.ContextSetting
type Action = (input: Input) => Promise<unknown>

const report: context.ContextReport = {
  trained: 32768,
  maxOnGpu: 131072,
  maxAny: 131072,
  maxStretched: 131072,
  sized: true,
  verdict: 'fits',
  needMib: null,
  freeMib: null,
  yarn: null,
  effective: null,
}

function deferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (reason: Error) => void
  const promise = new Promise<T>((yes, no) => {
    resolve = yes
    reject = no
  })
  return { promise, resolve, reject }
}

function loadContextSetting(overrides: Partial<Record<string, Action>> = {}) {
  const writes: Input[] = []
  const events: unknown[][] = []
  const errors: string[] = []
  const actions: Record<string, Action> = {
    get: async () => ({ ...context.DEFAULT_CONTEXT_SETTING }),
    report: async () => report,
    set: async (input) => {
      writes.push(input)
      return { context: input.context, cache: input.cache }
    },
    ...overrides,
  }
  const component = loadScriptSetup<{
    setting: Ref<context.ContextSetting>
    report: Ref<context.ContextReport | null>
    saving: Ref<boolean>
    loading: Ref<boolean>
    typed: Ref<string>
    typedError: Ref<string | undefined>
    commitTypedAsync: () => Promise<void>
    saveAsync: (next: context.ContextSetting) => Promise<void>
    refreshAsync: () => Promise<void>
  }>(
    'src/components/settings/ModelContextSetting.vue',
    [
      'setting',
      'report',
      'saving',
      'loading',
      'typed',
      'typedError',
      'commitTypedAsync',
      'saveAsync',
      'refreshAsync',
    ],
    {
      props: { modelId: 'a', active: true, busy: false },
      emit: (...args) => events.push(args),
      useFieldLabels: () => ({ input: { value: {} } }),
      useErrorString: () => ({ errString: String }),
      useActionOrThrow: (id) => (input: Input) =>
        actions[id.split('.').at(-1)!]!(input),
      imports: {
        '~/lib/models/contextSetting': context,
        'vue-sonner': {
          toast: { error: (message: string) => errors.push(message) },
        },
      },
    },
  )
  return { ...component, writes, events, errors, actions }
}

test('a model cannot be edited before its persisted setting arrives', async () => {
  const get = deferred<context.ContextSetting>()
  const c = loadContextSetting({ get: async () => get.promise })
  assert.equal(c.loading.value, true)
  await c.saveAsync({ context: '8192', cache: 'q8' })
  assert.deepEqual(c.writes, [])
  get.resolve({ context: '65536', cache: 'standard' })
  await setImmediate()
  assert.equal(c.loading.value, false)
  assert.equal(c.setting.value.context, '65536')
})

for (const fails of [false, true]) {
  test(`a typed report for a previous selection is ignored (failure: ${fails})`, async () => {
    const c = loadContextSetting()
    await setImmediate()
    const check = deferred<context.ContextReport>()
    c.actions.report = async () => check.promise
    c.typed.value = '64k'
    const saving = c.commitTypedAsync()
    await setImmediate()
    c.actions.report = async () => report
    c.props.modelId = 'b'
    await setImmediate()
    // Even returning to A must not revive the stale submission.
    c.props.modelId = 'a'
    await setImmediate()
    if (fails) check.reject(new Error('stale report'))
    else check.resolve(report)
    await saving
    assert.deepEqual(c.writes, [])
    assert.deepEqual(c.events, [])
    assert.deepEqual(c.errors, [])
    assert.equal(c.typedError.value, undefined)
    assert.equal(c.saving.value, false)
  })
}

test('a completed write belongs to its original model after selection changes', async () => {
  const saved = deferred<context.ContextSetting>()
  const c = loadContextSetting({ set: async () => saved.promise })
  await setImmediate()
  const saving = c.saveAsync({ context: '65536', cache: 'q8' })
  await setImmediate()
  c.props.modelId = 'b'
  await setImmediate()
  saved.resolve({ context: '65536', cache: 'q8' })
  await saving
  await setImmediate()
  assert.deepEqual(c.setting.value, context.DEFAULT_CONTEXT_SETTING)
  assert.deepEqual(c.events, [['changed', 'a']])
  assert.equal(c.report.value?.trained, 32768)
  assert.equal(c.saving.value, false)
})

test('report failure cannot suppress the reload after a successful write', async () => {
  const c = loadContextSetting()
  await setImmediate()
  c.actions.report = async () => {
    throw new Error('report failed')
  }
  await c.saveAsync({ context: '65536', cache: 'q8' })
  assert.deepEqual(c.events, [['changed', 'a']])
  assert.equal(c.setting.value.context, '65536')
  assert.equal(c.errors.length, 1)
  assert.equal(c.saving.value, false)
})

test('typed submissions are persisted in order even across a pending write', async () => {
  const c = loadContextSetting()
  await setImmediate()
  const first = deferred<context.ContextSetting>()
  const inputs: Input[] = []
  c.actions.set = async (input) => {
    inputs.push(input)
    return inputs.length === 1 ? first.promise : input
  }
  c.typed.value = '32k'
  const a = c.commitTypedAsync()
  await setImmediate()
  c.typed.value = '64k'
  const b = c.commitTypedAsync()
  await setImmediate()
  assert.equal(inputs.length, 1)
  first.resolve({ context: '32768', cache: 'standard' })
  await Promise.all([a, b])
  assert.deepEqual(
    inputs.map((input) => input.context),
    ['32768', '65536'],
  )
  assert.equal(c.setting.value.context, '65536')
  assert.equal(c.typed.value, '64k')
  assert.deepEqual(c.events, [['changed', 'a']])
  assert.equal(c.saving.value, false)
})

test('an old report cannot overwrite the report for the saved setting', async () => {
  const c = loadContextSetting()
  await setImmediate()
  const old = deferred<context.ContextReport>()
  c.actions.report = async () => old.promise
  const refresh = c.refreshAsync()
  c.actions.report = async () => ({ ...report, verdict: 'fits-on-host' })
  await c.saveAsync({ context: '65536', cache: 'q8' })
  old.resolve(report)
  await refresh
  assert.equal(c.report.value?.verdict, 'fits-on-host')
})

test('a failed write does not reload and a later choice can still be saved', async () => {
  const c = loadContextSetting()
  await setImmediate()
  c.actions.set = async () => {
    throw new Error('write failed')
  }
  await c.saveAsync({ context: '65536', cache: 'q8' })
  assert.deepEqual(c.events, [])
  assert.deepEqual(c.setting.value, context.DEFAULT_CONTEXT_SETTING)
  assert.equal(c.errors.length, 1)
  c.actions.set = async (input) => input
  await c.saveAsync({ context: '32768', cache: 'standard' })
  assert.equal(c.setting.value.context, '32768')
  assert.deepEqual(c.events, [['changed', 'a']])
})

test('a forbidden typed setting stays a field error without writing or reloading', async () => {
  const c = loadContextSetting()
  await setImmediate()
  c.actions.report = async () => ({ ...report, verdict: 'below-floor' })
  c.typed.value = '512'
  await c.commitTypedAsync()
  assert.equal(c.typedError.value, 'models.context.verdict.belowFloor')
  assert.deepEqual(c.writes, [])
  assert.deepEqual(c.events, [])
  assert.deepEqual(c.errors, [])
})

test('a refused queued choice still reloads the preceding successful write once', async () => {
  const c = loadContextSetting()
  await setImmediate()
  const first = deferred<context.ContextSetting>()
  c.actions.set = async () => first.promise
  c.actions.report = async (input) => ({
    ...report,
    verdict: input.context === '512' ? 'below-floor' : 'fits',
  })
  c.typed.value = '32k'
  const saved = c.commitTypedAsync()
  await setImmediate()
  c.typed.value = '512'
  const refused = c.commitTypedAsync()
  first.resolve({ context: '32768', cache: 'standard' })
  await Promise.all([saved, refused])
  assert.deepEqual(c.events, [['changed', 'a']])
  assert.equal(c.setting.value.context, '32768')
  assert.equal(c.typedError.value, 'models.context.verdict.belowFloor')
  assert.equal(c.saving.value, false)
})

test('returning to a model reads its setting after the outstanding write completes', async () => {
  const saved = deferred<context.ContextSetting>()
  let kept = { ...context.DEFAULT_CONTEXT_SETTING }
  const c = loadContextSetting({
    get: async () => ({ ...kept }),
    set: async () => {
      kept = await saved.promise
      return kept
    },
  })
  await setImmediate()
  const saving = c.saveAsync({ context: '65536', cache: 'q8' })
  await setImmediate()
  c.props.modelId = 'b'
  c.props.modelId = 'a'
  assert.equal(c.loading.value, true)
  saved.resolve({ context: '65536', cache: 'q8' })
  await saving
  await setImmediate()
  assert.deepEqual(c.setting.value, kept)
  assert.equal(c.loading.value, false)
})

for (const outcome of ['unloaded', 'activated', 'failed'] as const) {
  test(`context reload waits for a competing model operation (${outcome})`, async () => {
    const active = ref<string | null>('a')
    const pending = deferred<undefined>()
    const activations: string[] = []
    const c = loadScriptSetup<{
      contextReloadIds: Ref<Set<string>>
      busyModelId: Ref<string | null>
      activateAsync: (id: string) => Promise<void>
      unloadAsync: (id: string) => Promise<void>
    }>(
      'src/components/settings/InstalledModels.vue',
      ['contextReloadIds', 'busyModelId', 'activateAsync', 'unloadAsync'],
      {
        useModelsStore: () => ({
          installedModels: ref([
            { id: 'a', name: 'A' },
            { id: 'b', name: 'B' },
          ]),
          activeLocalModelId: active,
          integrityDialog: ref(null),
          integrityBusy: ref(false),
          integrityActionError: ref(null),
        }),
        useModelDownloadsStore: () => ({}),
        useActionOrThrow: () => async () => {},
        useAction: (action) => async (input: { modelId: string }) => {
          if (action === 'settings.models.deactivate') {
            await pending.promise
            active.value = null
          } else if (action === 'settings.models.activate') {
            activations.push(input.modelId)
            if (input.modelId === 'b') {
              await pending.promise
              if (outcome !== 'failed') active.value = 'b'
            }
          }
        },
        imports: {
          'vue-sonner': { toast: { error: () => {} } },
          '~/lib/models/format': { humanBytes: String },
        },
      },
    )
    const operation =
      outcome === 'unloaded' ? c.unloadAsync('a') : c.activateAsync('b')
    c.contextReloadIds.value.add('a')
    await setImmediate()
    assert.deepEqual(activations, outcome === 'unloaded' ? [] : ['b'])
    pending.resolve(undefined)
    await operation
    await setImmediate()
    assert.deepEqual(
      activations,
      outcome === 'failed' ? ['b', 'a'] : outcome === 'activated' ? ['b'] : [],
    )
    assert.equal(c.busyModelId.value, null)
    assert.equal(c.contextReloadIds.value.size, 0)
  })
}
