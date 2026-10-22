import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { createAndUnlock, unwrap } from '../lib/flows.ts'
import {
  fixture,
  inFrame,
  install,
  openFromLauncher,
} from '../lib/extensions.ts'

const settle = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms))

// Spec 017, US3, T073 (quickstart §5): from inside an extension frame nothing of holzi, the
// network or the IPC is reachable. Each attempt reports how it ended; none may succeed. A shortcut
// forged on the shim's port does nothing while the frame has no focus, and the frame can neither
// leave for the web nor load another extension's page without that frame's token.
scenario('extension-isolation', { timeoutMs: 180_000 }, async (ctx) => {
  const instance = await ctx.startInstance()
  await createAndUnlock(instance, { name: 'e2e-extension-isolation' })
  const probe = await install(instance, fixture('e2e', 'probe'))
  await openFromLauncher(instance, probe)

  const attempts = await inFrame<Record<string, string>>(
    instance,
    probe,
    `const done = (start) => { try { return start().then(() => 'succeeded', () => 'blocked') } catch { return Promise.resolve('blocked') } }
     const tried = (fn) => { try { const r = fn(); return r === undefined || r === null ? 'blocked' : 'succeeded' } catch { return 'blocked' } }
     const event = (make) => new Promise((resolve) => {
       try { make(() => resolve('succeeded'), () => resolve('blocked')) } catch { resolve('blocked') }
       setTimeout(() => resolve('blocked'), 3000)
     })
     return Promise.all([
       tried(() => window.parent.document.title),
       tried(() => window.localStorage.getItem('x') ?? 'readable'),
       done(() => fetch('https://example.org/')),
       event((ok, no) => { const ws = new WebSocket('wss://example.org/'); ws.onopen = ok; ws.onerror = no }),
       event((ok, no) => { const img = new Image(); img.onload = ok; img.onerror = no; img.src = 'https://example.org/x.png' }),
       tried(() => window.open('https://example.org/')),
       tried(() => window.__TAURI_INTERNALS__),
       done(() => fetch('ipc://localhost/plugin%3Aapp%7Cversion', { method: 'POST' })),
       done(() => fetch('http://ipc.localhost/plugin%3Aapp%7Cversion', { method: 'POST' })),
     ]).then(([parentDocument, storage, fetchRemote, webSocket, image, popup, tauri, ipc, ipcHttp]) =>
       ({ parentDocument, storage, fetchRemote, webSocket, image, popup, tauri, ipc, ipcHttp }))`,
  )
  for (const [attempt, outcome] of Object.entries(attempts)) {
    assert.equal(outcome, 'blocked', `${attempt} from inside the frame`)
  }
  ctx.step('holzi, network and IPC unreachable from the frame')

  await inFrame(
    instance,
    probe,
    `document.getElementById('to-second').click(); return true`,
  )
  const place = () =>
    inFrame<string>(
      instance,
      probe,
      `return document.getElementById('place').textContent`,
    )
  await ctx.waitFor(
    'the frame at #/second',
    async () => (await place()) === '#/second',
    { timeoutMs: 10_000 },
  )
  // The extension takes the shim's port by wrapping postMessage and changing the title, and sends
  // the forged shortcut a moment later; meanwhile holzi takes the focus back from the frame
  // (switching into the frame to run this script may have given it the focus).
  const captured = await inFrame<boolean>(
    instance,
    probe,
    `const send = MessagePort.prototype.postMessage
     let shim = null
     MessagePort.prototype.postMessage = function (...args) {
       if (args[0] && args[0].type === 'title') shim = this
       return send.apply(this, args)
     }
     document.title = 'Forged'
     return new Promise((resolve) => setTimeout(() => {
       MessagePort.prototype.postMessage = send
       if (!shim) return resolve(false)
       setTimeout(() => shim.postMessage({ type: 'shortcut', id: 'wm.tab.back' }), 1500)
       resolve(true)
     }, 300))`,
  )
  assert.ok(captured, 'the shim port was not captured')
  const frameFocused = await instance.exec<boolean>(
    `const frame = document.querySelector('[data-testid="extension-frame"]')
     if (document.activeElement === frame) frame.blur()
     return document.activeElement === frame`,
  )
  assert.equal(frameFocused, false, 'holzi took the focus back')
  await ctx.waitFor(
    'the forged title in holzi',
    async () =>
      instance.exec<boolean>(
        `return document.body.innerText.includes('Forged')`,
      ),
    { timeoutMs: 5_000 },
  )
  await settle(2_500)
  assert.equal(await place(), '#/second', 'the forged shortcut went back')
  ctx.step('a forged shortcut without focus does nothing')

  await inFrame(
    instance,
    probe,
    `location.href = 'https://example.org/'; return true`,
  )
  await settle(3_000)
  const left = await inFrame<string>(instance, probe, `return location.href`)
  assert.ok(!left.startsWith('https:'), `the frame went to ${left}`)
  ctx.step('the frame cannot leave for the web')

  // Another extension with an open frame; its page is served only with that frame's token.
  const notes = await install(instance, fixture('vectors', 'good-notes-like'))
  unwrap(
    'extension_frame_open',
    await instance.invoke('extension_frame_open', {
      extensionId: notes.id,
      tabId: 'e2e',
    }),
  )
  const notesPath = `/${notes.id.replaceAll('-', '')}/`
  await inFrame(
    instance,
    probe,
    `location.href = 'holzi-ext://localhost' + arguments[0] + location.search; return true`,
    [notesPath],
  )
  await settle(3_000)
  const other = await inFrame<{ path: string; title: string; text: string }>(
    instance,
    probe,
    `return { path: location.pathname, title: document.title, text: document.body ? document.body.innerText : '' }`,
  )
  assert.ok(
    !other.path.startsWith(notesPath) ||
      (other.title === '' && other.text.trim() === ''),
    `another extension's page was served: ${JSON.stringify(other)}`,
  )
  ctx.step("another extension's page is not served with this frame's token")
})
