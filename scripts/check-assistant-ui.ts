// Spec 066 T027: the assistant's settings in the interface. Every device setting of data-model.md
// has a German and an English label, and its pages sit under KI → Sprache (`ai/speech`).
import assert from 'node:assert/strict'
import { existsSync, readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { test } from 'node:test'

import {
  ALWAYS_AUDIBLE_TONES,
  ASSISTANT_SETTING_KEYS,
  assistantLabelKey,
  PAUSE_MS,
  parsePause,
  parsePreview,
  parseSpeechRate,
  parseTonesEnabled,
  parseTonesVolume,
  SPEECH_LANGUAGES,
  SPEECH_RATE,
  TONE_IDS,
  TONES_VOLUME,
  voicePrefKey,
} from '../src/lib/assistant/settings.ts'
import {
  BUBBLE_DEFAULT,
  lastEntries,
  parseBubblePosition,
  PHASE_ICONS,
  shareOf,
  splitSpoken,
} from '../src/lib/assistant/bubble.ts'
import {
  ASSISTANT_OFF,
  sortEntries,
  utcOffsetMinutes,
} from '../src/lib/assistant/wire.ts'
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

/** The assistant's pages, their views and the settings each one holds. */
const PAGES = [
  {
    id: 'ai.speech.assistant',
    view: 'src/components/settings/assistant/AssistantSetting.vue',
    prefs: ['assistant.preview', 'assistant.pauseMs'],
  },
  {
    id: 'ai.speech.voice',
    view: 'src/components/settings/assistant/VoiceSetting.vue',
    prefs: ['assistant.voice.<sprache>', 'assistant.speechRate'],
  },
  {
    id: 'ai.speech.tones',
    view: 'src/components/settings/assistant/TonesSetting.vue',
    prefs: ['assistant.tones.enabled', 'assistant.tones.volume'],
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

test('the pages save on selection, as device settings, without a save button', () => {
  for (const page of PAGES) {
    const view = read(page.view)
    assert.match(view, /kind: 'device'/, page.view)
    assert.doesNotMatch(view, /common\.save|\.save'\)/, page.view)
  }
  const voice = read('src/components/settings/assistant/VoiceSetting.vue')
  assert.match(voice, /voicePrefKey\(language\)/)
  assert.match(voice, /SPEECH_RATE_KEY/)
  const tones = read('src/components/settings/assistant/TonesSetting.vue')
  assert.match(tones, /TONES_ENABLED_KEY/)
  assert.match(tones, /TONES_VOLUME_KEY/)
})

/** The variants of `pub enum ToneId` in tones.rs, as their wire names. */
function rustTones(): string[] {
  const source = read('src-tauri/src/assistant/tones.rs')
  const body = source.split('pub enum ToneId {')[1]?.split('}')[0]
  assert.ok(body, 'tones.rs declares ToneId')
  return [...body.matchAll(/^\s*(\w+),/gm)].map(
    (m) => m[1]!.charAt(0).toLowerCase() + m[1]!.slice(1),
  )
}

test('the tone page lists every tone of tones.rs and of the contract', () => {
  assert.deepEqual([...TONE_IDS], rustTones())
  const contract = read('specs/066-voice-assistant/contracts/tauri-commands.md')
  const line = contract.split('`ToneId` =')[1]?.split('\n\n')[0] ?? ''
  for (const tone of TONE_IDS) assert.match(line, new RegExp(`\`${tone}\``))
})

test('every tone has a name and a meaning in German and English (FR-044)', () => {
  for (const tone of TONE_IDS) {
    for (const locale of ['de', 'en'] as const) {
      for (const part of ['names', 'when']) {
        const text = localeText(locale, `settings.tones.${part}.${tone}`)
        assert.ok(
          typeof text === 'string' && text.trim() !== '',
          `${locale}: ${part}.${tone}`,
        )
      }
    }
  }
})

test('the tones that always sound are the ones tones.rs lets through (FR-045)', () => {
  const source = read('src-tauri/src/assistant/tones.rs')
  const rule = source.match(
    /fn always_audible\(tone: ToneId\) -> bool \{\s*matches!\(tone, ([^)]+)\)/,
  )
  assert.ok(rule, 'tones.rs has always_audible')
  const rust = [...rule[1]!.matchAll(/ToneId::(\w+)/g)].map(
    (m) => m[1]!.charAt(0).toLowerCase() + m[1]!.slice(1),
  )
  assert.deepEqual([...ALWAYS_AUDIBLE_TONES].sort(), rust.sort())
})

test('the stored tone settings fall back to the defaults of data-model.md', () => {
  assert.equal(parseTonesEnabled(null), true)
  assert.equal(parseTonesEnabled('false'), false)
  assert.equal(parseTonesEnabled('true'), true)
  assert.equal(parseTonesVolume(null), TONES_VOLUME.default)
  assert.equal(parseTonesVolume('35'), 35)
  assert.equal(parseTonesVolume('101'), TONES_VOLUME.default)
  assert.equal(parseTonesVolume('2.5'), TONES_VOLUME.default)
  const doc = read('specs/066-voice-assistant/data-model.md')
  assert.match(
    doc,
    new RegExp(
      `\`assistant\\.tones\\.volume\`\\s*\\|\\s*int ${TONES_VOLUME.min}–${TONES_VOLUME.max}\\s*\\|\\s*\`${TONES_VOLUME.default}\``,
    ),
  )
})

test('the pause range and default match data-model.md (FR-012)', () => {
  const doc = read('specs/066-voice-assistant/data-model.md')
  assert.match(
    doc,
    new RegExp(
      `\`assistant\\.pauseMs\`\\s*\\|\\s*int ${PAUSE_MS.min}–${PAUSE_MS.max}\\s*\\|\\s*\`${PAUSE_MS.default}\``,
    ),
  )
  assert.equal(parsePause(null), PAUSE_MS.default)
  assert.equal(parsePause('2200'), 2200)
  assert.equal(parsePause('4000'), PAUSE_MS.default)
  assert.equal(parsePause('1,5'), PAUSE_MS.default)
})

test('the page "Lizenzen" credits the voices and speech models (T032)', () => {
  const view = read('src/components/settings/LicensesView.vue')
  assert.match(view, /creditsAsync\(\)/)
  for (const kind of ['voice', 'speechModel', 'recognitionModel']) {
    for (const locale of ['de', 'en'] as const) {
      const label = localeText(locale, `settings.licenses.speech.${kind}`)
      assert.ok(typeof label === 'string' && label !== '', `${locale}: ${kind}`)
    }
  }
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

test('the preview page sits under KI → Sprache with its label', () => {
  const location = SETTINGS_LOCATIONS.find(
    (l) => l.id === 'ai.speech.assistant',
  )
  assert.ok(location, 'ai.speech.assistant')
  assert.equal(location.pattern, 'ai/speech/assistant')
  assert.equal(parsePreview('true'), true)
  assert.equal(parsePreview('false'), false)
  assert.equal(parsePreview(null), false, 'off by default')
  assert.ok(
    read('src/components/wm/appRoutes.ts').includes(
      'components/settings/assistant/AssistantSetting.vue',
    ),
  )
})

test('the indicator has a look for every phase and keeps its position on the desktop', () => {
  assert.equal(ASSISTANT_OFF.phase, 'off')
  const phases = Object.keys(PHASE_ICONS)
  assert.deepEqual(phases.sort(), [
    'awaitingAnswer',
    'followUp',
    'listening',
    'off',
    'ready',
    'speaking',
    'thinking',
    'working',
  ])
  for (const phase of phases) {
    for (const locale of ['de', 'en'] as const) {
      const text = localeText(locale, `assistant.phase.${phase}`)
      assert.ok(typeof text === 'string' && text !== '', `${locale}: ${phase}`)
    }
  }
  assert.deepEqual(parseBubblePosition('{"x":0.2,"y":0.4}'), { x: 0.2, y: 0.4 })
  assert.deepEqual(parseBubblePosition('{"x":3,"y":-1}'), { x: 1, y: 0 })
  assert.deepEqual(parseBubblePosition('nope'), BUBBLE_DEFAULT)
  assert.deepEqual(parseBubblePosition(null), BUBBLE_DEFAULT)
  assert.deepEqual(shareOf({ x: 50, y: 300 }, 100, 400), { x: 0.5, y: 0.75 })
  assert.deepEqual(shareOf({ x: 50, y: 50 }, 0, 0), BUBBLE_DEFAULT)
})

test('an interrupted answer splits where haex stopped speaking', () => {
  assert.deepEqual(splitSpoken('Hallo Jonas. Das fällt aus.', 12), {
    spoken: 'Hallo Jonas.',
    unspoken: ' Das fällt aus.',
  })
  assert.deepEqual(splitSpoken('Grüße', null), {
    spoken: 'Grüße',
    unspoken: '',
  })
  assert.deepEqual(splitSpoken('Grüße', 99), { spoken: 'Grüße', unspoken: '' })
  assert.deepEqual(lastEntries([1, 2, 3, 4, 5, 6, 7, 8]), [3, 4, 5, 6, 7, 8])
})

test('transcript entries keep the backend order and the window gives its UTC offset', () => {
  const entry = (id: string, seq: number) => ({
    id,
    threadId: 't',
    seq,
    spokenAt: 0,
    speaker: 'user',
    kind: 'utterance',
    text: id,
    spokenUntil: null,
    outcome: null,
    refId: null,
    createdAt: 0,
  })
  assert.deepEqual(
    sortEntries([entry('b', 2), entry('c', 1), entry('a', 2)]).map((e) => e.id),
    ['c', 'a', 'b'],
  )
  // Berlin in winter is one hour ahead of UTC: getTimezoneOffset says -60.
  assert.equal(utcOffsetMinutes({ getTimezoneOffset: () => -60 } as Date), 60)
})
