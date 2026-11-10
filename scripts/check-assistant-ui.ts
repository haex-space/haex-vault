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
  CONVERSATION_LANGUAGE_KEY,
  CONVERSATION_LANGUAGES,
  parseConversationLanguage,
  spokenLanguage,
  PAUSE_MS,
  parsePause,
  parseContinueMins,
  continueMinsChoices,
  CONTINUE_MINS,
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
  PHASE_ICONS,
  restoreWanted,
  setupPath,
  SPINNING_PHASES,
} from '../src/lib/assistant/indicator.ts'
import { ASSISTANT_OFF, utcOffsetMinutes } from '../src/lib/assistant/wire.ts'
import {
  nextReadAloud,
  READ_ALOUD_TEXTS,
  READ_ALOUD_WORDS,
} from '../src/lib/assistant/readAloud.ts'
import { VOICE_THREAD_ICON } from '../src/lib/chat/threadOrigin.ts'
import {
  LABELLED_KINDS,
  OUTCOMES,
  SPEAKERS,
  compareEntries,
  mergeEntry,
  mergeStored,
  splitSpoken,
  type TranscriptEntry,
} from '../src/lib/assistant/transcript.ts'
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
    prefs: [
      'assistant.language',
      'assistant.pauseMs',
      'assistant.continueMins',
      'assistant.voice.<sprache>',
      'assistant.speechRate',
    ],
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
  // "Stimme" is part of the assistant page (operator 2026-11-03), opened at the conversation language.
  assert.match(
    read('src/components/settings/assistant/AssistantSetting.vue'),
    /<SettingsAssistantVoiceSetting\s+:language="spokenLanguage\(conversationLanguage, locale\)"/,
  )
  const voice = read('src/components/settings/assistant/VoiceSetting.vue')
  assert.match(voice, /kind: 'device'/)
  // Each voice is its own row with its own download; no language tabs, no speech model row.
  assert.doesNotMatch(voice, /ShadcnTabs/)
  assert.match(voice, /v-for="voice in shownVoices"/)
  assert.match(voice, /settings-voice-download-\$\{voice\.ref\}/)
  // The download of a voice is its model's: its progress shows in the speech model row, not
  // behind every voice (operator 2026-11-06).
  assert.doesNotMatch(voice, /ModelsDownloadBar|ModelsDownloadStatus/)
  assert.match(
    read('src/components/settings/assistant/SpeechModelRow.vue'),
    /<ModelsDownloadBar :model-id="option\.id" \/>/,
  )
  // The voices stand where the list of what is missing was; that list is gone (operator
  // 2026-11-03), and a voice that is not here stays choosable, asking for its download.
  assert.ok(
    voice.indexOf('v-for="voice in shownVoices"') <
      voice.indexOf("assistantLabelKey('assistant.speechRate')"),
    'the voices come before the speaking rate',
  )
  assert.doesNotMatch(voice, /:disabled="!voice\.installed"/)
  assert.match(voice, /voicePrefKey\(language\)/)
  assert.match(voice, /SPEECH_RATE_KEY/)
  const assistantPage = read(
    'src/components/settings/assistant/AssistantSetting.vue',
  )
  assert.match(assistantPage, /CONVERSATION_LANGUAGE_KEY/)
  assert.doesNotMatch(assistantPage, /assistant_readiness/)
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
  // Out of range is clamped, as the Rust side does, so the slider shows the pause in use.
  assert.equal(parsePause('4000'), PAUSE_MS.max)
  assert.equal(parsePause('300'), PAUSE_MS.min)
  assert.equal(parsePause('1,5'), PAUSE_MS.default)
  assert.equal(parsePause(''), PAUSE_MS.default)
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

test('the assistant page sits under KI → Sprache', () => {
  const location = SETTINGS_LOCATIONS.find(
    (l) => l.id === 'ai.speech.assistant',
  )
  assert.ok(location, 'ai.speech.assistant')
  assert.equal(location.pattern, 'ai/speech/assistant')
  assert.ok(
    read('src/components/wm/appRoutes.ts').includes(
      'components/settings/assistant/AssistantSetting.vue',
    ),
  )
})

test('the toggle shows every phase with an icon and a text', () => {
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
  assert.equal(PHASE_ICONS.off, 'lucide:mic-off', 'off is visibly off')
  assert.deepEqual([...SPINNING_PHASES], ['thinking', 'working'])
})

