import assert from 'node:assert/strict'
import { join } from 'node:path'
import { scenario } from '../lib/scenario.ts'
import { createAndUnlock } from '../lib/flows.ts'
import { freePort } from '../lib/ports.ts'
import { KEY, openSettings, waitForLocation } from '../lib/settings.ts'
import {
  entryTitles,
  openPasswords,
  overview,
  selectTab,
  slideSettled,
} from '../lib/passwords.ts'
import {
  startBridgeClient,
  type BridgeClient,
  type BridgeEvent,
} from '../lib/browser-bridge.ts'

// Spec 049, US1 (T036, quickstart M1–M2 without a browser): the External Bridge is off until it is
// turned on in Einstellungen → Erweiterungen → Browser-Erweiterungen; a stand-in for haex-pass-browser
// connects, holzi asks with the client's name, fingerprint and origin, "Zulassen" with "Erlaubnis
// merken" reaches the open connection at once, and the next connection is approved without a
// question. A client approved for reading only is asked before it saves (US3, FR-015), and the
// saved entry is in the vault at once. The settings list the clients (US5, T058): narrowing one to
// "Lesen", revoking it while it holds a connection, which hears `authorized: false` and is closed,
// and unblocking a declined one, which is asked again. A passkey is created after the presence
// confirmation and shows in the new entry's tab Extra; a declined confirmation answers
// USER_CANCELLED (US4, T068).
scenario('browser-bridge', { timeoutMs: 420_000 }, async (ctx) => {
  const instance = await ctx.startInstance()
  await createAndUnlock(instance, { name: 'e2e-browser-bridge' })
  const port = await freePort()
  const logDir = join(process.env.E2E_RUN_DIR ?? '.', 'browser-bridge')
  const clients: BridgeClient[] = []
  const client = async (
    name: string,
    seed: number,
    request?: { action: string; payload: Record<string, unknown> },
    hold = false,
  ) => {
    const started = await startBridgeClient({
      port,
      seed,
      logFile: join(logDir, `${name}.log`),
      request,
      hold,
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

    const reader = await client('reader', 11, {
      action: 'create-item',
      payload: {
        title: 'E2E Shop',
        url: 'https://shop.example',
        password: 'pw',
      },
    })
    await instance.waitForDisplayed('bridge-approval')
    await instance.click('bridge-approval-read')
    await instance.click('bridge-approval-allow')
    await instance.waitForDisplayed('bridge-write')
    ctx.step('a reader is asked before it saves')
    await instance.click('bridge-write-allow')
    const saved = await waitForEvent(
      reader,
      'the answer to create-item',
      (e) => e.event === 'reply',
    )
    const reply = saved?.reply as { success?: boolean } | undefined
    assert.equal(reply?.success, true, JSON.stringify(saved))
    assert.ok((await entryTitles(instance)).includes('E2E Shop'))
    ctx.step('the entry is saved')

    const row = (clientId: unknown, extra = '') =>
      `[data-testid="bridge-client"][data-client-id="${String(clientId)}"]${extra}`
    const firstId = first.events()[0]?.clientId
    const held = await client('held', 7, undefined, true)
    await waitForEvent(
      held,
      'the held connection',
      (e) => e.event === 'handshake' && e.authorized === true,
    )
    await instance.waitForDisplayed(
      row(firstId, '[data-access="readWrite"][data-connected]'),
    )
    ctx.step('the approved client is listed as connected')

    await instance.click(row(firstId))
    await instance.waitForDisplayed('bridge-client-editor')
    await instance.click('bridge-client-read')
    await instance.waitForDisplayed(row(firstId, '[data-access="read"]'))
    ctx.step('narrowed to reading')

    await instance.click('bridge-client-revoke')
    await instance.click('bridge-client-confirm')
    await waitForEvent(
      held,
      'the revocation on the open connection',
      (e) => e.event === 'authorization' && e.authorized === false,
    )
    await waitForEvent(
      held,
      'the closed connection',
      (e) => e.event === 'closed',
    )
    await ctx.waitFor('no row for the revoked client', async () =>
      (await instance.exec<number>(
        `return document.querySelectorAll('${row(firstId)}').length`,
      )) === 0
        ? true
        : undefined,
    )
    await ctx.waitFor('the revoke confirmation closes', async () =>
      (await instance.exec<number>(
        `return document.querySelectorAll('[data-testid="bridge-client-confirm"]').length`,
      )) === 0
        ? true
        : undefined,
    )
    ctx.step('revoked: the held connection hears it and closes')

    const strangerId = stranger.events()[0]?.clientId
    await instance.click(
      row(strangerId, ' [data-testid="bridge-client-unblock"]'),
    )
    await instance.click('bridge-client-confirm')
    const asked = await client('stranger-again', 9)
    await waitForEvent(
      asked,
      'the unblocked client waits',
      (e) => e.event === 'handshake' && e.pending === true,
    )
    await instance.waitForDisplayed('bridge-approval')
    await instance.click('bridge-approval-deny')
    ctx.step('unblocked: the client is asked again')

    const passkeyRequest = {
      action: 'passkey-create',
      payload: {
        relyingPartyId: 'example.de',
        relyingPartyName: 'E2E Passkey Site',
        userHandle: Buffer.from('e2e-user').toString('base64'),
        userName: 'anna@example.de',
        userDisplayName: 'Anna',
        challenge: Buffer.from('a fixed e2e challenge').toString('base64'),
        excludeCredentials: [],
        requireResidentKey: true,
        userVerification: 'preferred',
        origin: 'https://login.example.de',
      },
    }
    const creator = await client('passkey', 13, passkeyRequest)
    await instance.waitForDisplayed('bridge-approval')
    await instance.click('[data-testid="bridge-approval-remember"]')
    await instance.click('bridge-approval-allow')
    await instance.waitForDisplayed('bridge-presence')
    const site = await instance.exec<string>(
      `return document.querySelector('[data-testid="bridge-presence-rp"]')?.textContent?.trim() ?? ''`,
    )
    assert.equal(site, 'example.de')
    await instance.click('bridge-presence-confirm')
    const created = await waitForEvent(
      creator,
      'the new passkey',
      (e) => e.event === 'reply',
    )
    const createdReply = created?.reply as
      { success?: boolean; data?: { passkeyId?: string } } | undefined
    assert.equal(createdReply?.success, true, JSON.stringify(created))
    const passkeyId = createdReply?.data?.passkeyId
    assert.ok(passkeyId)
    ctx.step('a passkey is created after the presence confirmation')

    await openPasswords(instance)
    const entryId = (await ctx.waitFor('the entry of the passkey', async () => {
      const found = (await overview(instance)).headers.find(
        (h) => h.title === 'E2E Passkey Site',
      )
      return found?.id ?? false
    })) as string
    await instance.click(`passwords-entry-${entryId}`)
    await selectTab(instance, 'extra')
    await ctx.waitFor('the slide Extra to rest', () =>
      slideSettled(instance, 'extra'),
    )
    await instance.waitForDisplayed(`passwords-passkey-row-${passkeyId}`)
    ctx.step('the passkey shows in the tab Extra of its new entry')

    const declined = await client('passkey-declined', 13, passkeyRequest)
    await instance.waitForDisplayed('bridge-presence')
    await instance.click('bridge-presence-deny')
    const refused = await waitForEvent(
      declined,
      'the declined passkey',
      (e) => e.event === 'reply',
    )
    const refusedReply = refused?.reply as
      { success?: boolean; errorCode?: string; error?: string } | undefined
    assert.equal(refusedReply?.success, false)
    assert.equal(refusedReply?.errorCode, 'USER_CANCELLED')
    assert.equal(refusedReply?.error, 'USER_CANCELLED')
    ctx.step('a declined confirmation answers USER_CANCELLED')
  } finally {
    for (const running of clients) await running.stop()
  }
})
