// The test extension of spec 057 (T069): invites, opens lobbies, joins, sends and closes through
// `client.peers` of the released vault-sdk, and keeps every `peers:*` event it gets. The e2e scene
// `extension-peers` drives it through `window.peersProbe` and reads what arrived from there; the
// page shows the events as well. `build.sh` bundles this file with the SDK into app.js.
import { createHaexVaultSdk } from '@haex-space/vault-sdk'
import manifest from './manifest.json'

const sdk = createHaexVaultSdk({ manifest })

/** A timed fast message starts with this byte, then the sender's `Date.now()` as a float64. */
const TIMED = 0x54
const encoder = new TextEncoder()
const decoder = new TextDecoder()

/** Every event, in the order it arrived; a message is summarised, not kept whole. */
const events = []
/** Per channel: what arrived, for the counters of the scene. */
const channels = {}
/** Milliseconds from sending to arrival of the timed fast messages (SC-004). Both sides of a
 * scene run on one machine, so their clocks agree. */
const delays = []

function channelOf(id) {
  channels[id] ??= { fast: 0, reliable: 0, texts: [] }
  return channels[id]
}

function render() {
  document.getElementById('count').textContent = String(events.length)
  document.getElementById('events').textContent = events
    .slice(-30)
    .map((event) => `${event.type} ${JSON.stringify(event.data)}`)
    .join('\n')
}

function record(type, data) {
  events.push({ type, data, at: Date.now() })
  render()
}

function onMessage(message) {
  const channel = channelOf(message.channelId)
  const bytes = message.data
  if (message.reliable) channel.reliable += 1
  else channel.fast += 1
  if (!message.reliable && bytes[0] === TIMED && bytes.length >= 9) {
    const sent = new DataView(bytes.buffer, bytes.byteOffset).getFloat64(1)
    delays.push(Date.now() - sent)
    return
  }
  const text = decoder.decode(bytes)
  channel.texts.push(text)
  record('message', {
    channelId: message.channelId,
    reliable: message.reliable,
    size: bytes.length,
  })
}

for (const type of [
  'invite-status',
  'channel-opened',
  'joined',
  'channel-closed',
]) {
  sdk.peers.on(type, (data) => record(type, data))
}
sdk.peers.on('message', onMessage)

/** What the scene reads of a failed call: the numbers and names of the SDK's `PeerError`. */
function failure(error) {
  return {
    error: {
      code: error.code ?? null,
      reason: error.reason ?? null,
      kind: error.kind ?? null,
      limit: error.limit ?? null,
      message: String(error.message ?? error),
    },
  }
}

/** Runs `call`; the answer is `{ ok: … }` or `{ error: … }`, never a rejection. */
async function attempt(call) {
  try {
    return { ok: (await call()) ?? null }
  } catch (error) {
    return failure(error)
  }
}

window.peersProbe = {
  limits: {
    fast: sdk.peers.MAX_FAST_BYTES,
    reliable: sdk.peers.MAX_RELIABLE_BYTES,
    payload: sdk.peers.MAX_PAYLOAD_BYTES,
  },
  invite: (contactId, payload) =>
    attempt(() => sdk.peers.inviteAsync(contactId, payload)),
  cancelInvite: (inviteId) =>
    attempt(() => sdk.peers.cancelInviteAsync(inviteId)),
  openLobby: (lobbyId, visibility) =>
    attempt(() => sdk.peers.openLobbyAsync(lobbyId, visibility)),
  closeLobby: (lobbyId) => attempt(() => sdk.peers.closeLobbyAsync(lobbyId)),
  join: (joinCode, displayName, payload) =>
    attempt(() => sdk.peers.joinAsync(joinCode, displayName, payload)),
  remove: (channelId) => attempt(() => sdk.peers.removeAsync(channelId)),
  /** Sends `text` as UTF-8, fast or reliable. */
  send: (channelId, text, reliable = false) =>
    attempt(() =>
      sdk.peers.sendAsync(channelId, encoder.encode(text), { reliable }),
    ),
  /** Sends `size` bytes of filler, fast or reliable, to test the limits. */
  sendFiller: (channelId, size, reliable = false) =>
    attempt(() =>
      sdk.peers.sendAsync(channelId, new Uint8Array(size).fill(0x61), {
        reliable,
      }),
    ),
  /** Sends `count` timed fast messages of 1,024 bytes, `perSecond` of them every second. */
  async sendTimed(channelId, count, perSecond) {
    const gap = 1000 / perSecond
    const started = performance.now()
    let failed = 0
    for (let i = 0; i < count; i++) {
      const bytes = new Uint8Array(sdk.peers.MAX_FAST_BYTES)
      bytes[0] = TIMED
      new DataView(bytes.buffer).setFloat64(1, Date.now())
      const sent = await attempt(() =>
        sdk.peers.sendAsync(channelId, bytes, { reliable: false }),
      )
      if (sent.error) failed += 1
      const next = started + (i + 1) * gap
      const wait = next - performance.now()
      if (wait > 0) await new Promise((resolve) => setTimeout(resolve, wait))
    }
    return {
      ok: { sent: count, failed, elapsedMs: performance.now() - started },
    }
  },
  close: (channelId) => attempt(() => sdk.peers.closeAsync(channelId)),
  /** The events so far, optionally of one type; nothing of a message but its size. */
  events: (type) =>
    events.filter((event) => type === undefined || event.type === type),
  /** The counters of one channel. */
  channel: (channelId) => channelOf(channelId),
  /** The delays of the timed messages that arrived. */
  delays: () => delays.slice(),
}
