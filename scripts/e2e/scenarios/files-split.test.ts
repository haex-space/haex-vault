import assert from 'node:assert/strict'

import { deviceFiles } from '../lib/extension-files.ts'
import {
  activePanel,
  entryIn,
  describeFocus,
  dragBetween,
  focusedPanel,
  focusList,
  inPanel,
  namesIn,
  panelShows,
  type PanelSide,
  toastShows,
  visiblePanels,
} from '../lib/files-split.ts'
import { createAndUnlock, unwrap } from '../lib/flows.ts'
import { doubleClick } from '../lib/passwords.ts'
import { scenario } from '../lib/scenario.ts'
import { KEY, runAction } from '../lib/settings.ts'

// Spec 062 US1: the file browser with two panels. The switch turns the split on, the second panel
// starts at the first one's place; each panel navigates and selects on its own and has its own
// history (Alt+Left acts on the active one); Tab makes the other panel the active one; F5 copies and
// F6 moves the selection into the other panel's folder, and both panels show what changes on disk.

const chord = (...keys: string[]) => `${keys.join('')}${KEY.release}`

scenario('files-split', { timeoutMs: 300_000 }, async (ctx) => {
  const files = deviceFiles('holzi-split-')
  try {
    const instance = await ctx.startInstance()
    await createAndUnlock(instance, { name: 'e2e-files-split' })
    files.ensureFolder()

    // After the start: on Android it clears the app's data, where the folder lives.
    for (const [path, name] of [
      [files.folder, 'split-a'],
      [files.folder, 'split-b'],
      [files.path('split-a'), 'unter'],
    ] as const) {
      unwrap(
        'files_create_folder',
        await instance.invoke('files_create_folder', {
          source: { kind: 'device' },
          path,
          name,
        }),
      )
    }
    files.write('split-a/eins.txt', 'eins')
    files.write('split-a/zwei.txt', 'zwei')
    files.write('split-a/unter/tief.txt', 'tief')
    const onDisk = (sub: string, name: string) =>
      files.files(sub).some((file) => file.endsWith(name))

    const press = (keys: string) => instance.typeToFocused(keys)
    const active = (want: PanelSide, what: string) =>
      ctx.waitFor(what, async () => (await activePanel(instance)) === want)
    const shows = (panel: PanelSide, name: string, what: string) =>
      ctx.waitFor(what, () => panelShows(instance, panel, name))
    const gone = (panel: PanelSide, name: string, what: string) =>
      ctx.waitFor(what, async () => !(await panelShows(instance, panel, name)))
    /** Selects `name` with a click and puts the focus in the list, so the keys act there. */
    const select = async (panel: PanelSide, name: string) => {
      await instance.click(entryIn(panel, name))
      await ctx.waitFor(`${name} to be selected`, () =>
        instance.exec<boolean>(
          `const row = document.querySelector(arguments[0])
           return Boolean(row && row.getAttribute('aria-selected') === 'true')`,
          [entryIn(panel, name)],
        ),
      )
      await focusList(instance, panel)
    }

    await runAction(instance, 'wm.app.open', {
      appId: 'system.files',
      at: `/device?p=${encodeURIComponent(files.path('split-a'))}`,
    })
    await instance.waitForDisplayed('files-entry-eins.txt')
    assert.equal(await visiblePanels(instance), 1, 'one panel to begin with')

    // The default desktop viewport keeps the sidebar open, leaving less than the 720 px needed by
    // two panels. Hide it before checking the split layout; the product correctly keeps one panel
    // visible while the area is too narrow.
    await instance.click('files-sidebar-toggle')
    await ctx.waitFor('the panel area to fit two panels', () =>
      instance.exec<boolean>(
        `const area = document.querySelector('[data-files-app] > div > div:last-child')
         return Boolean(area && area.getBoundingClientRect().width >= 720)`,
      ),
    )

    // The switch shows two panels: the second at the first one's place, the first one active.
    await instance.click('files-split-toggle')
    await ctx.waitFor(
      'two panels',
      async () => (await visiblePanels(instance)) === 2,
    )
    await instance.waitForDisplayed(entryIn('right', 'eins.txt'))
    await active('left', 'the left panel to be the active one')
    ctx.step('the switch shows two panels, the second at the first one’s place')

    // The bar belongs to the panels: the toolbar keeps the sidebar and the switch, each panel has
    // its own back, forward, view and sort, and the view of one does not change the other's.
    assert.equal(
      await instance.exec<boolean>(
        `return Boolean(document.querySelector('[data-files-app] > :first-child [data-testid="files-view-toggle"]'))`,
      ),
      false,
      'no view switch in the toolbar',
    )
    const viewLabel = (side: PanelSide) =>
      instance.exec<string | null>(
        `const el = document.querySelector(arguments[0])
         return el ? el.getAttribute('aria-label') : null`,
        [inPanel(side, 'files-view-toggle')],
      )
    const viewBefore = await viewLabel('left')
    assert.ok(viewBefore && viewBefore === (await viewLabel('right')))
    await instance.click(inPanel('left', 'files-view-toggle'))
    await ctx.waitFor(
      'the left view to change alone',
      async () =>
        (await viewLabel('left')) !== viewBefore &&
        (await viewLabel('right')) === viewBefore,
    )
    await instance.click(inPanel('left', 'files-view-toggle'))
    await ctx.waitFor(
      'the left view back to the list',
      async () => (await viewLabel('left')) === viewBefore,
    )
    ctx.step('view and sort are per panel')

    // Each panel navigates on its own: the right one up and into split-b (it becomes the active
    // one), then the left one into unter.
    await instance.click(inPanel('right', 'files-up'))
    await instance.waitForDisplayed(entryIn('right', 'split-b'))
    await active('right', 'the right panel to be the active one')
    await doubleClick(instance, entryIn('right', 'split-b'))
    await ctx.waitFor(
      'the right panel in split-b',
      async () => (await namesIn(instance, 'right')).length === 0,
    )
    await doubleClick(instance, entryIn('left', 'unter'))
    await shows('left', 'tief.txt', 'the left panel in unter')
    await active('left', 'the left panel to be the active one again')
    assert.deepEqual(
      await namesIn(instance, 'right'),
      [],
      'the right one stays',
    )
    ctx.step('each panel navigates on its own')

    // Alt+Left goes back in the active panel's history only; Tab makes the other panel the
    // active one and puts the focus in its list.
    await focusList(instance, 'left')
    await press(chord(KEY.alt, KEY.arrowLeft))
    await shows('left', 'eins.txt', 'the left panel back in split-a')
    assert.deepEqual(
      await namesIn(instance, 'right'),
      [],
      'the right one stays',
    )
    await press(KEY.tab)
    await active('right', 'Tab to make the right panel the active one')
    await ctx
      .waitFor(
        'the focus in the right list',
        async () => (await focusedPanel(instance)) === 'right',
      )
      .catch(async (error: Error) => {
        error.message += `; the focus is on ${await describeFocus(instance)}`
        throw error
      })
    await press(chord(KEY.alt, KEY.arrowLeft))
    await shows('right', 'split-a', 'the right panel back in the parent')
    assert.ok(
      await panelShows(instance, 'left', 'eins.txt'),
      'the left one stays',
    )
    await press(chord(KEY.alt, KEY.arrowRight))
    await ctx.waitFor(
      'the right panel forward in split-b',
      async () => (await namesIn(instance, 'right')).length === 0,
    )
    ctx.step('each panel has its own history, Tab switches the active one')

    // F5 copies, F6 moves into the other panel's folder.
    await select('left', 'eins.txt')
    await press(KEY.f5)
    await shows('right', 'eins.txt', 'eins.txt copied into the right panel')
    assert.ok(onDisk('split-b', 'eins.txt') && onDisk('split-a', 'eins.txt'))
    await select('left', 'zwei.txt')
    await press(KEY.f6)
    await shows('right', 'zwei.txt', 'zwei.txt moved into the right panel')
    await gone('left', 'zwei.txt', 'zwei.txt to leave the left panel')
    assert.ok(onDisk('split-b', 'zwei.txt') && !onDisk('split-a', 'zwei.txt'))
    ctx.step('F5 copies and F6 moves into the other panel')

    // F5 without a selection does nothing.
    await focusList(instance, 'right')
    const before = await namesIn(instance, 'right')
    await press(KEY.f5)
    await new Promise((resolve) => setTimeout(resolve, 800))
    assert.deepEqual(await namesIn(instance, 'right'), before)
    ctx.step('F5 without a selection does nothing')

    // Both panels in the same folder: F6 says so, and both show an external change.
    await instance.click(inPanel('right', 'files-up'))
    await instance.waitForDisplayed(entryIn('right', 'split-a'))
    await doubleClick(instance, entryIn('right', 'split-a'))
    await shows('right', 'eins.txt', 'the right panel in split-a')
    await select('left', 'eins.txt')
    await press(KEY.f6)
    await ctx.waitFor('the same-folder message', () =>
      toastShows(instance, 'same folder|derselbe Ordner'),
    )
    assert.ok(onDisk('split-a', 'eins.txt'), 'nothing was moved')
    files.write('split-a/neu.txt', 'neu')
    await shows('left', 'neu.txt', 'the new file in the left panel')
    await shows('right', 'neu.txt', 'the new file in the right panel')
    ctx.step('the same folder is refused for a move and shown in both panels')

    // Dragging between the panels. The right panel is typed to split-b (eins.txt, zwei.txt), the left
    // one stays in split-a (eins.txt, neu.txt, unter).
    // The path is typed: the pencil turns the crumbs into a field, Enter goes there.
    await instance.click(inPanel('right', 'files-path-edit'))
    await instance.type(
      inPanel('right', 'files-path-input'),
      files.path('split-b'),
    )
    await press(KEY.enter)
    await shows('right', 'zwei.txt', 'the right panel in split-b')
    ctx.step('a typed path opens that folder')
    const rightList = '[data-files-panel="right"] [data-files-list]'

    // Onto the other panel's empty area: the entry moves into its open folder (same source).
    assert.equal(
      await dragBetween(instance, entryIn('left', 'neu.txt'), rightList),
      'accepted',
    )
    await shows('right', 'neu.txt', 'neu.txt moved into the right panel')
    await gone('left', 'neu.txt', 'neu.txt to leave the left panel')
    assert.ok(onDisk('split-b', 'neu.txt') && !onDisk('split-a', 'neu.txt'))

    // With Ctrl it copies, and the folder keeps its entries.
    assert.equal(
      await dragBetween(instance, entryIn('left', 'unter'), rightList, true),
      'accepted',
    )
    await shows('right', 'unter', 'unter copied into the right panel')
    assert.ok(await panelShows(instance, 'left', 'unter'), 'the original stays')
    assert.ok(onDisk('split-b/unter', 'tief.txt'))

    // Onto a folder row of the other panel: into that folder.
    await dragBetween(
      instance,
      entryIn('right', 'zwei.txt'),
      entryIn('left', 'unter'),
    )
    await gone('right', 'zwei.txt', 'zwei.txt to leave the right panel')
    assert.ok(
      onDisk('split-a/unter', 'zwei.txt') && !onDisk('split-b', 'zwei.txt'),
    )
    ctx.step('dragging moves into the other panel, with Ctrl it copies')
  } finally {
    files.remove()
  }
})
