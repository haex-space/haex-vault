// `pnpm check:settings` (spec 086): the context setting of a local model as the settings page words it.
import assert from 'node:assert/strict'
import { test } from 'node:test'
import { readFileSync } from 'node:fs'
import ts from 'typescript'

import {
  DEFAULT_CONTEXT_SETTING,
  isDefaultSetting,
  isRefused,
  noteKeys,
  parseTokens,
  presetsFor,
  reasonKey,
  tokensOf,
  verdictKey,
  type ContextReport,
  type ContextVerdict,
} from '../src/lib/models/contextSetting.ts'

function report(partial: Partial<ContextReport> = {}): ContextReport {
  return {
    trained: 32768,
    maxOnGpu: 65000,
    maxAny: 131072,
    maxStretched: 131072,
    sized: true,
    verdict: 'fits',
    needMib: null,
    freeMib: null,
    yarn: null,
    effective: null,
    ...partial,
  }
}

test('a typed length is digits or a number of k, and nothing else', () => {
  assert.equal(parseTokens('131072'), 131072)
  assert.equal(parseTokens(' 128k '), 131072)
  assert.equal(parseTokens('64 K'), 65536)
  assert.equal(parseTokens('0,5k'), 512)
  assert.equal(parseTokens('4294967295'), 4294967295)
  for (const text of ['4294967296', '4194304k', '99999999999']) {
    assert.equal(parseTokens(text), null, text)
  }
  for (const text of ['', 'auto', '12k4', '-5', '1e5', '0', '0.3', 'k']) {
    assert.equal(parseTokens(text), null, text)
  }
})

test('the default setting is Auto with the standard cache and a fixed one is a number', () => {
  assert.equal(isDefaultSetting(DEFAULT_CONTEXT_SETTING), true)
  assert.equal(isDefaultSetting({ context: 'auto', cache: 'q8' }), false)
  assert.equal(isDefaultSetting({ context: '65536', cache: 'standard' }), false)
  assert.equal(tokensOf(DEFAULT_CONTEXT_SETTING), null)
  assert.equal(tokensOf({ context: '65536', cache: 'q8' }), 65536)
})

test('the presets stop at what the model can be stretched to, else at its own length', () => {
  assert.deepEqual(presetsFor(report()), [8192, 16384, 32768, 65536, 131072])
  assert.deepEqual(
    presetsFor(report({ maxStretched: null, trained: 32768 })),
    [8192, 16384, 32768],
  )
  assert.equal(presetsFor(null).at(-1), 1048576)
})

test('every verdict has a sentence and only the broken rules refuse a setting', () => {
  const verdicts: ContextVerdict[] = [
    'fits',
    'fits-on-host',
    'too-large',
    'beyond-training',
    'not-stretchable',
    'below-floor',
    'unsized',
  ]
  assert.deepEqual(verdicts.map(verdictKey), [
    'models.context.verdict.fits',
    'models.context.verdict.fitsOnHost',
    'models.context.verdict.tooLarge',
    'models.context.verdict.beyondTraining',
    'models.context.verdict.notStretchable',
    'models.context.verdict.belowFloor',
    'models.context.verdict.unsized',
  ])
  assert.deepEqual(verdicts.filter(isRefused), [
    'beyond-training',
    'not-stretchable',
    'below-floor',
  ])
})

test('a stretch and a smaller cache each bring their note', () => {
  assert.deepEqual(noteKeys(DEFAULT_CONTEXT_SETTING, report()), [])
  assert.deepEqual(
    noteKeys({ context: '65536', cache: 'q4' }, report({ yarn: 2 })),
    ['models.context.note.yarn', 'models.context.note.cache'],
  )
  assert.equal(reasonKey('trained'), 'models.context.reason.trained')
})

test('context actions reject malformed lengths before IPC and preserve valid wire values', async () => {
  const code = ts.transpileModule(
    readFileSync('src/stores/settingsActionHandlers.ts', 'utf8'),
    {
      compilerOptions: {
        module: ts.ModuleKind.CommonJS,
        target: ts.ScriptTarget.ES2022,
      },
    },
  ).outputText
  const handlers = new Map<
    string,
    (args: { input: Record<string, unknown> }) => Promise<unknown>
  >()
  const calls: unknown[] = []
  const exports = new Function(
    'exports',
    'require',
    `${code}; return exports;`,
  )({}, (name: string) => {
    if (name === '~/lib/models/contextSetting') return { parseTokens }
    if (name === '@tauri-apps/api/core')
      return {
        invoke: async (command: string, args: { setting: unknown }) => {
          calls.push({ command, args })
          return args.setting
        },
      }
    // Other settings domains only register callbacks; their dependencies stay unused.
    return new Proxy({}, { get: () => () => ({}) })
  })
  exports.registerSettingsActionHandlers({
    registerGlobalActionHandler: (
      id: string,
      handler: (args: { input: Record<string, unknown> }) => Promise<unknown>,
    ) => handlers.set(id, handler),
  })
  for (const action of ['set', 'report']) {
    const handler = handlers.get(`settings.models.context.${action}`)!
    for (const context of ['abc', '', '0', '-1', '1.5', 'NaN', '4294967296']) {
      await assert.rejects(
        handler({ input: { modelId: 'a', context, cache: 'q8' } }),
        /Context must be/,
      )
    }
    assert.equal(calls.length, 0)
    await handler({ input: { modelId: 'a', context: '65536', cache: 'q8' } })
    assert.deepEqual(calls.pop(), {
      command: action === 'set' ? 'set_model_context' : 'model_context_report',
      args: { modelId: 'a', setting: { context: 65536, cache: 'q8' } },
    })
    await handler({
      input: { modelId: 'a', context: 'auto', cache: 'standard' },
    })
    assert.deepEqual(calls.pop(), {
      command: action === 'set' ? 'set_model_context' : 'model_context_report',
      args: { modelId: 'a', setting: { context: 'auto', cache: 'standard' } },
    })
  }
})
