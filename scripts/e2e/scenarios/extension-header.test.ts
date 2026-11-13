import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import {
  fixture,
  inFrame,
  install,
  openFromLauncher,
  probeRequest,
  type InstalledExtension,
} from '../lib/extensions.ts'
import { KEY, runAction, wmSnapshot } from '../lib/settings.ts'

// Spec 090-app-header-bar, US3 and US6 (contracts/extension-header.md): the probe extension puts
// controls into the header of its window and gets the events back; a refused description changes
// nothing; a label with markup is text; a reloaded frame loses its controls; each tab of a window
// shows its own header. (The development-version cases, an inline `data:` SVG with a script, and two
// windows of one extension are covered by the Rust tests of `bridge::header_set_tests` and a hand
// check: the rig opens no second window of a single-instance extension.)
const DESCRIPTION = {
  start: [
    {
      type: 'button',
      id: 'a',
      label: 'Add',
      icon: 'icons/plus.svg',
      monochrome: true,
    },
  ],
  center: [
    {
      type: 'search',
      id: 'q',
      label: 'Search',
      placeholder: 'Search…',
      value: '',
      collapse: { order: 10, form: 'shrink' },
    },
  ],
  end: [
    {
      type: 'choice',
      id: 'view',
      label: 'View',
      selected: 'list',
      options: [
        { id: 'list', label: 'List', icon: 'icons/plus.svg' },
        { id: 'grid', label: 'Grid', icon: 'icons/plus.svg' },
      ],
    },
    {
      type: 'menu',
      id: 'more',
      label: 'More',
      text: 'More',
      items: [
        { type: 'item', id: 'export', label: 'Export' },
        { type: 'separator' },
        { type: 'item', id: 'about', label: 'About' },
      ],
    },
    { type: 'toggle', id: 'pin', label: 'Pin', text: 'Pin', pressed: false },
  ],
}

type HeaderEvent = { id: string; kind: string; value?: unknown }

