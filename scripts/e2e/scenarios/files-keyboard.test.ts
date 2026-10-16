import assert from 'node:assert/strict'

import { deviceFiles } from '../lib/extension-files.ts'
import { createAndUnlock, unwrap } from '../lib/flows.ts'
import { contextMenu } from '../lib/passwords.ts'
import { scenario, type ScenarioContext } from '../lib/scenario.ts'
import { KEY, runAction } from '../lib/settings.ts'

// Spec 044 FR-017b (T098): the file browser by keyboard. The list has the focus once a folder opens;
// the arrows, Home and End move and select, Shift extends; letters jump to a name; Ctrl+C and Ctrl+V
// copy into a folder opened with Enter, Alt+Up goes back up; F2 renames, Ctrl+Shift+N makes a
// folder, Delete deletes; Ctrl+H shows hidden entries; Ctrl+A and Esc select all and nothing. The
// context menu names the keys.

type Instance = Awaited<ReturnType<ScenarioContext['startInstance']>>

const TYPE_PAUSE_MS = 1100

const chord = (...keys: string[]) => `${keys.join('')}${KEY.release}`

/** The names of the selected rows, in the order shown. */
const selected = (instance: Instance) =>
  instance.exec<string[]>(
    `return [...document.querySelectorAll('[data-files-list] [aria-selected="true"]')]
       .map((row) => row.getAttribute('data-path').split(/[\\\\/]/).pop())`,
  )

/** Whether the focus is in the open folder's list. */
const listFocused = (instance: Instance) =>
  instance.exec<boolean>(
    `return Boolean(document.activeElement && document.activeElement.closest('[data-files-list]'))`,
  )

const shows = (instance: Instance, name: string) =>
  instance.exec<boolean>(
    `return Boolean(document.querySelector('[data-testid="files-entry-' + arguments[0] + '"]'))`,
    [name],
  )

async function setName(instance: Instance, name: string) {
  await instance.waitForDisplayed('files-name-input')
  for (let attempt = 0; attempt < 2; attempt++) {
    await instance.type(
      'files-name-input',
      `${KEY.control}a${KEY.release}${KEY.backspace}`,
    )
    await instance.type('files-name-input', name)
    const value = await instance.exec<string>(
      `return document.querySelector('[data-testid="files-name-input"]').value`,
    )
    if (value === name) {
      try {
        await instance.click('files-name-save')
      } catch (cause) {
        // WebKit can report the button as non-interactable after the click already submitted the
        // form and the drawer closed. The caller's entry assertion verifies that the save landed.
        const dialogOpen = await instance.exec<boolean>(
          `return Boolean(document.querySelector('[data-testid="files-name-form"]'))`,
        )
        if (dialogOpen) throw cause
      }
      return
    }
  }
  const value = await instance.exec<string>(
    `return document.querySelector('[data-testid="files-name-input"]').value`,
  )
  assert.equal(value, name)
}

