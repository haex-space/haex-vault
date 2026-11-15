import { describe, it } from 'node:test'
import assert from 'node:assert/strict'
import { formatPageState, PAGE_STATE_SCRIPT } from './page-state.ts'

describe('the page state', () => {
  it('is a script the page can parse and that answers with an object', () => {
    // The page runs the text as the body of a function; a typo in it would otherwise show up only
    // as "no page state" in a failure's material.
    const run = new Function(PAGE_STATE_SCRIPT)
    assert.equal(typeof run, 'function')
  })

  it('reads a page: address, focus, dialogs, ids and text', () => {
    const element = (attributes: Record<string, string>, text = '') => ({
      tagName: 'BUTTON',
      id: '',
      innerText: text,
      getAttribute: (name: string) => attributes[name] ?? null,
      getBoundingClientRect: () => ({ width: 10, height: 10 }),
    })
    const button = element({ 'data-testid': 'files-new' }, 'New')
    const dialog = element({ 'data-testid': 'sync-rule' }, '  Sync   rule ')
    const page = {
      location: { href: 'http://localhost/files' },
      document: {
        title: 'HaexVault',
        readyState: 'complete',
        activeElement: button,
        body: { innerText: 'Files\n  New' },
        querySelectorAll: (selector: string) =>
          selector.includes('dialog')
            ? [dialog]
            : selector === '[data-testid]'
              ? [button, dialog]
              : [],
      },
      getComputedStyle: () => ({ visibility: 'visible', display: 'block' }),
      innerWidth: 800,
      innerHeight: 600,
    }
    const run = new Function(...Object.keys(page), PAGE_STATE_SCRIPT) as (
      ...values: unknown[]
    ) => Parameters<typeof formatPageState>[0]
    const state = run(...Object.values(page))
    assert.equal(state.href, 'http://localhost/files')
    assert.equal(state.viewport, '800x600')
    assert.equal(state.focused, 'button[data-testid=files-new]')
    assert.deepEqual(state.dialogs, [
      'button[data-testid=sync-rule]: Sync rule',
    ])
    assert.deepEqual(state.testIds, ['files-new', 'sync-rule'])
    assert.equal(state.text, 'Files New')
  })

  it('formats every part, naming an empty list', () => {
    const text = formatPageState({
      href: 'http://localhost/',
      title: 'T',
      readyState: 'complete',
      viewport: '1x1',
      focused: 'nothing',
      dialogs: [],
      toasts: ['div: Saved'],
      testIds: [],
      text: '',
    })
    assert.match(text, /dialogs open:\n {2}none/)
    assert.match(text, /toasts shown:\n {2}- div: Saved/)
    assert.match(text, /visible test ids \(0, at most 150\):\n {2}none/)
    assert.match(text, /visible text:\n {2}none/)
  })
})
