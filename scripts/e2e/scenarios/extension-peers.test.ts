import assert from 'node:assert/strict'
import { unwrap } from '../lib/flows.ts'
import { scenario } from '../lib/scenario.ts'
import { fixture, install, openFromLauncher } from '../lib/extensions.ts'
import {
  acceptCard,
  eventsOf,
  ownCardText,
  probe,
  read,
  succeeded,
} from '../lib/peers.ts'
import type { PeerDevice } from '../lib/peers.ts'

// Spec 057 (quickstart §2–§5, tasks T080, T081): three vaults, each with the test extension `peers`
// that declares the permission `peers`; the scene grants it. Anna and Ben are contacts, Dave is
// nobody's contact. Anna invites Ben: his holzi asks and his extension hears nothing until he
// accepts. The channel carries fast, timed and reliable messages and ends for both sides. A lobby for
// contacts refuses Dave and a removed Ben; a public lobby takes Dave as not verified. Killing a
// device ends its channel for the other one as `timeout`. The devices meet through the scene's iroh
// relay; run with `HOLZI_E2E_SYNC_LOOPBACK=1` the devices bind loopback only (T081).

/** The most a channel waits to notice a device that vanished without a word (FR-039: 30 s idle). */
const IDLE_MS = 30_000
/** QUIC may wait one keep-alive interval after the last idle-timeout tick. */
const IDLE_DETECTION_MS = IDLE_MS + 5_000
/** The software-rendered Android emulator adds relay latency to the desktop baseline. */
const MAX_MEDIAN_DELAY_MS = process.env.E2E_PLATFORM === 'android' ? 2_000 : 100

