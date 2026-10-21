import assert from 'node:assert/strict'
import { join } from 'node:path'
import { scenario } from '../lib/scenario.ts'
import { createAndUnlock } from '../lib/flows.ts'
import { freePort } from '../lib/ports.ts'
import { KEY, openSettings, waitForLocation } from '../lib/settings.ts'
import {
  startBridgeClient,
  type BridgeClient,
  type BridgeEvent,
} from '../lib/browser-bridge.ts'

// Spec 049, US1 (T036, quickstart M1–M2 without a browser): the External Bridge is off until it is
// turned on in Einstellungen → Erweiterungen → Browser-Erweiterungen; a stand-in for haex-pass-browser
// connects, holzi asks with the client's name, fingerprint and origin, "Zulassen" with "Erlaubnis
// merken" reaches the open connection at once, and the next connection is approved without a
// question.
scenario('browser-bridge', { timeoutMs: 300_000 }, async (ctx) => {
  const instance = await ctx.startInstance()
  await createAndUnlock(instance, { name: 'e2e-browser-bridge' })
  const port = await freePort()
  const logDir = join(process.env.E2E_RUN_DIR ?? '.', 'browser-bridge')
  const clients: BridgeClient[] = []
  const client = async (name: string, seed: number) => {
    const started = await startBridgeClient({
      port,
      seed,
      logFile: join(logDir, `${name}.log`),
    })
    clients.push(started)
    return started
  }
  const waitForEvent = (
    running: BridgeClient,
    what: string,
    match: (event: BridgeEvent) => boolean,
  ) =>
    ctx.waitFor(what, () => running.events().find(match), {
      timeoutMs: 20_000,
    })

  try {
    await openSettings(instance)
    await instance.click('settings-category-extensions')
    await waitForLocation(instance, 'extensions')
    await instance.click('extensions-browser-bridge')
    await waitForLocation(instance, 'extensions.browserBridge')
    await instance.waitForDisplayed(
      '[data-testid="browser-bridge-state"][data-state="off"]',
    )
    ctx.step('off by default')

    await instance.type(
      'browser-bridge-port',
      `${KEY.end}${KEY.backspace.repeat(5)}${port}${KEY.enter}`,
    )
    await instance.click('browser-bridge-switch')
    await instance.waitForDisplayed(
      '[data-testid="browser-bridge-state"][data-state="running"]',
    )
    ctx.step('turned on')

    const first = await client('first', 7)
    await waitForEvent(
      first,
      'the handshake answer',
      (e) => e.event === 'handshake',
    )
    assert.equal(first.events()[0]?.pending, true, 'waits for approval')
    await instance.waitForDisplayed('bridge-approval')
    const shown = await instance.exec<Record<string, string>>(
      `const text = (id) => document.querySelector('[data-testid="' + id + '"]')?.textContent?.trim() ?? ''
       return {
         name: text('bridge-approval-name'),
         fingerprint: text('bridge-approval-fingerprint').replaceAll(' ', ''),
         origin: text('bridge-approval-origin'),
       }`,
    )
    assert.match(shown.name ?? '', /haex-pass Browser Extension/)
    assert.equal(shown.fingerprint, first.events()[0]?.clientId)
    assert.equal(shown.origin, 'chrome-extension://e2e')
    ctx.step('asked with name, fingerprint and origin')

    await instance.click('[data-testid="bridge-approval-remember"]')
    await instance.click('bridge-approval-allow')
    await waitForEvent(
      first,
      'the approval on the open connection',
      (e) => e.event === 'authorization' && e.authorized === true,
    )
    ctx.step('approved and remembered')

    const again = await client('again', 7)
    const handshake = await waitForEvent(
      again,
      'the second handshake',
      (e) => e.event === 'handshake',
    )
    assert.ok(handshake)
    assert.equal(handshake.authorized, true, 'remembered: no question')
    const dialogs = await instance.exec<number>(
      `return document.querySelectorAll('[data-testid="bridge-approval"]').length`,
    )
    assert.equal(dialogs, 0)
    ctx.step('the remembered client connects without a question')

    const stranger = await client('stranger', 9)
    await waitForEvent(
      stranger,
      'the stranger waits',
      (e) => e.event === 'handshake',
    )
    await instance.waitForDisplayed('bridge-approval')
    await instance.click('bridge-approval-deny')
    await waitForEvent(
      stranger,
      'the refusal',
      (e) => e.event === 'authorization' && e.authorized === false,
    )
    ctx.step('a declined client hears it and is turned away')
  } finally {
    for (const running of clients) await running.stop()
  }
})