scenario('extension-header', { timeoutMs: 240_000 }, async (ctx) => {
  const group = await ctx.group({ users: { anna: ['laptop'] } })
  const page = group.device('anna/laptop').page
  const probe: InstalledExtension = await install(page, fixture('e2e', 'probe'))
  await openFromLauncher(page, probe)
  await ctx.waitFor(
    'the SDK channel of the probe',
    () =>
      inFrame<boolean>(
        page,
        probe,
        `return document.documentElement.dataset.probeReady === '1'`,
      ),
    { timeoutMs: 20_000 },
  )

  const events = () =>
    inFrame<HeaderEvent[]>(
      page,
      probe,
      `return window.probe.events
         .filter((e) => e.type === 'haextension:header:event')
         .map((e) => e.data)`,
    )
  const shown = (id: string) =>
    page.exec<boolean>(
      `return !!document.querySelector('[data-testid="wm-header"] [data-testid="wm-header-' + arguments[0] + '"]')`,
      [id],
    )

  // --- Every control shows, in its area -----------------------------------------------------
  const set = await probeRequest(
    page,
    probe,
    'extension_header_set',
    DESCRIPTION,
  )
  assert.equal(set.error, undefined, JSON.stringify(set.error))
  await ctx.waitFor('the controls in the header', () => shown('a'))
  for (const [id, area] of [
    ['a', 'start'],
    ['q', 'center'],
    ['view', 'end'],
    ['more', 'end'],
    ['pin', 'end'],
  ] as const) {
    const inArea = await page.exec<boolean>(
      `return !!document.querySelector('[data-testid="wm-header"] [data-area="' + arguments[1] + '"] [data-testid="wm-header-' + arguments[0] + '"]')`,
      [id, area],
    )
    assert.ok(inArea, `${id} is not in the ${area} area`)
  }
  const drawn = await page.exec<boolean>(
    `const img = document.querySelector('[data-testid="wm-header-view-list"] img')
     return !!img && img.src.startsWith('data:image/svg+xml')`,
  )
  assert.ok(drawn, 'the picture of the bundle is not drawn from its data URL')
  ctx.step('every control shows in its area, a bundle picture is drawn')

  // --- Events reach the frame ---------------------------------------------------------------
  await page.click('wm-header-a')
  await page.click('wm-header-pin')
  await page.click('wm-header-view-grid')
  await page.type('wm-header-q', 'abc')
  await page.type('wm-header-q', KEY.enter)
  await ctx.waitFor('the events in the frame', async () => {
    const seen = await events()
    return seen.some((e) => e.id === 'q' && e.kind === 'submit')
  })
  const seen = await events()
  assert.deepEqual(
    seen.find((e) => e.id === 'a'),
    { id: 'a', kind: 'click' },
  )
  assert.deepEqual(
    seen.find((e) => e.id === 'pin'),
    {
      id: 'pin',
      kind: 'toggle',
      value: true,
    },
  )
  assert.deepEqual(
    seen.find((e) => e.id === 'view'),
    {
      id: 'view',
      kind: 'select',
      value: 'grid',
    },
  )
  assert.deepEqual(
    seen.filter((e) => e.id === 'q').map((e) => [e.kind, e.value]),
    [
      ['input', 'abc'],
      ['submit', 'abc'],
    ],
  )
  ctx.step('click, toggle, select, merged input and submit arrive as events')

  // --- A new description replaces the old one, a typed search keeps its text -----------------
  await set_({
    ...DESCRIPTION,
    center: [{ ...DESCRIPTION.center[0]!, value: 'abc' }],
  })
  const echoed = await page.exec<string>(
    `return document.querySelector('[data-testid="wm-header-q"]').value`,
  )
  assert.equal(echoed, 'abc', 'the echo of the typed text changed the field')
  await set_({ end: [DESCRIPTION.end[2]!] })
  await ctx.waitFor('the old controls to go', async () => !(await shown('a')))
  assert.equal(await shown('pin'), true)
  ctx.step(
    'a second description replaces the first; an echo keeps the typed text',
  )

  // --- A menu with `active` shows a dot, without it none (spec 093, FR-008) -----------------
  const dotted = () =>
    page.exec<boolean>(
      `return !!document.querySelector('[data-testid="wm-header-more"] [data-testid="wm-header-dot"]')`,
    )
  const more = DESCRIPTION.end[1]!
  await set_({ end: [{ ...more, active: true }] })
  await ctx.waitFor('the dot on the menu', dotted)
  await set_({ end: [{ ...more, active: false }] })
  await ctx.waitFor('the dot to go', async () => !(await dotted()))
  ctx.step('an active menu shows a dot, an inactive one none')

  async function set_(description: unknown): Promise<void> {
    const answer = await probeRequest(
      page,
      probe,
      'extension_header_set',
      description,
    )
    assert.equal(answer.error, undefined, JSON.stringify(answer.error))
  }

  // --- A refused description changes nothing ------------------------------------------------
  const tooMany = {
    end: Array.from({ length: 33 }, (_, i) => ({
      type: 'button',
      id: `x${i}`,
      label: 'X',
      text: 'X',
    })),
  }
  const refused = await probeRequest(
    page,
    probe,
    'extension_header_set',
    tooMany,
  )
  assert.equal(refused.error?.code, 3001)
  assert.equal(
    await shown('pin'),
    true,
    'the old header went with the refused one',
  )
  assert.equal(await shown('x0'), false, 'a refused control showed')
  ctx.step('33 controls are refused with 3001 and the old header stays')

  // --- A label with markup is text ----------------------------------------------------------
  const markup = '<img src=x onerror="document.title=1">'
  await set_({
    end: [{ type: 'button', id: 'm', label: markup, text: markup }],
  })
  await ctx.waitFor('the markup button', () => shown('m'))
  const asText = await page.exec<{ text: string; images: number }>(
    `const button = document.querySelector('[data-testid="wm-header-m"]')
     return { text: button.textContent.trim(), images: button.querySelectorAll('img').length }`,
  )
  assert.equal(asText.text, markup)
  assert.equal(asText.images, 0)
  ctx.step('a label with markup is shown as that text')

  // --- Each tab shows its own header --------------------------------------------------------
  await set_(DESCRIPTION)
  await ctx.waitFor('the probe header', () => shown('a'))
  const [window] = (await wmSnapshot(page)).windows
  assert.ok(window)
  const probeTab = window.activeTabId
  const added = await runAction(page, 'wm.tab.new', {
    windowId: window.id,
    appId: 'system.settings',
  })
  assert.ok(added.ok, JSON.stringify(added))
  await ctx.waitFor('the settings header', async () => !(await shown('a')))
  await page.waitForDisplayed('settings-sidebar-toggle')
  const back = await runAction(page, 'wm.tab.activate', { tabId: probeTab })
  assert.ok(back.ok, JSON.stringify(back))
  await ctx.waitFor('the probe header again', () => shown('a'))
  ctx.step('two tabs of a window show their own headers when active')

  // --- A frame that reloads loses its controls ----------------------------------------------
  await inFrame(
    page,
    probe,
    `setTimeout(() => location.reload(), 0); return true`,
  )
  await ctx.waitFor(
    'the controls to go with the document',
    async () => !(await shown('a')),
  )
  // The new document opens its own SDK channel; a header restored once that is ready (a late event
  // of the old session) would show up now, so the absence is checked after it.
  await ctx.waitFor(
    'the SDK channel of the reloaded probe',
    () =>
      inFrame<boolean>(
        page,
        probe,
        `return document.documentElement.dataset.probeReady === '1'`,
      ),
    { timeoutMs: 20_000 },
  )
  assert.equal(await shown('a'), false, 'the old header came back after reload')
  ctx.step('a reloaded frame loses its controls')
})
