// A local WebDAV server for the storage scenarios (spec 050, T036): `rclone serve webdav` in Docker
// on a free port of the loopback interface, with a generated user and password and an empty folder
// tree, ended with the scenario. rclone is the fallback of research R14: one image, two flags, and
// it can serve SFTP later. Reached from an Android emulator like RustFS (`reachFromDevice`).
import { spawnSync } from 'node:child_process'
import { randomBytes } from 'node:crypto'
import { freePort } from './ports.ts'
import { reachFromDevice } from './platform/reach.ts'

export const RCLONE_IMAGE = 'rclone/rclone:1.71.1'

export interface Webdav {
  /** `http://127.0.0.1:<port>/`: a local address, so the connection gets the scope `local`. */
  endpoint: string
  username: string
  password: string
  stop(): void
}

function authorization(server: Pick<Webdav, 'username' | 'password'>): string {
  const pair = `${server.username}:${server.password}`
  return `Basic ${Buffer.from(pair).toString('base64')}`
}

/** One request to the server, as another program would send it. */
export async function davRequest(
  server: Webdav,
  method: string,
  path: string,
  init: { body?: Uint8Array | string; headers?: Record<string, string> } = {},
): Promise<Response> {
  return fetch(new URL(path, server.endpoint), {
    method,
    body: init.body,
    headers: { authorization: authorization(server), ...init.headers },
  })
}

/** Starts the server; `stop` removes the container with its files. */
export async function startWebdav(): Promise<Webdav> {
  const port = await freePort()
  reachFromDevice(port)
  const username = `e2e${randomBytes(4).toString('hex')}`
  const password = randomBytes(12).toString('hex')
  const run = spawnSync(
    'docker',
    [
      'run',
      '-d',
      '--rm',
      '-p',
      `127.0.0.1:${port}:8080`,
      RCLONE_IMAGE,
      'serve',
      'webdav',
      '/tmp',
      '--addr',
      ':8080',
      '--user',
      username,
      '--pass',
      password,
    ],
    { encoding: 'utf8', timeout: 300_000 },
  )
  if (run.status !== 0) throw new Error(`rclone did not start: ${run.stderr}`)
  const container = run.stdout.trim()
  const server: Webdav = {
    endpoint: `http://127.0.0.1:${port}/`,
    username,
    password,
    stop: () => {
      spawnSync('docker', ['rm', '-f', container], { stdio: 'ignore' })
    },
  }
  try {
    const deadline = Date.now() + 60_000
    for (;;) {
      const ready = await davRequest(server, 'PROPFIND', '/', {
        headers: { depth: '0' },
      }).then(
        (r) => r.status === 207,
        () => false,
      )
      if (ready) break
      if (Date.now() > deadline) throw new Error('rclone did not answer')
      await new Promise((resolve) => setTimeout(resolve, 500))
    }
  } catch (error) {
    server.stop()
    throw error
  }
  return server
}

/** The names in the folder `path` (with `/` at the end), as one level of a listing shows them. */
export async function listFolder(
  server: Webdav,
  path: string,
): Promise<string[]> {
  const answer = await davRequest(server, 'PROPFIND', path, {
    headers: { depth: '1' },
  })
  if (answer.status !== 207)
    throw new Error(`PROPFIND ${path}: ${answer.status}`)
  const text = await answer.text()
  const hrefs = [...text.matchAll(/<(?:\w+:)?href>([^<]*)<\/(?:\w+:)?href>/g)]
  return hrefs
    .map((m) => decodeURIComponent(m[1] ?? ''))
    .filter((href) => href.replace(/\/$/, '') !== path.replace(/\/$/, ''))
    .map((href) => href.replace(/\/$/, '').split('/').pop() ?? '')
}
