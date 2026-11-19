import assert from 'node:assert/strict'
import { describe, it } from 'node:test'
import { PORT_VARIABLE, createRelay } from './nostr-relay.ts'
import type { RelayProcess } from './nostr-relay.ts'

function launcher() {
  const log: string[] = []
  return {
    log,
    launch: async (port: number): Promise<RelayProcess> => {
      log.push(`start ${port}`)
      return { stop: async () => void log.push(`stop ${port}`) }
    },
  }
}

describe('a relay that can go down and come back', () => {
  it('has its address before it starts and keeps it', async () => {
    const { launch, log } = launcher()
    const relay = createRelay(41234, launch)
    assert.equal(relay.url, 'ws://127.0.0.1:41234')
    assert.equal(relay.state, 'down')
    await relay.start()
    assert.equal(relay.state, 'up')
    await relay.stop()
    assert.equal(relay.state, 'down')
    await relay.start()
    assert.equal(relay.url, 'ws://127.0.0.1:41234')
    assert.deepEqual(log, ['start 41234', 'stop 41234', 'start 41234'])
  })

  it('refuses a second start while it is up', async () => {
    const { launch } = launcher()
    const relay = createRelay(41235, launch)
    await relay.start()
    await assert.rejects(relay.start(), /already up/)
    assert.equal(relay.state, 'up')
  })

  it('can be stopped twice', async () => {
    const { launch, log } = launcher()
    const relay = createRelay(41236, launch)
    await relay.start()
    await relay.stop()
    await relay.stop()
    assert.deepEqual(log, ['start 41236', 'stop 41236'])
  })

  it('stays down when the start fails', async () => {
    const relay = createRelay(41237, async () => {
      throw new Error('no binary')
    })
    await assert.rejects(relay.start(), /no binary/)
    assert.equal(relay.state, 'down')
  })

  it('names the variable the binary reads its port from', () => {
    assert.equal(PORT_VARIABLE, 'HAEX_E2E_RELAY_PORT')
  })
})
