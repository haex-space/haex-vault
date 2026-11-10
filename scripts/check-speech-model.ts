// Part of `pnpm check:settings` (spec 081 T037): what the speech model row says about a model and
// what the hint on a faster model writes (src/lib/assistant/speechModel.ts).
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { test } from 'node:test'

import {
  chosenValue,
  defaultModel,
  mayDelete,
  preferenceFor,
  reserveBytes,
  downloadBytes,
  hintActions,
  soundPhraseKey,
  type SpeechModelOption,
} from '../src/lib/assistant/speechModel.ts'
import {
  SPEECH_MODEL_HINT_KEY,
  speechModelPrefKey,
} from '../src/lib/assistant/settings.ts'

function option(
  id: string,
  engine: SpeechModelOption['engine'],
  overrides: Partial<SpeechModelOption> = {},
): SpeechModelOption {
  return {
    id,
    name: id,
    engine,
    languages:
      engine === 'pocket' ? ['de'] : ['de', 'en', 'fr', 'es', 'it', 'pt'],
    license: 'Apache-2.0',
    sizeBytes: engine === 'pocket' ? 200 : 2_300,
    installed: false,
    fit: 'fits',
    speed: 'unknown',
    ratio: null,
    acceleration: 'cpu',
    chosen: false,
    isDefault: false,
    ...overrides,
  }
}

function localeText(locale: 'de' | 'en', key: string): unknown {
  const doc = JSON.parse(
    readFileSync(resolve(`src/i18n/locales/${locale}.json`), 'utf8'),
  ) as Record<string, unknown>
  return key
    .split('.')
    .reduce<unknown>(
      (node, part) => (node as Record<string, unknown> | undefined)?.[part],
      doc,
    )
}

test('every sound phrase and row text has a German and an English text', () => {
  const keys = [
    soundPhraseKey('pocket'),
    soundPhraseKey('qwen3-tts'),
    'settings.voice.speechModel.default',
    'settings.voice.speechModel.withReserve',
    'settings.voice.speechModel.deleteOf',
    'settings.voice.speechModel.downloadOf',
  ]
  for (const key of keys)
    for (const locale of ['de', 'en'] as const)
      assert.equal(
        typeof localeText(locale, key),
        'string',
        `${locale}: ${key}`,
      )
  assert.notEqual(soundPhraseKey('pocket'), soundPhraseKey('qwen3-tts'))
})

test('Qwen3-TTS downloads the Pocket reserve too while it is missing', () => {
  const pocket = option('pocket-de', 'pocket')
  const qwen = option('qwen3-tts-1.7b', 'qwen3-tts')
  assert.equal(downloadBytes(qwen, [qwen, pocket]), 2_500)
  assert.equal(downloadBytes(pocket, [qwen, pocket]), 200)
  const here = { ...pocket, installed: true }
  assert.equal(downloadBytes(qwen, [qwen, here]), 2_300)
  assert.equal(downloadBytes({ ...qwen, installed: true }, [qwen, here]), 0)
})

test('the choice shows the chosen model, else the default in its own row', () => {
  const pocket = option('pocket-de', 'pocket')
  const qwen = option('qwen3-tts-1.7b', 'qwen3-tts', { isDefault: true })
  assert.equal(chosenValue([qwen, pocket]), 'qwen3-tts-1.7b')
  assert.equal(defaultModel([qwen, pocket])?.id, 'qwen3-tts-1.7b')
  assert.equal(chosenValue([qwen, { ...pocket, chosen: true }]), 'pocket-de')
  assert.equal(chosenValue([]), '')
})

test('choosing the default removes the preference, any other model is stored', () => {
  const pocket = option('pocket-de', 'pocket')
  const qwen = option('qwen3-tts-1.7b', 'qwen3-tts', { isDefault: true })
  assert.equal(preferenceFor('qwen3-tts-1.7b', [qwen, pocket]), null)
  assert.equal(preferenceFor('pocket-de', [qwen, pocket]), 'pocket-de')
})

test('the row names only what Qwen3-TTS downloads beyond itself', () => {
  const pocket = option('pocket-de', 'pocket')
  const qwen = option('qwen3-tts-1.7b', 'qwen3-tts')
  assert.equal(reserveBytes(qwen, [qwen, pocket]), 200)
  assert.equal(reserveBytes(qwen, [qwen, { ...pocket, installed: true }]), 0)
  assert.equal(reserveBytes(pocket, [qwen, pocket]), 0)
})

test('the hint switches to the faster model and is dismissed for good', () => {
  const actions = hintActions({
    language: 'de',
    slowModelId: 'qwen3-tts-1.7b',
    fasterModelId: 'pocket-de',
  })
  assert.deepEqual(actions.switchTo, {
    key: speechModelPrefKey('de'),
    value: 'pocket-de',
  })
  assert.deepEqual(actions.dismiss, {
    key: SPEECH_MODEL_HINT_KEY,
    value: 'dismissed',
  })
})

test('installed models can be deleted, Pocket stays as the reserve', () => {
  const pocket = option('pocket-de', 'pocket', { installed: true })
  const qwen = option('qwen3-tts-1.7b', 'qwen3-tts', { installed: true })
  assert.equal(mayDelete(qwen), true)
  assert.equal(mayDelete(pocket), false)
  assert.equal(mayDelete({ ...qwen, installed: false }), false)
})
