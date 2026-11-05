// Part of `pnpm check:settings` (spec 081 T037): what the speech model row says about a model and
// what the hint on a faster model writes (src/lib/assistant/speechModel.ts).
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { test } from 'node:test'

import {
  AUTOMATIC,
  chosenValue,
  defaultModel,
  downloadBytes,
  hintActions,
  soundPhraseKey,
  speedLabelKey,
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

test('every speed mark and sound phrase has a German and an English text', () => {
  const keys = [
    ...(['keepsUp', 'slow', 'unknown'] as const).map((speed) =>
      speedLabelKey({ speed }),
    ),
    soundPhraseKey('pocket'),
    soundPhraseKey('qwen3-tts'),
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

test('the choice shows the chosen model, else Automatisch with the default', () => {
  const pocket = option('pocket-de', 'pocket')
  const qwen = option('qwen3-tts-1.7b', 'qwen3-tts', { isDefault: true })
  assert.equal(chosenValue([qwen, pocket]), AUTOMATIC)
  assert.equal(defaultModel([qwen, pocket])?.id, 'qwen3-tts-1.7b')
  assert.equal(chosenValue([qwen, { ...pocket, chosen: true }]), 'pocket-de')
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
