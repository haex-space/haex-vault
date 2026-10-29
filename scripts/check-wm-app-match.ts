// Part of `pnpm check:wm-navigation` (spec 046-agent-choice-prompt, US1, FR-001–FR-004, research R5):
// which app a name, a misspelled name or a guessed id opens (src/lib/wm/appMatch.ts), and when the
// answer is a choice instead.
import assert from 'node:assert/strict'
import { test } from 'node:test'

import type { ExtensionSummary } from '../src/types/bindings/ExtensionSummary.ts'
import { allApps, extensionApps } from '../src/lib/extensions/apps.ts'
import { matchApp, type AppMatch } from '../src/lib/wm/appMatch.ts'
import type { AppDefinition } from '../src/lib/wm/apps.ts'

const GERMAN: Record<string, string> = {
  'wm.apps.chat': 'Chat',
  'wm.apps.settings': 'Einstellungen',
  'wm.apps.passwords': 'Passwörter',
  'wm.apps.files': 'Dateien',
}
const titleOf = (app: AppDefinition) =>
  app.title ?? GERMAN[app.titleKey] ?? app.titleKey

function summary(
  id: string,
  title: string,
  patch: Partial<ExtensionSummary> = {},
): ExtensionSummary {
  return {
    id,
    name: title,
    title,
    publisherFingerprint: 'aaaa',
    enabled: true,
    state: 'installed',
    singleInstance: false,
    hasIcon: false,
    devices: [],
    dev: false,
    openers: [],
    ...patch,
  }
}

const MAIL = 'extension.3103c350-174b-52c3-af42-13ad66ba4a93'
const NOTES = 'extension.2b40421d-3518-542a-b649-f3149e8c7665'
const FILES = 'extension.b5101ab7-a04f-58c7-817d-c6f1f95ba512'

const APPS = allApps(
  extensionApps(
    [
      summary(MAIL.slice('extension.'.length), 'haex-mail'),
      summary(NOTES.slice('extension.'.length), 'haex-notes'),
      summary(FILES.slice('extension.'.length), 'haex-files', {
        statusHere: 'transferring',
      }),
    ],
    {},
  ),
)

const match = (input: string, apps: readonly AppDefinition[] = APPS) =>
  matchApp(input, apps, titleOf)

function exact(appId: string, at: string | null = null): AppMatch {
  return { kind: 'exact', appId, at }
}

function candidateIds(result: AppMatch): string[] {
  assert.equal(
    result.kind,
    'choice',
    `expected a choice, got ${JSON.stringify(result)}`,
  )
  return result.kind === 'choice' ? result.candidates.map((c) => c.appId) : []
}

test('an app id opens that app', () => {
  assert.deepEqual(match('system.chat'), exact('system.chat'))
  assert.deepEqual(match(MAIL), exact(MAIL))
})

test('a replaced app id opens its replacement at the alias location', () => {
  assert.deepEqual(
    match('system.federation'),
    exact('system.settings', '/federation'),
  )
})

test('the name of an app opens it, in any case', () => {
  assert.deepEqual(match('haex-mail'), exact(MAIL))
  assert.deepEqual(match('HAEX-MAIL'), exact(MAIL))
  assert.deepEqual(match('einstellungen'), exact('system.settings'))
  assert.deepEqual(match('Passworter'), exact('system.passwords'))
})

test('the english name of a holzi app is the rest of its id', () => {
  assert.deepEqual(match('settings'), exact('system.settings'))
  assert.deepEqual(match('passwords'), exact('system.passwords'))
})

test('a typo in a name still opens the app without a question', () => {
  assert.deepEqual(match('haex-mial'), exact(MAIL))
  assert.deepEqual(match('haex-ntoes'), exact(NOTES))
  assert.deepEqual(match('Einstelungen'), exact('system.settings'))
})

test('a guessed id with a wrong prefix finds the app by its name', () => {
  assert.deepEqual(match('system.notes'), exact(NOTES))
  assert.deepEqual(match('system.mail'), exact(MAIL))
})

test('a name that fits several apps equally well asks instead of opening one', () => {
  assert.deepEqual(
    candidateIds(match('haex')).sort(),
    [FILES, MAIL, NOTES].sort(),
  )
})

test('a name that fits nothing well asks with at most five candidates', () => {
  assert.ok(candidateIds(match('Kalender')).length <= 5)
})

test('candidates come best first and never more than five', () => {
  const many = allApps(
    extensionApps(
      Array.from({ length: 8 }, (_, i) =>
        summary(`00000000-0000-0000-0000-00000000000${i}`, `haex-tool-${i}`),
      ),
      {},
    ),
  )
  const ids = candidateIds(match('haex-tool', many))
  assert.equal(ids.length, 5)

  const notes = allApps(
    extensionApps(
      [
        summary('00000000-0000-0000-0000-000000000001', 'Notes'),
        summary('00000000-0000-0000-0000-000000000002', 'Notes Pro'),
      ],
      {},
    ),
  )
  assert.deepEqual(candidateIds(match('note', notes)), [
    'extension.00000000-0000-0000-0000-000000000001',
    'extension.00000000-0000-0000-0000-000000000002',
  ])
})

test('a candidate that cannot open on this device carries its reason', () => {
  const result = match('haex')
  assert.equal(result.kind, 'choice')
  const files =
    result.kind === 'choice'
      ? result.candidates.find((c) => c.appId === FILES)
      : undefined
  assert.equal(files?.unavailableKey, 'extensions.status.transferring')
  assert.equal(files?.title, 'haex-files')
})

test('with forty extensions every app opens by its name and is found despite a typo', () => {
  const names = [
    'mail',
    'notes',
    'files',
    'calendar',
    'contacts',
    'draw',
    'code',
    'image',
    'tetris',
    'music',
    'video',
    'maps',
    'weather',
    'news',
    'tasks',
    'todo',
    'journal',
    'budget',
    'recipes',
    'fitness',
    'books',
    'podcasts',
    'chess',
    'sudoku',
    'translate',
    'clock',
    'timer',
    'scanner',
    'pdf',
    'terminal',
    'git',
    'wiki',
    'bookmarks',
    'feeds',
    'photos',
    'camera',
    'voice',
    'whiteboard',
    'spreadsheet',
    'slides',
  ]
  const apps = allApps(
    extensionApps(
      names.map((name, i) =>
        summary(
          `00000000-0000-0000-0000-${String(i).padStart(12, '0')}`,
          `haex-${name}`,
        ),
      ),
      {},
    ),
  )
  let clear = 0
  let typos = 0
  for (const app of apps) {
    const title = titleOf(app)
    assert.deepEqual(match(title, apps), exact(app.id), title)
    assert.deepEqual(match(title.toUpperCase(), apps), exact(app.id), title)

    // Swap two neighbouring letters of the name after the prefix, where there are two.
    const at = title.startsWith('haex-') ? 6 : 1
    if (title.length < at + 2) continue
    const typo =
      title.slice(0, at) + title[at + 1] + title[at] + title.slice(at + 2)
    if (typo === title) continue
    typos++
    const result = match(typo, apps)
    if (result.kind === 'exact') {
      assert.equal(result.appId, app.id, `${typo} opened another app`)
      clear++
    } else {
      assert.ok(
        result.candidates.some((c) => c.appId === app.id),
        `${typo} does not offer ${title}`,
      )
    }
  }
  assert.ok(
    clear / typos >= 0.9,
    `only ${clear} of ${typos} typos open directly`,
  )
})
