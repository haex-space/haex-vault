import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { devProject, devServer } from '../lib/extension-dev.ts'
import { unwrap, waitForWorkspace, type FlowInstance } from '../lib/flows.ts'
import type { Page } from '../lib/page.ts'
import {
  KEY,
  openSettings,
  setSessionRestore,
  WM,
  wmSnapshot,
} from '../lib/settings.ts'
import {
  backInTab,
  inFrame,
  openFromLauncher,
  probeRequest,
  type InstalledExtension,
} from '../lib/extensions.ts'

// Spec 017, US12, T093/T096 (quickstart §7 last bullet): developer mode switched on in the
// settings reloads holzi's window; a project served from 127.0.0.1 loads unsigned, opens with the
// permanent mark on its tab, talks to holzi like an installed extension, keeps its tables on this
// device, shows its console output and gets a new document after a change. Its tab behaves like an
// installed extension's through the frame shim; switched off, it is gone.

type Instance = Page & FlowInstance

/** Waits until `change` replaced holzi's document and the workspace is back. */
async function reloaded(page: Instance, change: () => Promise<unknown>) {
  await page.exec(`window.__beforeDevSwitch = true; return true`)
  await change()
  const end = Date.now() + 15_000
  while (
    await page
      .exec<boolean>(`return window.__beforeDevSwitch === true`)
      .catch(() => true)
  ) {
    if (Date.now() > end) throw new Error('holzi did not reload')
    await new Promise((resolve) => setTimeout(resolve, 100))
  }
  await waitForWorkspace(page)
}

/** What the last readiness check saw inside the frame, or why it could not look. */
let lastFrameCheck: unknown = null

async function frameReady(page: Instance, extension: InstalledExtension) {
  try {
    const inside = await inFrame<{ ready: boolean }>(
      page,
      extension,
      `return {
         ready: document.documentElement.dataset.probeReady === '1',
         readyState: document.readyState,
         href: location.href,
         probe: typeof window.probe,
         module: document.getElementById('module')?.textContent ?? null,
       }`,
    )
    lastFrameCheck = inside
    return inside.ready
  } catch (e) {
    lastFrameCheck = { error: String(e) }
    return false
  }
}

/** The frames of `extension` in holzi's document and what they show around them. */
async function framesAround(page: Instance, extension: InstalledExtension) {
  return page
    .exec(
      `return [...document.querySelectorAll('[data-extension-id="' + arguments[0] + '"]')].map((f) => {
         const box = f.getBoundingClientRect()
         return {
           tag: f.tagName,
           invisible: f.classList.contains('invisible'),
           box: [Math.round(box.width), Math.round(box.height)],
           src: f.getAttribute('src'),
           around: f.parentElement?.innerText.slice(0, 200) ?? null,
         }
       })`,
      [extension.id],
    )
    .catch((e: unknown) => ({ error: String(e) }))
}

