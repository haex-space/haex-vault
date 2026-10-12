// Part of `pnpm check:extensions`: how the install dialog and the settings group and label the tools
// an extension offers to the agent (src/lib/extensions/agentTools.ts, spec 018).
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  effectLabelKey,
  splitAgentTools,
  toolTitle,
} from '../src/lib/extensions/agentTools.ts'

const web = { kind: 'web', action: '*', target: 'https://a.example/*' }
const count = {
  kind: 'agentTool',
  action: 'read',
  target: 'count_unread',
  titleDe: 'Ungelesene zählen',
  titleEn: 'Count unread',
}
const archive = {
  kind: 'agentTool',
  action: 'change',
  target: 'archive_mail',
  titleDe: 'Mail archivieren',
  titleEn: 'Archive mail',
}

test('agent tools are split from the other permissions, which keep their order', () => {
  const { tools, others } = splitAgentTools([count, web, archive], 'de')
  assert.deepEqual(others, [web])
  assert.deepEqual(
    tools.map((tool) => tool.target),
    ['archive_mail', 'count_unread'],
  )
})

test('tools are sorted by their title in the shown locale', () => {
  const zeta = { ...count, titleDe: 'Zählen', titleEn: 'Archive all' }
  assert.deepEqual(
    splitAgentTools([zeta, archive], 'de').tools.map((t) => t.target),
    ['archive_mail', 'count_unread'],
  )
  assert.deepEqual(
    splitAgentTools([zeta, archive], 'en').tools.map((t) => t.target),
    ['count_unread', 'archive_mail'],
  )
})

test('the title follows the locale, English for any other, the name without titles', () => {
  assert.equal(toolTitle(count, 'de'), 'Ungelesene zählen')
  assert.equal(toolTitle(count, 'en'), 'Count unread')
  assert.equal(toolTitle(count, 'fr'), 'Count unread')
  assert.equal(
    toolTitle({ kind: 'agentTool', action: 'read', target: 'count' }, 'de'),
    'count',
  )
})

test('each effect has its label key', () => {
  assert.equal(effectLabelKey('read'), 'extensions.permissions.effect.read')
  assert.equal(
    effectLabelKey('destructive'),
    'extensions.permissions.effect.destructive',
  )
})
