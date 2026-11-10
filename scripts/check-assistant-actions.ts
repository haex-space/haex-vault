// Spec 066 T050 (operator 2026-11-02: one toggle, no further buttons or shortcut): the voice
// assistant has exactly one action, a guardrail no agent can call, without default keys, with a
// title in German and English and a global handler. T144: making, importing and deleting own
// voices is no action, agent tool or extension API (FR-050, FR-051).
import assert from 'node:assert/strict'
import { readdirSync, readFileSync, statSync } from 'node:fs'
import { join, relative, resolve } from 'node:path'
import { test } from 'node:test'

import { ASSISTANT_ACTIONS } from '../src/lib/actions/assistantActions.ts'
import { ALL_ACTIONS } from '../src/lib/actions/catalog.ts'

const root = resolve(import.meta.dirname, '..')
const read = (path: string) => readFileSync(resolve(root, path), 'utf8')

function localeText(locale: 'de' | 'en', key: string): unknown {
  let node: unknown = JSON.parse(read(`src/i18n/locales/${locale}.json`))
  for (const part of key.split('.')) {
    node = (node as Record<string, unknown> | undefined)?.[part]
  }
  return node
}

test('the assistant has exactly its toggle, in the catalog', () => {
  assert.deepEqual(
    ALL_ACTIONS.filter((a) => a.id.startsWith('assistant.')).map((a) => a.id),
    ['assistant.toggle'],
  )
  assert.deepEqual(
    ASSISTANT_ACTIONS.map((a) => a.id),
    ['assistant.toggle'],
  )
})

test('the toggle is a guardrail no agent can call, without a shortcut', () => {
  const [toggle] = ASSISTANT_ACTIONS
  assert.equal(toggle?.scope, 'guardrails')
  assert.equal(toggle?.agentCallable, false)
  assert.equal(toggle?.defaultKeys, undefined)
})

test('the toggle has a German and an English title and a global handler', () => {
  for (const action of ASSISTANT_ACTIONS) {
    for (const locale of ['de', 'en'] as const) {
      const title = localeText(locale, action.titleKey)
      assert.ok(
        typeof title === 'string' && title.trim() !== '',
        `${locale}: ${action.id}`,
      )
    }
  }
  assert.ok(
    read('src/stores/assistantActionHandlers.ts').includes(
      "registerGlobalActionHandler('assistant.toggle'",
    ),
  )
  assert.match(
    read('src/plugins/actions.client.ts'),
    /registerAssistantActionHandlers\(/,
  )
})

/** Every `.ts`, `.vue` and `.rs` file below `dir`, relative to the repository. */
function sources(dir: string): string[] {
  const out: string[] = []
  for (const name of readdirSync(resolve(root, dir))) {
    const path = join(dir, name)
    if (statSync(resolve(root, path)).isDirectory()) out.push(...sources(path))
    else if (/\.(ts|vue|rs)$/.test(name)) out.push(path)
  }
  return out
}

/** The commands only the user may run, from the settings page (FR-050, FR-051, and enrolling or
 * removing the people haex listens to, FR-016a). */
const USER_ONLY =
  /tts_clone_|tts_voice_delete|tts_model_import|assistant_speaker_/
/** Their wrappers in `useTtsVoices`, which the window reaches them through. */
const USER_ONLY_WRAPPERS =
  /\bclone\w*Async\b|\bvoiceDeleteAsync\b|\bmodelImportAsync\b/

test('no action, agent tool or extension API makes, imports or deletes a voice (T144)', () => {
  const guarded = [
    'src/lib/actions',
    'src/stores',
    'src-tauri/src/chat',
    'src-tauri/src/mcp_server',
    'src-tauri/src/extensions',
    'src-tauri/src/agent_browser',
  ].flatMap(sources)
  assert.ok(guarded.length > 50, 'the guarded places were read')
  for (const file of guarded) {
    const name = relative(root, resolve(root, file))
    assert.doesNotMatch(read(file), USER_ONLY, name)
    assert.doesNotMatch(read(file), USER_ONLY_WRAPPERS, name)
  }
  // In the window only the voices composable calls them, and only the settings page uses its
  // wrappers.
  const callers = sources('src').filter((file) => USER_ONLY.test(read(file)))
  assert.deepEqual(callers.sort(), [
    'src/composables/useAssistantSpeakers.ts',
    'src/composables/useTtsVoices.ts',
  ])
  const speakerComposableCallers = sources('src').filter(
    (file) =>
      file !== 'src/composables/useAssistantSpeakers.ts' &&
      /\buseAssistantSpeakers\s*\(/.test(read(file)),
  )
  assert.deepEqual(speakerComposableCallers.sort(), [
    'src/components/settings/assistant/SpeakerEnrolDialog.vue',
    'src/components/settings/assistant/SpeakersSetting.vue',
  ])
  const wrapperCallers = sources('src').filter((file) =>
    USER_ONLY_WRAPPERS.test(read(file)),
  )
  assert.deepEqual(wrapperCallers.sort(), [
    'src/components/settings/assistant/SpeakerEnrolDialog.vue',
    'src/components/settings/assistant/VoiceCloneDialog.vue',
    'src/components/settings/assistant/VoiceSetting.vue',
    'src/composables/useTtsVoices.ts',
  ])
})
