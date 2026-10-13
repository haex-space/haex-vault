import { describe, it } from 'node:test'
import assert from 'node:assert/strict'
import net from 'node:net'
import {
  PortInUseError,
  freePort,
  reservePort,
  withPortRetry,
} from './ports.ts'

function listen(port: number): Promise<net.Server> {
  return new Promise((resolve, reject) => {
    const server = net.createServer()
    server.once('error', reject)
    server.listen(port, '127.0.0.1', () => resolve(server))
  })
}

describe('ports', () => {
  it('returns a port from 1024 to 65535 that can be bound at once', async () => {
    const port = await freePort()
    assert.ok(Number.isInteger(port) && port >= 1024 && port <= 65535)
    const server = await listen(port)
    server.close()
  })

  it('holds a reserved port until it is released', async () => {
    const reserved = await reservePort()
    await assert.rejects(listen(reserved.port), { code: 'EADDRINUSE' })
    for (let i = 0; i < 50; i++)
      assert.notEqual(await freePort(), reserved.port)
    await reserved.release()
    await reserved.release()
    const server = await listen(reserved.port)
    server.close()
  })

  it('retries exactly once when the port turns out to be taken', async () => {
    let calls = 0
    const result = await withPortRetry(async (attempt) => {
      calls += 1
      if (attempt === 0) throw new PortInUseError(4242)
      return 'ok'
    })
    assert.equal(result, 'ok')
    assert.equal(calls, 2)
  })

  it('rethrows the second failure with the port in the message', async () => {
    let calls = 0
    await assert.rejects(
      withPortRetry(async () => {
        calls += 1
        throw new PortInUseError(1234)
      }),
      /1234/,
    )
    assert.equal(calls, 2)
  })

  it('does not retry any other error', async () => {
    let calls = 0
    await assert.rejects(
      withPortRetry(async () => {
        calls += 1
        throw new Error('something else')
      }),
      /something else/,
    )
    assert.equal(calls, 1)
  })
})
