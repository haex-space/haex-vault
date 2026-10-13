import assert from 'node:assert/strict'

import { deviceFiles } from '../lib/platform/device-files.ts'
import { solidPng } from '../lib/png.ts'
import { scenario } from '../lib/scenario.ts'
import { runAction, setSessionRestore, wmSnapshot } from '../lib/settings.ts'

// Spec 044 T035 with T034 (spec 022 session restore): two file browser tabs come back after a
// restart where they stood, one in its folder, the other with its file open in the viewer. The
// saved session names folders and files by path; share URLs of the media server are never stored.

const NOTE = 'Hallo aus der Sitzung'

scenario('files-restore', { timeoutMs: 240_000 }, async (ctx) => {
  const group = await ctx.group({ users: { anna: ['laptop'] } })
  const device = group.device('anna/laptop')
  const page = device.page
  // The folders lie where this device reads them: on the phone of an Android run, else here.
  const env =
    'phone' in page && page.phone !== undefined
      ? process.env
      : { E2E_PLATFORM: 'linux' }
  const notes = deviceFiles('holzi-files-restore-notes-', env)
  const pictures = deviceFiles('holzi-files-restore-pictures-', env)
  ctx.onTeardown(() => notes.remove())
  ctx.onTeardown(() => pictures.remove())
  notes.write('notiz.txt', NOTE)
  pictures.write('foto.png', solidPng(32, 24, [40, 120, 220]))

  await setSessionRestore(page, true)
  const opened = await runAction(page, 'wm.app.open', {
    appId: 'system.files',
    at: `/device?p=${encodeURIComponent(notes.folder)}&open=notiz.txt`,
  })
  assert.ok(opened.ok, JSON.stringify(opened))
  await page.waitForDisplayed('files-viewer-text')
  const second = await runAction(page, 'wm.tab.new', {
    appId: 'system.files',
    at: `/device?p=${encodeURIComponent(pictures.folder)}`,
  })
  assert.ok(second.ok, JSON.stringify(second))
  await page.waitForDisplayed('files-entry-foto.png')
  ctx.step('two tabs: a file in the viewer, a folder')

  const saved = (await ctx.waitFor(
    'the saved session to hold both tabs',
    async () => {
      const text = JSON.stringify(await page.invoke('wm_session_load'))
      return text.includes('notiz.txt') && text.includes('pictures')
        ? text
        : false
    },
    { fixed: true, timeoutMs: 15_000 },
  )) as string
  assert.ok(!saved.includes('127.0.0.1'), 'the session holds a share URL')
  ctx.step('session saved')

  await device.restart()
  const restored = device.page
  const tabs = (await ctx.waitFor(
    'two file browser tabs after the restart',
    async () => {
      const ids = (await wmSnapshot(restored)).windows.flatMap((window) =>
        window.tabs
          .filter((tab) => tab.appId === 'system.files')
          .map((tab) => tab.id),
      )
      return ids.length === 2 ? ids : false
    },
    { fixed: true, timeoutMs: 30_000 },
  )) as string[]
  // Each tab in front once: what it shows tells which one it is.
  const shown: string[] = []
  for (const tabId of tabs) {
    const activated = await runAction(restored, 'wm.tab.activate', { tabId })
    assert.ok(activated.ok, JSON.stringify(activated))
    shown.push(
      (await ctx.waitFor(
        'the tab to show its folder or its file',
        () =>
          restored.exec<string | false>(
            `const visible = (hook) => {
               const el = document.querySelector('[data-testid="' + hook + '"]')
               return !!el && el.getClientRects().length > 0
             }
             if (visible('files-viewer-text'))
               return document.querySelector('[data-testid="files-viewer-text"]').textContent
             return visible('files-entry-foto.png') ? 'foto.png' : false`,
          ),
        { fixed: true, timeoutMs: 30_000 },
      )) as string,
    )
  }
  assert.ok(
    shown.includes('foto.png'),
    `the folder tab: ${JSON.stringify(shown)}`,
  )
  assert.ok(
    shown.some((text) => text.includes(NOTE)),
    `the file tab: ${JSON.stringify(shown)}`,
  )
  ctx.step('both tabs back: the folder, and the file in the viewer')
})