scenario('files-keyboard', { timeoutMs: 240_000 }, async (ctx) => {
  const files = deviceFiles('holzi-keyboard-')
  try {
    const instance = await ctx.startInstance()
    await createAndUnlock(instance, { name: 'e2e-files-keyboard' })

    // After the start: on Android it clears the app's data, where the folder lives.
    for (const name of ['alpha.txt', 'beta.txt', 'gamma.txt', '.versteckt'])
      files.write(name, name)
    unwrap(
      'files_create_folder',
      await instance.invoke('files_create_folder', {
        source: { kind: 'device' },
        path: files.folder,
        name: 'ordner',
      }),
    )
    const press = (keys: string) => instance.typeToFocused(keys)
    const selection = async (want: string[], what: string) => {
      await ctx.waitFor(what, async () => {
        const now = await selected(instance)
        return JSON.stringify(now) === JSON.stringify(want)
      })
    }

    await runAction(instance, 'wm.app.open', {
      appId: 'system.files',
      at: `/device?p=${encodeURIComponent(files.folder)}`,
    })
    await instance.waitForDisplayed('files-entry-alpha.txt')
    await ctx.waitFor('the list to have the focus', () => listFocused(instance))
    ctx.step('the list has the focus')

    await press(KEY.arrowDown)
    await selection(['ordner'], 'the first entry')
    await press(KEY.arrowDown)
    await selection(['alpha.txt'], 'the next entry')
    await press(chord(KEY.shift, KEY.arrowDown))
    await selection(['alpha.txt', 'beta.txt'], 'Shift to extend')
    await press(KEY.end)
    await selection(['gamma.txt'], 'the last entry')
    await press(KEY.home)
    await selection(['ordner'], 'the first entry again')
    ctx.step('the arrows, Home and End move and select')

    await press('g')
    await selection(['gamma.txt'], 'a jump to g')
    await new Promise((resolve) => setTimeout(resolve, TYPE_PAUSE_MS))
    await press('al')
    await selection(['alpha.txt'], 'a jump to al')
    ctx.step('letters jump to a name')

    await press(chord(KEY.control, 'c'))
    await press(KEY.home)
    await selection(['ordner'], 'the folder')
    await press(KEY.enter)
    await instance.waitForDisplayed('files-empty')
    await ctx.waitFor('the list to have the focus in the folder', () =>
      listFocused(instance),
    )
    await press(chord(KEY.control, 'v'))
    await instance.waitForDisplayed('files-entry-alpha.txt', 20_000)
    await press(chord(KEY.alt, KEY.arrowUp))
    await instance.waitForDisplayed('files-entry-ordner')
    await ctx.waitFor('the list to have the focus back up', () =>
      listFocused(instance),
    )
    ctx.step('copied into a folder opened with Enter, and back up')

    await press('b')
    await selection(['beta.txt'], 'beta')
    await press(KEY.f2)
    await setName(instance, 'beta2.txt')
    await instance.waitForDisplayed('files-entry-beta2.txt')
    ctx.step('F2 renames')

    await press(chord(KEY.control, KEY.shift, 'n'))
    await setName(instance, 'Neu')
    await instance.waitForDisplayed('files-entry-Neu')
    ctx.step('Ctrl+Shift+N makes a folder')

    await ctx.waitFor('the list to have the focus after the dialog', () =>
      listFocused(instance),
    )
    await new Promise((resolve) => setTimeout(resolve, TYPE_PAUSE_MS))
    await press('g')
    await selection(['gamma.txt'], 'gamma')
    await press(KEY.delete)
    await ctx.waitFor('gamma.txt to go', async () => {
      const asks = await instance.exec<boolean>(
        `return Boolean(document.querySelector('[data-testid="files-delete-confirm"]'))`,
      )
      if (asks) await instance.click('files-delete-confirm')
      return !(await shows(instance, 'gamma.txt'))
    })
    ctx.step('Delete deletes')

    await ctx.waitFor('the list to keep the focus', () => listFocused(instance))
    await press(chord(KEY.control, 'h'))
    await instance.waitForDisplayed('files-entry-.versteckt')
    await press(chord(KEY.control, 'h'))
    await ctx.waitFor(
      'the hidden entry to go',
      async () => !(await shows(instance, '.versteckt')),
    )
    ctx.step('Ctrl+H shows and hides hidden entries')

    await press(chord(KEY.control, 'a'))
    await selection(
      ['Neu', 'ordner', 'alpha.txt', 'beta2.txt'],
      'everything selected',
    )
    await press(KEY.escape)
    await selection([], 'nothing selected')
    ctx.step('Ctrl+A and Esc')

    await contextMenu(instance, 'files-entry-alpha.txt')
    await instance.waitForDisplayed('files-menu-copy')
    const copy = await instance.exec<string>(
      `return document.querySelector('[data-testid="files-menu-copy"]').textContent`,
    )
    assert.match(copy, /(Strg|Ctrl)\+C/)
    await press(KEY.escape)
    ctx.step('the menu names the keys')
  } finally {
    files.remove()
  }
})