scenario('extension-dev-mode', { timeoutMs: 300_000 }, async (ctx) => {
  const group = await ctx.group({ users: { anna: ['laptop'] } })
  const page = group.device('anna/laptop').page
  // With session restore a tab of the development version would come back if nothing removed it.
  await setSessionRestore(page, true)
  const { server, port, title } = await devServer()
  try {
    const folder = devProject(port)
    const refused = (await page.invoke('extension_dev_load', {
      projectPath: folder,
    })) as { ok: boolean; error?: { reason?: string } }
    assert.equal(refused.ok, false)
    assert.equal(refused.error?.reason, 'dev_mode_off')

    // Through the switch in the settings, as the developer does it.
    await reloaded(page, async () => {
      await openSettings(page)
      await page.click('settings-category-extensions')
      await page.click('extension-dev-mode')
    })
    ctx.step('developer mode on; holzi reloaded')

    // The folder dialog is native: the scene does what it hands over.
    const preview = unwrap<{ name: string }>(
      'extension_dev_load',
      await page.invoke('extension_dev_load', { projectPath: folder }),
    )
    assert.equal(preview.name, 'devprobe')
    const id = unwrap<string>(
      'extension_dev_confirm',
      await page.invoke('extension_dev_confirm', {
        projectPath: folder,
        accepted: [],
      }),
    )
    const dev: InstalledExtension = { id, title: 'Dev Probe' }
    await ctx.waitFor(
      'the development version in the launcher',
      async () =>
        JSON.stringify(await page.invoke('extension_list')).includes(id),
      { timeoutMs: 10_000 },
    )
    await openFromLauncher(page, dev)
    await ctx
      .waitFor('the dev page to be ready', () => frameReady(page, dev), {
        // The development server and WebKitGTK can both be cold in CI. Keep the scenario's
        // 300-second limit as the outer bound, but do not fail a healthy frame after 20 seconds.
        timeoutMs: 60_000,
      })
      .catch(async (e: unknown) => {
        // Fails now and then in CI with the page rendered: say what the check saw.
        const frames = await framesAround(page, dev)
        throw new Error(
          `${String(e)}; last check in the frame: ${JSON.stringify(lastFrameCheck)}; frames: ${JSON.stringify(frames)}`,
        )
      })
    await page.waitForDisplayed('dev-badge', 5_000)
    ctx.step('loaded from 127.0.0.1, its tab marked')

    const info = await probeRequest(page, dev, 'extension_get_info', {})
    assert.equal((info.result as { name?: string }).name, 'devprobe')
    const key = (info.result as { publicKey: string }).publicKey
    const table = `${key}__devprobe__notes`
    const migrated = await probeRequest(
      page,
      dev,
      'extension_database_register_migrations',
      {
        extensionVersion: '0.1.0',
        migrations: [
          {
            name: '0000_init',
            sql: `CREATE TABLE \`${table}\` (\`id\` text PRIMARY KEY NOT NULL);`,
          },
        ],
      },
    )
    assert.equal(migrated.error, undefined, JSON.stringify(migrated))
    const written = await probeRequest(
      page,
      dev,
      'extension_database_execute',
      { query: `INSERT INTO "${table}" (id) VALUES ('n1')`, params: [] },
    )
    assert.equal(written.error, undefined, JSON.stringify(written))
    ctx.step('its own tables, on this device')

    await inFrame(
      page,
      dev,
      `window.parent.postMessage({ type: 'console.forward', data: { level: 'warn', message: 'hello from dev', timestamp: '12:00' } }, '*'); return true`,
    )
    await page.click('dev-console-toggle')
    await ctx.waitFor(
      'the console line',
      async () =>
        (
          await page.exec<string>(
            `return [...document.querySelectorAll('[data-testid="dev-console-line"]')].map((l) => l.textContent).join('|')`,
          )
        ).includes('hello from dev'),
      { timeoutMs: 5_000 },
    )
    ctx.step('console output shown')

    // WebKitGTK fires `load` for a hash navigation too, and the SDK takes `port:init` once per
    // document: the page keeps talking over the channel it has.
    await inFrame(page, dev, `location.hash = '#/elsewhere'; return true`)
    await new Promise((resolve) => setTimeout(resolve, 1_000))
    const afterHash = await probeRequest(
      page,
      dev,
      'extension_database_query',
      { query: `SELECT id FROM "${table}"`, params: [] },
    )
    assert.deepEqual(
      (afterHash.result as { rows?: unknown[][] }).rows,
      [['n1']],
      JSON.stringify(afterHash),
    )
    assert.equal(
      await page
        .exec<string>(
          `return document.querySelector('[data-testid="extension-frame"]').className`,
        )
        .then((name) => name.includes('invisible')),
      false,
      'the frame stays ready',
    )
    ctx.step('a hash navigation keeps the channel')

    // The frame shim comes with holzi's init script (research R16): title, navigation with
    // holzi Back, holzi's shortcuts and the close guard work as in an installed extension.
    const place = () =>
      inFrame<string>(
        page,
        dev,
        `return document.getElementById('place').textContent`,
      )
    await inFrame(page, dev, `document.title = 'Dev titled'; return true`)
    await ctx.waitFor(
      'the page title on its tab',
      async () =>
        page.exec<boolean>(
          `return document.body.innerText.includes('Dev titled')`,
        ),
      { timeoutMs: 5_000 },
    )
    ctx.step('the page names its tab')

    await inFrame(
      page,
      dev,
      `document.getElementById('to-second').click(); return true`,
    )
    await ctx.waitFor(
      'holzi to record the place inside the page',
      async () =>
        JSON.stringify(await page.invoke('wm_session_load')).includes(
          '/second',
        ),
      { fixed: true, timeoutMs: 15_000 },
    )
    await backInTab(page, dev)
    await ctx.waitFor(
      'the page to follow holzi Back',
      async () => (await place()) === '#/elsewhere',
      { timeoutMs: 10_000 },
    )
    ctx.step('navigation inside the tab and holzi Back')

    await inFrame(
      page,
      dev,
      `document.getElementById('to-second').click(); return true`,
    )
    await ctx.waitFor(
      'the second place',
      async () => (await place()) === '#/second',
      {
        timeoutMs: 5_000,
      },
    )
    // Alt+ArrowLeft is holzi's Back; pressed inside the frame, the shim hands it to holzi, which
    // acts only while the frame has the focus. WebKitWebDriver cannot send keys into this sandboxed
    // frame, so the page gets the key as an event after holzi focused the frame.
    await page.exec(
      `document.querySelector('[data-testid="extension-frame"][data-extension-id="${dev.id}"]').focus(); return true`,
    )
    await inFrame(
      page,
      dev,
      `window.dispatchEvent(new KeyboardEvent('keydown', { code: 'ArrowLeft', key: 'ArrowLeft', altKey: true, bubbles: true })); return true`,
    )
    await ctx.waitFor(
      'holzi Back from a key pressed in the frame',
      async () => (await place()) === '#/elsewhere',
      { timeoutMs: 10_000 },
    )
    ctx.step('a holzi shortcut pressed inside the frame')

    await inFrame(
      page,
      dev,
      `window.__guard = (event) => event.preventDefault(); window.addEventListener('beforeunload', window.__guard); return true`,
    )
    await page.exec(`${WM}.runAction('wm.tab.close', {}); return true`)
    await ctx.waitFor(
      'the close confirmation',
      async () =>
        page.exec<boolean>(
          `return document.querySelector('[role="alertdialog"]') !== null`,
        ),
      { timeoutMs: 5_000 },
    )
    await page.typeToFocused(KEY.escape)
    await ctx.waitFor(
      'the confirmation gone',
      async () =>
        page.exec<boolean>(
          `return document.querySelector('[role="alertdialog"]') === null`,
        ),
      { timeoutMs: 5_000 },
    )
    assert.ok(
      (await wmSnapshot(page)).windows.some((w) =>
        w.tabs.some((t) => t.appId === `extension.${id}`),
      ),
      'the guarded tab stays open',
    )
    await inFrame(
      page,
      dev,
      `window.removeEventListener('beforeunload', window.__guard); return true`,
    )
    ctx.step('a close guard asks before the tab closes')

    title.text = 'Probe dev changed'
    await inFrame(page, dev, `location.reload(); return true`).catch(() => {})
    await ctx.waitFor(
      'the changed page after a reload',
      async () =>
        (await inFrame<string>(
          page,
          dev,
          `return document.documentElement.dataset.probeReady === '1' ? document.getElementById('title').textContent : ''`,
        ).catch(() => '')) === 'Probe dev changed',
      { timeoutMs: 20_000 },
    )
    const again = await probeRequest(page, dev, 'extension_database_query', {
      query: `SELECT id FROM "${table}"`,
      params: [],
    })
    assert.deepEqual((again.result as { rows?: unknown[][] }).rows, [['n1']])
    ctx.step('a new document talks to holzi again')

    // The settings came back with the session; switching off as the settings do it.
    await reloaded(page, async () => {
      unwrap(
        'extension_dev_mode_set',
        await page.invoke('extension_dev_mode_set', { enabled: false }),
      )
      await page.exec(`window.location.reload(); return true`).catch(() => {})
    })
    const snapshot = await wmSnapshot(page)
    assert.ok(
      !snapshot.windows.some((w) =>
        w.tabs.some((t) => t.appId === `extension.${id}`),
      ),
      'its tab is gone',
    )
    // Still listed, so the settings can unload it, but not enabled: no launcher entry, no tab.
    const listed = unwrap<Array<{ id: string; enabled: boolean }>>(
      'extension_list',
      await page.invoke('extension_list'),
    ).find((e) => e.id === id)
    assert.equal(
      listed?.enabled,
      false,
      'not enabled while developer mode is off',
    )
    await openSettings(page)
    await page.click('settings-category-extensions')
    await page.waitForDisplayed('extension-dev-unload', 5_000)
    await page.click('extension-dev-unload')
    await ctx.waitFor(
      'the development version unloaded',
      async () =>
        !JSON.stringify(await page.invoke('extension_list')).includes(id),
      { timeoutMs: 10_000 },
    )
    ctx.step(
      'developer mode off: the development version is gone, unloaded from the settings',
    )
  } finally {
    server.close()
  }
})
