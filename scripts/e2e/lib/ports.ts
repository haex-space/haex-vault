import net from 'node:net'

export class PortInUseError extends Error {
  port: number

  constructor(port: number) {
    super(`port ${port} is already in use`)
    this.name = 'PortInUseError'
    this.port = port
  }
}

/** A loopback port held by a listening socket until `release`, so no `freePort` hands it out meanwhile. */
export interface ReservedPort {
  port: number
  release(): Promise<void>
}

/**
 * Holds a free loopback port for a server that starts later, such as a container whose address a
 * scenario types into holzi first. Release it right before the server binds it.
 */
export function reservePort(): Promise<ReservedPort> {
  return new Promise((resolve, reject) => {
    const server = net.createServer()
    server.once('error', reject)
    server.listen(0, '127.0.0.1', () => {
      const address = server.address()
      if (address === null || typeof address === 'string') {
        server.close()
        reject(new Error('the operating system gave no port'))
        return
      }
      resolve({
        port: address.port,
        release: () =>
          new Promise((done) => {
            if (!server.listening) return done()
            server.close(() => done())
          }),
      })
    })
  })
}

/**
 * A port on the loopback interface that is free at this moment (FR-009).
 * ponytail: the port is bound and released, so another process can take it before the caller uses it.
 * One retry covers that in practice (`withPortRetry`); the upgrade path is to hand the caller an already
 * bound socket, which needs a driver that accepts an inherited one. A server that starts long after the
 * port is chosen holds it with `reservePort` instead.
 */
export async function freePort(): Promise<number> {
  const reserved = await reservePort()
  await reserved.release()
  return reserved.port
}

/** Run an operation that picks its own ports, and run it once more if a port turned out to be taken. */
export async function withPortRetry<T>(
  operation: (attempt: number) => Promise<T>,
): Promise<T> {
  try {
    return await operation(0)
  } catch (error) {
    if (!(error instanceof PortInUseError)) throw error
  }
  return operation(1)
}