test('the toggle sits in the dock and the launcher, not in a floating indicator', () => {
  assert.ok(
    !existsSync(resolve(root, 'src/components/assistant/AssistantBubble.vue')),
  )
  assert.ok(!read('src/components/wm/Desktop.vue').includes('AssistantBubble'))
  assert.match(read('src/components/wm/Dock.vue'), /withAssistant\(/)
  assert.match(read('src/lib/wm/dock.ts'), /id: 'assistant'/)
  assert.match(
    read('src/components/wm/DockItem.vue'),
    /useAction\('assistant.toggle'\)/,
  )
  assert.match(
    read('src/components/wm/Launcher.vue'),
    /useAction\('assistant.toggle'\)/,
  )
})

test('a toggle that is not set up leads to the page of what is missing', () => {
  assert.equal(setupPath([]), null)
  assert.equal(setupPath(['sttModel', 'voice']), '/ai/speech')
  assert.equal(setupPath(['voice']), '/ai/speech/assistant')
  assert.equal(setupPath(['wakeWordModel']), '/ai/speech/assistant')
  assert.equal(setupPath(['chatModel']), '/ai/local')
  assert.equal(setupPath(['microphone']), null, 'no page sets up a microphone')
  assert.equal(setupPath(['microphone', 'voice']), '/ai/speech/assistant')
  for (const path of ['/ai/speech', '/ai/speech/assistant', '/ai/local']) {
    assert.ok(
      SETTINGS_LOCATIONS.some((l) => `/${l.pattern}` === path),
      `${path} is a settings page`,
    )
  }
  for (const item of [
    'title',
    'sttModel',
    'vadModel',
    'wakeWordModel',
    'voice',
    'chatModel',
    'microphone',
  ]) {
    for (const locale of ['de', 'en'] as const) {
      const text = localeText(locale, `assistant.missing.${item}`)
      assert.ok(typeof text === 'string' && text !== '', `${locale}: ${item}`)
    }
  }
})

test('after unlock the assistant comes back on only when it was on', () => {
  assert.equal(restoreWanted('true', false), true)
  assert.equal(restoreWanted('true', true), false, 'already on')
  assert.equal(restoreWanted('false', false), false)
  assert.equal(restoreWanted(null, false), false, 'off by default')
})

test('the first switching on asks to confirm the notice', () => {
  for (const key of ['title', 'body', 'confirm', 'cancel']) {
    for (const locale of ['de', 'en'] as const) {
      const text = localeText(locale, `assistant.notice.${key}`)
      assert.ok(typeof text === 'string' && text !== '', `${locale}: ${key}`)
    }
  }
  assert.match(
    read('src/components/wm/Desktop.vue'),
    /<AssistantNoticeDialog \/>/,
  )
  assert.match(
    read('src/stores/assistantActionHandlers.ts'),
    /confirmNoticeAsync\(\)/,
  )
})

test('the continuation time reads like the backend and offers what is stored', () => {
  assert.equal(parseContinueMins(null), CONTINUE_MINS.default)
  assert.equal(parseContinueMins('0'), 0, 'always a new conversation')
  assert.equal(parseContinueMins(' 15 '), 15)
  assert.equal(parseContinueMins('90'), 60, 'clamped like Timing::from_prefs')
  assert.equal(parseContinueMins('-3'), 0)
  assert.equal(parseContinueMins('2.5'), CONTINUE_MINS.default)
  assert.deepEqual(continueMinsChoices(5), [0, 1, 2, 5, 10, 15, 30, 60])
  assert.deepEqual(continueMinsChoices(7), [0, 1, 2, 5, 7, 10, 15, 30, 60])
  for (const key of ['continueHint', 'continueNever', 'minutes']) {
    for (const locale of ['de', 'en'] as const) {
      const text = localeText(locale, `settings.assistantPage.${key}`)
      assert.ok(typeof text === 'string' && text !== '', `${locale}: ${key}`)
    }
  }
})

test('the window gives its UTC offset', () => {
  // Berlin in winter is one hour ahead of UTC: getTimezoneOffset says -60.
  assert.equal(utcOffsetMinutes({ getTimezoneOffset: () => -60 } as Date), 60)
})

test('a spoken conversation is marked in "Verlauf" and in the session header (T051)', () => {
  assert.match(
    read('src-tauri/src/chat/thread_commands.rs'),
    /pub origin: Option<ThreadOrigin>/,
    'list_threads hands the origin to the window',
  )
  assert.match(read('src/composables/useChat.ts'), /origin: 'voice' \| null/)
  assert.match(VOICE_THREAD_ICON, /^lucide:/)
  for (const [file, guard] of [
    [
      'src/components/chat/AgentSessionsPopover.vue',
      "thread.origin === 'voice'",
    ],
    ['src/components/chat/AgentHeader.vue', "session?.origin === 'voice'"],
  ] as const) {
    const source = read(file)
    assert.ok(source.includes(`v-if="${guard}"`), file)
    assert.ok(source.includes(':name="VOICE_THREAD_ICON"'), file)
    assert.ok(source.includes(':aria-label="t(\'chat.threads.voice\')"'), file)
  }
  assert.equal(localeText('de', 'chat.threads.voice'), 'Sprachgespräch')
  assert.equal(localeText('en', 'chat.threads.voice'), 'Voice conversation')
})

function rustTextEnum(name: string): string[] {
  const source = read('src-tauri/src/storage/voice_transcript.rs')
  const block = source.slice(source.indexOf(`    ${name} {`))
  const body = block.slice(0, block.indexOf('\n    }\n'))
  return [...body.matchAll(/=> "([a-zA-Z]+)",/g)].map((match) => match[1]!)
}

test('every kind, outcome and speaker of the transcript has a text (T127)', () => {
  const kinds = rustTextEnum('EntryKind')
  assert.ok(kinds.length >= 13, 'kinds read from voice_transcript.rs')
  assert.deepEqual(
    [...kinds].sort(),
    [...LABELLED_KINDS, 'utterance', 'answer'].sort(),
    'every kind is labelled except plain utterances and answers',
  )
  assert.deepEqual(rustTextEnum('Outcome'), [...OUTCOMES])
  assert.deepEqual(rustTextEnum('Speaker'), [...SPEAKERS])
  for (const locale of ['de', 'en'] as const) {
    for (const kind of LABELLED_KINDS)
      assert.equal(
        typeof localeText(locale, `assistant.kind.${kind}`),
        'string',
        `${locale} ${kind}`,
      )
    for (const outcome of OUTCOMES)
      assert.equal(
        typeof localeText(locale, `assistant.outcome.${outcome}`),
        'string',
        `${locale} ${outcome}`,
      )
    for (const speaker of SPEAKERS)
      assert.equal(
        typeof localeText(locale, `assistant.speaker.${speaker}`),
        'string',
        `${locale} ${speaker}`,
      )
    for (const key of ['title', 'empty', 'unspoken', 'loadFailed'])
      assert.equal(
        typeof localeText(locale, `assistant.transcript.${key}`),
        'string',
        `${locale} ${key}`,
      )
    for (const key of ['searchTranscripts', 'noHits', 'moreHits'])
      assert.equal(
        typeof localeText(locale, `chat.threads.${key}`),
        'string',
        `${locale} ${key}`,
      )
  }
})

test('an interrupted answer splits where haex stopped, by characters', () => {
  assert.deepEqual(splitSpoken('Morgen regnet es.', null), {
    spoken: 'Morgen regnet es.',
    unspoken: '',
  })
  assert.deepEqual(splitSpoken('Grüße 👋 dich', 7), {
    spoken: 'Grüße 👋',
    unspoken: ' dich',
  })
  assert.deepEqual(splitSpoken('kurz', 99), { spoken: 'kurz', unspoken: '' })
})

test('live entries join the stored ones in the order of the table', () => {
  const entry = (id: string, seq: number, text = id): TranscriptEntry => ({
    id,
    threadId: 't',
    seq,
    spokenAt: 0,
    speaker: 'user',
    kind: 'utterance',
    text,
    spokenUntil: null,
    outcome: null,
    refId: null,
    createdAt: 0,
  })
  let entries = [entry('b', 2), entry('a', 1)].reduce<TranscriptEntry[]>(
    mergeEntry,
    [],
  )
  entries = mergeEntry(entries, entry('c', 2))
  entries = mergeEntry(entries, entry('a', 1, 'neu'))
  assert.deepEqual(
    entries.map((e) => `${e.seq}${e.id}:${e.text}`),
    ['1a:neu', '2b:b', '2c:c'],
  )
  assert.equal(compareEntries(entry('x', 3), entry('y', 3)) < 0, true)

  // A fresh read wins for its ids; an entry that arrived live while reading stays.
  const shown = [entry('a', 1, 'alt'), entry('live', 3)]
  const stored = [entry('b', 2), entry('a', 1, 'gelesen')]
  assert.deepEqual(
    mergeStored(shown, stored).map((e) => `${e.seq}${e.id}:${e.text}`),
    ['1a:gelesen', '2b:b', '3live:live'],
  )
  assert.ok(
    read('src/components/chat/VoiceTranscript.vue').includes(
      'mergeStored(entries.value, stored)',
    ),
    'the view merges a read in one pass',
  )
})

test('the chat shows the transcript of a spoken session and searches transcripts (T127)', () => {
  assert.match(
    read('src-tauri/src/lib.rs'),
    /assistant::transcript::assistant_transcript_search,/,
  )
  const app = read('src/components/apps/ChatApp.vue')
  assert.ok(
    app.includes('v-if="transcriptOpen && activeIsVoice && activeThreadId"'),
    'only a spoken session shows a transcript',
  )
  assert.ok(app.includes('v-model:transcript="transcriptOpen"'))
  assert.ok(app.includes('@open-transcript="openTranscript"'))
  const header = read('src/components/chat/AgentHeader.vue')
  assert.match(header, /v-if="session\?\.origin === 'voice'"\s+variant="ghost"/)
  assert.match(header, /:aria-pressed="transcript"/)
  const view = read('src/components/chat/VoiceTranscript.vue')
  assert.ok(view.includes("'assistant_transcript_list'"))
  assert.ok(view.includes("'assistant-transcript'"))
  assert.ok(view.includes("onVaultTablesChanged(['voice_transcript_entries']"))
  assert.ok(
    read('src/composables/useTranscriptSearch.ts').includes(
      "'assistant_transcript_search'",
    ),
  )
  assert.ok(
    read('src/components/chat/AgentSessionsPopover.vue').includes(
      'useTranscriptSearch()',
    ),
  )
})

test('conversations take the interface language unless this device picks another (FR-002a)', () => {
  assert.deepEqual([...CONVERSATION_LANGUAGES], ['ui', ...SPEECH_LANGUAGES])
  assert.equal(parseConversationLanguage(null), 'ui')
  assert.equal(parseConversationLanguage('klingonisch'), 'ui')
  assert.equal(parseConversationLanguage('fr'), 'fr')
  assert.equal(spokenLanguage('ui', 'de'), 'de')
  assert.equal(spokenLanguage('ui', 'en-US'), 'en')
  assert.equal(spokenLanguage('fr', 'de'), 'fr')
  // The Rust side reads the same key and the same codes.
  const rust = read('src-tauri/src/chat/reply_language.rs')
  assert.ok(rust.includes(`"${CONVERSATION_LANGUAGE_KEY}"`))
  for (const code of SPEECH_LANGUAGES)
    assert.ok(rust.includes(`"${code}" => Self::`), code)
  for (const locale of ['de', 'en'] as const) {
    assert.equal(
      typeof localeText(locale, 'settings.assistantPage.languageUi'),
      'string',
    )
    assert.equal(
      typeof localeText(locale, 'settings.assistantPage.languageHint'),
      'string',
    )
  }
})

test('own voices are made, heard and deleted on the assistant page (T149)', () => {
  const voice = read('src/components/settings/assistant/VoiceSetting.vue')
  assert.match(voice, /<SettingsAssistantVoiceCloneDialog/)
  assert.match(voice, /data-testid="settings-voice-clone"/)
  assert.match(voice, /listen\('tts-voices-changed'/)
  assert.match(voice, /v-if="voice\.kind === 'clone'"/)
  const dialog = read('src/components/settings/assistant/VoiceCloneDialog.vue')
  // Saving needs a passed check, the consent and a name (FR-050).
  assert.match(
    dialog,
    /check\.value\?\.ok === true &&\s+consent\.value &&\s+name\.value\.trim\(\) !== ''/,
  )
  assert.match(dialog, /data-testid="voice-clone-consent"/)
  assert.match(dialog, /'tts-clone-level'/)
  const rust = read('src-tauri/src/tts/clone.rs')
  const problems = [...rust.matchAll(/^\s{4}(Too\w+|Clipped|Noisy),$/gm)].map(
    (m) => m[1]!.charAt(0).toLowerCase() + m[1]!.slice(1),
  )
  assert.deepEqual(problems, [
    'tooShort',
    'tooLong',
    'tooQuiet',
    'clipped',
    'noisy',
  ])
  const reasons = [
    ...read('src-tauri/src/tts/clone_commands.rs').matchAll(
      /(?:invalid\(|Err\()"([a-z][A-Za-z]+)"\)/g,
    ),
  ].map((m) => m[1]!)
  assert.ok(
    reasons.includes('consentRequired'),
    'reasons read from the commands',
  )
  for (const locale of ['de', 'en'] as const) {
    for (const problem of problems)
      assert.equal(
        typeof localeText(locale, `settings.voice.clone.problems.${problem}`),
        'string',
        `${locale} ${problem}`,
      )
    for (const reason of reasons.filter(
      (r) => r !== 'builtinVoice' && r !== 'unknownVoice',
    ))
      assert.equal(
        typeof localeText(locale, `settings.voice.clone.errors.${reason}`),
        'string',
        `${locale} ${reason}`,
      )
  }
})

test('the assistant pages save silently and show failures as toasts (operator 2026-11-03)', () => {
  for (const file of [
    'src/components/settings/assistant/AssistantSetting.vue',
    'src/components/settings/assistant/VoiceSetting.vue',
    'src/components/settings/assistant/TonesSetting.vue',
    'src/components/settings/assistant/VoiceCloneDialog.vue',
  ]) {
    const source = read(file)
    assert.doesNotMatch(source, /savedFlash|role="status"/, file)
    assert.match(source, /toast\.error\(/, file)
  }
  // The dialog has no page to load: none of its failures stays in it.
  assert.doesNotMatch(
    read('src/components/settings/assistant/VoiceCloneDialog.vue'),
    /role="alert"/,
  )
})

test('an own voice offers a text to read aloud in every language (operator 2026-11-03)', () => {
  for (const language of SPEECH_LANGUAGES) {
    const texts = READ_ALOUD_TEXTS[language]
    assert.ok(texts.length >= 5, `${language}: at least five texts`)
    assert.equal(
      new Set(texts).size,
      texts.length,
      `${language}: no text twice`,
    )
    for (const text of texts) {
      const words = text.split(/\s+/).filter(Boolean).length
      assert.ok(
        words >= READ_ALOUD_WORDS.min && words <= READ_ALOUD_WORDS.max,
        `${language}: ${words} words in "${text.slice(0, 30)}…"`,
      )
    }
  }
  // "Anderer Text" never offers the same text again.
  for (let current = 0; current < 5; current++)
    for (const draw of [0, 0.3, 0.6, 0.99])
      assert.notEqual(
        nextReadAloud('de', current, () => draw),
        current,
      )
  assert.equal(
    nextReadAloud('de', null, () => 0.99),
    4,
  )
  const dialog = read('src/components/settings/assistant/VoiceCloneDialog.vue')
  assert.ok(
    dialog.indexOf('data-testid="voice-clone-language"') <
      dialog.indexOf('data-testid="voice-clone-text"') &&
      dialog.indexOf('data-testid="voice-clone-text"') <
        dialog.indexOf('data-testid="voice-clone-record"'),
    'language, then the text, then recording',
  )
})

test('the onboarding ends with the voice, so the assistant speaks right away', () => {
  // Operator 2026-11-06 (spec 081 FR-012a): Pocket TTS is too big to ship in the installer.
  const page = read('src/pages/onboarding/[instance].vue')
  assert.match(page, /<OnboardingVoiceStep/)
  assert.match(
    page,
    /@skip="afterSttAsync"/,
    'skipping recognition still offers the voice',
  )
  assert.match(
    page,
    /async function afterSttAsync\(\) \{\s*await voiceOptionsLoaded/,
    'waits for the voice catalog before finishing onboarding',
  )
  assert.match(page, /voiceOption\.value && !voiceOption\.value\.installed/)
  const step = read('src/components/onboarding/VoiceStep.vue')
  assert.match(step, /<ModelsDownloadConfirmStep/, 'asks before the download')
  assert.match(step, /emit\('skip'\)/)
})