scenario('extension-peers', { timeoutMs: 600_000 }, async (ctx) => {
  // Ben has a second device: a Visitenkarte names the relay only of the devices its issuer has seen
  // (never its own), so Anna reaches Ben through his tablet, where the prompt appears.
  const group = await ctx.group({
    users: { anna: ['laptop'], ben: ['desktop', 'tablet'], dave: ['pc'] },
    irohRelay: true,
  })
  const pages = {
    anna: group.device('anna/laptop').page,
    ben: group.device('ben/desktop').page,
    tablet: group.device('ben/tablet').page,
    dave: group.device('dave/pc').page,
  }
  const extensions = {
    anna: await install(pages.anna, fixture('e2e', 'peers')),
    ben: await install(pages.ben, fixture('e2e', 'peers')),
    dave: await install(pages.dave, fixture('e2e', 'peers')),
  }
  // Installed unticked, the extension would ask at its first call; the scene grants what it declares.
  for (const [name, extension] of Object.entries(extensions)) {
    for (const [kind, action] of [
      ['peers', 'connect'],
      ['contacts', 'read'],
    ]) {
      unwrap(
        'extension_permission_set',
        await pages[name as keyof typeof extensions].invoke(
          'extension_permission_set',
          {
            args: {
              extensionId: extension.id,
              kind,
              action,
              target: '*',
              status: 'granted',
            },
          },
        ),
      )
    }
  }
  // The tablet gets the extension and its permissions through the sync of Ben's devices.
  await ctx.waitFor(
    'the tablet to hold the extension and its permission',
    async () => {
      const listed = unwrap<Array<{ id: string; statusHere?: string }>>(
        'extension_list',
        await pages.tablet.invoke('extension_list'),
      )
      const permissions = unwrap<Array<{ kind: string; status: string }>>(
        'extension_permissions_list',
        await pages.tablet.invoke('extension_permissions_list', {
          extensionId: extensions.ben.id,
        }),
      )
      return (
        listed.some(
          (e) => e.id === extensions.ben.id && e.statusHere === 'ready',
        ) &&
        permissions.some((p) => p.kind === 'peers' && p.status === 'granted')
      )
    },
    { timeoutMs: 90_000, fixed: true },
  )
  const devices: Record<string, PeerDevice> = {}
  for (const [name, page, extension] of [
    ['anna', pages.anna, extensions.anna],
    ['ben', pages.ben, extensions.ben],
    ['tablet', pages.tablet, extensions.ben],
    ['dave', pages.dave, extensions.dave],
  ] as const) {
    await openFromLauncher(page, extension)
    devices[name] = { page, extension }
  }
  const anna = devices.anna!
  const ben = devices.ben!
  const tablet = devices.tablet!
  const dave = devices.dave!
  ctx.step(
    'the test extension is installed and may open channels on four devices',
  )

  const [annaCard, benCard] = [
    await ownCardText(pages.anna, 'Anna'),
    await ownCardText(pages.ben, 'Ben'),
  ]
  const benOnAnna = await acceptCard(pages.anna, benCard)
  await acceptCard(pages.ben, annaCard)
  // The contacts of Ben's desktop reach his tablet, which asks about Anna by her card.
  await ctx.waitFor(
    'Ben’s tablet to hold Anna as a contact',
    async () =>
      unwrap<unknown[]>(
        'contacts_list',
        await pages.tablet.invoke('contacts_list'),
      ).length >= 2,
    { timeoutMs: 90_000, fixed: true },
  )
  ctx.step('Anna and Ben are contacts of each other, Dave is nobody’s')

  // An invitation: Ben's holzi asks, his extension hears nothing before the answer.
  const invited = await succeeded<{ inviteId: string }>(
    anna,
    'invite',
    benOnAnna,
    {
      map: 'e1m1',
    },
  )
  try {
    await pages.tablet.waitForDisplayed(
      'extension-peer-invitation-accept',
      60_000,
    )
  } catch (error) {
    // The usual cause is a card without a way to the device: Anna's status says so.
    const statuses = await eventsOf(anna, 'invite-status')
    throw new Error(
      `${String(error)}; Anna's invitation status: ${JSON.stringify(statuses)}`,
      { cause: error },
    )
  }
  assert.deepEqual(await eventsOf(tablet, 'channel-opened'), [])
  assert.deepEqual(await eventsOf(anna, 'invite-status'), [])
  ctx.step('Ben’s tablet is asked in holzi’s prompt; no frame has heard of it')

  await pages.tablet.click('extension-peer-invitation-accept')
  await ctx.waitFor(
    'Anna to hear that the invitation was accepted',
    async () => (await eventsOf(anna, 'invite-status')).length === 1,
    { timeoutMs: 60_000 },
  )
  const [status] = await eventsOf(anna, 'invite-status')
  assert.equal(status!.data.status, 'accepted')
  assert.equal(status!.data.inviteId, invited.inviteId)
  const annaChannel = String(status!.data.channelId)
  await ctx.waitFor(
    'the extension of Ben’s tablet to hear that the channel opened',
    async () => (await eventsOf(tablet, 'channel-opened')).length === 1,
  )
  const [opened] = await eventsOf(tablet, 'channel-opened')
  assert.equal(opened!.data.origin, 'invitation')
  assert.equal(opened!.data.inviteId, invited.inviteId)
  assert.equal((opened!.data.from as { name: string }).name, 'Anna')
  assert.deepEqual(opened!.data.payload, { map: 'e1m1' })
  const tabletChannel = String(opened!.data.channelId)
  ctx.step('accepted: one status for Anna, one channel for both')

  // Messages in both directions, the limits, and the timing of fast ones.
  await succeeded(anna, 'send', annaChannel, 'hi Ben')
  await succeeded(tablet, 'send', tabletChannel, 'hi Anna', true)
  await succeeded(anna, 'sendFiller', annaChannel, 65_536, true)
  type Counters = { fast: number; reliable: number }
  await ctx.waitFor(
    'both sides to receive',
    async () => {
      const [onBen, onAnna] = await Promise.all([
        read<Counters>(tablet, 'channel', tabletChannel),
        read<Counters>(anna, 'channel', annaChannel),
      ])
      return onBen.fast >= 1 && onBen.reliable >= 1 && onAnna.reliable >= 1
    },
    { timeoutMs: 30_000 },
  )
  const tooBig = await probe(anna, 'sendFiller', annaChannel, 1025, false)
  assert.equal(tooBig.error?.code, 7000)
  assert.equal(tooBig.error?.reason, 'too_large')
  assert.equal(tooBig.error?.limit, 1024)
  ctx.step('fast and reliable messages arrive, 1,025 bytes are too large')

  const count = 175 // five seconds at 35 messages a second
  const timed = await succeeded<{
    sent: number
    failed: number
    elapsedMs: number
  }>(anna, 'sendTimed', annaChannel, count, 35)
  assert.equal(timed.failed, 0)
  assert.ok(
    timed.sent / (timed.elapsedMs / 1000) >= 35,
    `send rate ${timed.sent / (timed.elapsedMs / 1000)} messages/s`,
  )
  await ctx.waitFor(
    'the timed messages to arrive',
    async () => (await read<number[]>(tablet, 'delays')).length >= count * 0.99,
    { timeoutMs: 30_000 },
  )
  const delays = (await read<number[]>(tablet, 'delays')).sort((a, b) => a - b)
  const median = delays[Math.floor(delays.length / 2)]!
  const p95 = delays[Math.floor(delays.length * 0.95)]!
  ctx.step(
    'timed messages',
    `${delays.length}/${count} arrived, delay median ${median} ms, p95 ${p95} ms`,
  )
  assert.ok(
    median < MAX_MEDIAN_DELAY_MS,
    `median delay ${median} ms (limit ${MAX_MEDIAN_DELAY_MS} ms)`,
  )

  await succeeded(anna, 'close', annaChannel)
  for (const [device, channel] of [
    [anna, annaChannel],
    [tablet, tabletChannel],
  ] as const) {
    await ctx.waitFor(
      'the end of the channel on both sides',
      async () =>
        (await eventsOf(device, 'channel-closed')).filter(
          (event) => event.data.channelId === channel,
        ).length === 1,
    )
    const [end] = await eventsOf(device, 'channel-closed')
    assert.equal(end!.data.reason, 'closed')
  }
  ctx.step('closed: both sides hear it once')

  // A lobby for contacts: Ben joins, Dave and a removed Ben do not.
  const lobby = await succeeded<{ joinCode: string }>(
    anna,
    'openLobby',
    'arena',
    'contacts',
  )
  assert.ok(lobby.joinCode.startsWith('haex-vault:join:'))
  const joinedBen = await succeeded<{ channelId: string; lobbyId: string }>(
    ben,
    'join',
    lobby.joinCode,
    'Ben',
    { skin: 2 },
  )
  assert.equal(joinedBen.lobbyId, 'arena')
  await ctx.waitFor(
    'Anna to hear that Ben joined',
    async () => (await eventsOf(anna, 'joined')).length === 1,
  )
  const [joinedEvent] = await eventsOf(anna, 'joined')
  assert.equal(joinedEvent!.data.verified, true)
  assert.equal(joinedEvent!.data.displayName, 'Ben')
  assert.deepEqual(joinedEvent!.data.payload, { skin: 2 })
  const daveRefused = await probe(dave, 'join', lobby.joinCode, 'Dave', {})
  assert.equal(daveRefused.error?.reason, 'not_admitted')
  assert.equal(daveRefused.error?.code, 1002)
  ctx.step('a contact joins the lobby for contacts, a stranger is refused')

  await succeeded(anna, 'remove', String(joinedEvent!.data.channelId))
  await ctx.waitFor('Ben to lose the channel', async () =>
    (await eventsOf(ben, 'channel-closed')).some(
      (event) => event.data.channelId === joinedBen.channelId,
    ),
  )
  const again = await probe(ben, 'join', lobby.joinCode, 'Ben', {})
  assert.equal(again.error?.reason, 'not_admitted')
  await succeeded(anna, 'closeLobby', 'arena')
  const late = await probe(ben, 'join', lobby.joinCode, 'Ben', {})
  assert.equal(late.error?.reason, 'lobby_closed')
  assert.equal(late.error?.code, 1001)
  ctx.step(
    'a removed player cannot return, a closed lobby answers lobby_closed',
  )

  // A public lobby takes Dave as not verified.
  const open = await succeeded<{ joinCode: string }>(
    anna,
    'openLobby',
    'open-arena',
    'public',
  )
  await succeeded(dave, 'join', open.joinCode, 'Dave', {})
  await ctx.waitFor(
    'Anna to hear that Dave joined',
    async () => (await eventsOf(anna, 'joined')).length === 2,
  )
  const daveJoined = (await eventsOf(anna, 'joined')).at(-1)!
  assert.equal(daveJoined.data.verified, false)
  assert.equal(daveJoined.data.displayName, 'Dave')
  assert.equal(daveJoined.data.contactId, undefined)
  ctx.step('the public lobby takes a stranger as not verified')

  // A device that vanishes without a word ends the channel after the idle time (FR-039).
  await group.device('dave/pc').kill()
  const killedAt = Date.now()
  await ctx.waitFor(
    'Anna to hear that Dave’s channel timed out',
    async () =>
      (await eventsOf(anna, 'channel-closed')).some(
        (event) => event.data.channelId === daveJoined.data.channelId,
      ),
    { timeoutMs: IDLE_MS * 3 },
  )
  const end = (await eventsOf(anna, 'channel-closed')).find(
    (event) => event.data.channelId === daveJoined.data.channelId,
  )!
  assert.equal(end.data.reason, 'timeout')
  assert.ok(
    Date.now() - killedAt <= IDLE_DETECTION_MS,
    `channel timed out after ${Date.now() - killedAt} ms`,
  )
  ctx.step(
    'a vanished device',
    `Anna heard "timeout" ${Date.now() - killedAt} ms after the kill`,
  )
})
