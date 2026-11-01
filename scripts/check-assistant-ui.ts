// Spec 066 T027: the assistant's settings in the interface. Every device setting of data-model.md
// has a German and an English label, and its pages sit under KI → Sprache (`ai/speech`).
import assert from 'node:assert/strict'
import { existsSync, readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { test } from 'node:test'

import {
  ASSISTANT_SETTING_KEYS,
  assistantLabelKey,
  parseSpeechRate,
  SPEECH_LANGUAGES,
  SPEECH_RATE,
  voicePrefKey,
} from '../src/lib/assistant/settings.ts'
import { SETTINGS_LOCATIONS } from '../src/lib/settings/registry.ts'

const root = resolve(import.meta.dirname, '..')
const read = (path: string) => readFileSync(resolve(root, path), 'utf8')

function localeText(locale: 'de' | 'en', key: string): unknown {
  let node: unknown = JSON.parse(read(`src/i18n/locales/${locale}.json`))
  for (const part of key.split('.')) {
    node = (node as Record<string, unknown> | undefined)?.[part]
  }
  return node
}

/** The keys of the table "Geräteeinstellungen" in data-model.md, as written there. */
function dataModelKeys(): string[] {
  const doc = read('specs/066-voice-assistant/data-model.md')
  const table = doc.split('### Geräteeinstellungen')[1]?.split('\n## ')[0]
  assert.ok(table, 'data-model.md has the device settings table')
  return [...table.matchAll(/^\| `(assistant\.[^`]+)`/gm)].map(
    (match) => match[1]!,
  )
}

test('the settings module lists exactly the device settings of data-model.md', () => {
  assert.deepEqual([...ASSISTANT_SETTING_KEYS].sort(), dataModelKeys().sort())
})

test('every device setting has a German and an English label', () => {
  for (const key of ASSISTANT_SETTING_KEYS) {
    for (const locale of ['de', 'en'] as const) {
      const label = localeText(locale, assistantLabelKey(key))
      assert.ok(
        typeof label === 'string' && label.trim() !== '',
        `${locale}: ${key}`,
      )
    }
  }
  assert.equal(
    assistantLabelKey('assistant.tones.volume'),
    'settings.assistant.labels.tonesVolume',
  )
  assert.equal(
    assistantLabelKey('assistant.voice.<sprache>'),
    'settings.assistant.labels.voice',
  )
})

/** The assistant's pages and their views; the tone page (T030) joins the list when it is built. */
const PAGES = [
  {
    id: 'ai.speech.voice',
    view: 'src/components/settings/assistant/VoiceSetting.vue',
    prefs: ['assistant.voice.<sprache>', 'assistant.speechRate'],
  },
] as const

test('the assistant pages sit under KI → Sprache and have a view', () => {
  const routes = read('src/components/wm/appRoutes.ts')
  for (const page of PAGES) {
    const location = SETTINGS_LOCATIONS.find((l) => l.id === page.id)
    assert.ok(location, page.id)
    assert.match(location.pattern, /^ai\/speech\//, page.id)
    assert.equal(location.group, 'speech', page.id)
    assert.ok(location.overviewRow, `${page.id} is a row of the KI overview`)
    assert.ok(existsSync(resolve(root, page.view)), page.view)
    assert.ok(
      routes.includes(`'${page.id}'`) &&
        routes.includes(page.view.replace(/^src\//, '~/')),
      `${page.id} → ${page.view} in appRoutes.ts`,
    )
    for (const pref of page.prefs) {
      assert.ok(
        location.settingKeys?.includes(assistantLabelKey(pref)),
        `${page.id} is found by the label of ${pref}`,
      )
    }
  }
})

test('the voice page saves on selection, as device settings, without a save button', () => {
  const view = read('src/components/settings/assistant/VoiceSetting.vue')
  assert.match(view, /kind: 'device'/)
  assert.doesNotMatch(view, /common\.save|\.save'\)/)
  assert.match(view, /voicePrefKey\(language\)/)
  assert.match(view, /SPEECH_RATE_KEY/)
})

test('the voice preferences and the rate range match data-model.md', () => {
  assert.deepEqual(
    SPEECH_LANGUAGES.map(voicePrefKey),
    SPEECH_LANGUAGES.map((language) => `assistant.voice.${language}`),
  )
  const row = dataModelKeys().includes('assistant.speechRate')
  assert.ok(row)
  const doc = read('specs/066-voice-assistant/data-model.md')
  assert.match(
    doc,
    new RegExp(
      `\`assistant\\.speechRate\`\\s*\\|\\s*float ${SPEECH_RATE.min}–${SPEECH_RATE.max}\\s*\\|\\s*\`${SPEECH_RATE.default.toFixed(1)}\``,
    ),
  )
  assert.equal(parseSpeechRate(null), SPEECH_RATE.default)
  assert.equal(parseSpeechRate('1.25'), 1.25)
  assert.equal(parseSpeechRate('2'), SPEECH_RATE.default)
  assert.equal(parseSpeechRate('schnell'), SPEECH_RATE.default)
})
