// The development side of the developer-mode scene (spec 017, US12): a project folder as the SDK
// writes it and a development server for the probe page on 127.0.0.1.
import { generateKeyPairSync } from 'node:crypto'
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { createServer, type Server } from 'node:http'
import type { AddressInfo } from 'node:net'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { reachFromDevice } from './platform/reach.ts'

const FIXTURES = fileURLToPath(
  new URL('../../../src-tauri/tests/fixtures/extension_e2e/', import.meta.url),
)

/** A development server for the probe page; `title` is what the page shows next. */
export function devServer(): Promise<{
  server: Server
  port: number
  title: { text: string }
}> {
  const title = { text: 'Probe dev' }
  const server = createServer((request, response) => {
    // A sandboxed frame has an opaque origin: module scripts need CORS, as Vite's server sends it.
    response.setHeader('Access-Control-Allow-Origin', '*')
    const path = (request.url ?? '/').split('?')[0]
    if (path === '/' || path === '/index.html') {
      const page = readFileSync(join(FIXTURES, 'probe.html'), 'utf8').replace(
        '<h1 id="title">Probe</h1>',
        `<h1 id="title">${title.text}</h1>`,
      )
      response.writeHead(200, { 'Content-Type': 'text/html' })
      response.end(page)
      return
    }
    if (path === '/probe.js') {
      response.writeHead(200, { 'Content-Type': 'text/javascript' })
      response.end(readFileSync(join(FIXTURES, 'probe.js')))
      return
    }
    response.writeHead(404)
    response.end()
  })
  return new Promise((resolve) => {
    server.listen(0, '127.0.0.1', () => {
      const port = (server.address() as AddressInfo).port
      reachFromDevice(port)
      resolve({ server, port, title })
    })
  })
}

/** A project folder for the server on `port`, with a fresh publisher key. */
export function devProject(port: number): string {
  const dir = mkdtempSync(join(tmpdir(), 'haex-dev-project-'))
  writeFileSync(
    join(dir, 'haextension.config.json'),
    JSON.stringify({ dev: { host: '127.0.0.1', port } }),
  )
  const key = generateKeyPairSync('ed25519')
    .publicKey.export({ format: 'der', type: 'spki' })
    .subarray(-32)
    .toString('hex')
  // As `haex init` sets a project up: name and version in package.json, not in the manifest.
  writeFileSync(
    join(dir, 'package.json'),
    JSON.stringify({ name: 'devprobe', version: '0.1.0' }),
  )
  mkdirSync(join(dir, 'haextension'))
  writeFileSync(
    join(dir, 'haextension', 'manifest.json'),
    JSON.stringify({ publicKey: key, displayName: 'Dev Probe' }),
  )
  return dir
}
