// A local SFTP server for the storage scenarios (spec 050, T080): `rclone serve sftp` in Docker on
// a free port of the loopback interface, with a host key and the user's authorized public keys
// from a temporary folder, so a scenario can register a key holzi generated and restart the
// server with another host key on the same port. Reached from an Android emulator like RustFS.
import { spawnSync } from 'node:child_process'
import { createHash, createPublicKey, generateKeyPairSync } from 'node:crypto'
import {
  mkdirSync,
  mkdtempSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { reservePort } from './ports.ts'
import { reachFromDevice } from './platform/reach.ts'
import { RCLONE_IMAGE } from './webdav.ts'

export interface Sftp {
  /** `sftp://127.0.0.1:<port>`: a local address, so the connection gets the scope `local`. */
  endpoint: string
  username: string
  /** Creates a folder below the server root for scenario setup. */
  makeFolder(name: string): void
  /** Lists a folder below the server root for scenario assertions. */
  listFolder(name: string): string[]
  /** `SHA256:…` of the host key the server shows now, as holzi and OpenSSH name it. */
  fingerprint: string
  /** Lets the user sign in with the public key line `line` (`ssh-ed25519 AAAA… holzi`). */
  authorize(line: string): void
  /** Starts the server, or starts it again with a new host key. */
  startAsync(options?: { newHostKey?: boolean }): Promise<void>
  stop(): void
}

/** An SSH string: its length, then its bytes. */
function sshString(bytes: Buffer): Buffer {
  const length = Buffer.alloc(4)
  length.writeUInt32BE(bytes.length)
  return Buffer.concat([length, bytes])
}

/** A new Ed25519 host key: the PEM for the server and its OpenSSH fingerprint. */
function hostKey(): { pem: string; fingerprint: string } {
  const { privateKey } = generateKeyPairSync('ed25519')
  const pem = privateKey.export({ format: 'pem', type: 'pkcs8' }).toString()
  const jwk = createPublicKey(privateKey).export({ format: 'jwk' })
  const raw = Buffer.from(jwk.x ?? '', 'base64url')
  const blob = Buffer.concat([
    sshString(Buffer.from('ssh-ed25519')),
    sshString(raw),
  ])
  const digest = createHash('sha256').update(blob).digest('base64')
  return { pem, fingerprint: `SHA256:${digest.replace(/=+$/, '')}` }
}

/**
 * Prepares a server on a free port; `startAsync` starts it. The port stays reserved until then: a
 * scenario starts its devices in between, and their drivers and relays pick free ports too.
 */
export async function prepareSftp(): Promise<Sftp> {
  const reserved = await reservePort()
  const port = reserved.port
  reachFromDevice(port)
  const base = mkdtempSync(join(tmpdir(), 'holzi-sftp-'))
  const keys = join(base, 'keys')
  const root = join(base, 'data')
  mkdirSync(keys)
  mkdirSync(root)
  const authorized: string[] = []
  let container: string | null = null
  const server: Sftp = {
    endpoint: `sftp://127.0.0.1:${port}`,
    username: 'anna',
    makeFolder(name) {
      mkdirSync(join(root, name))
    },
    listFolder(name) {
      return readdirSync(join(root, name))
    },
    fingerprint: '',
    authorize(line) {
      authorized.push(line.trim())
    },
    async startAsync(options = {}) {
      if (container) spawnSync('docker', ['rm', '-f', container])
      if (!server.fingerprint || options.newHostKey) {
        const key = hostKey()
        writeFileSync(join(keys, 'host'), key.pem, { mode: 0o600 })
        server.fingerprint = key.fingerprint
      }
      // rclone 1.71.1 hangs on an empty file of authorized keys.
      if (authorized.length === 0)
        throw new Error('authorize a key before the server starts')
      writeFileSync(join(keys, 'authorized_keys'), `${authorized.join('\n')}\n`)
      await reserved.release()
      const run = spawnSync(
        'docker',
        [
          'run',
          '-d',
          '--rm',
          '-p',
          `127.0.0.1:${port}:2022`,
          '-v',
          `${keys}:/keys:ro`,
          '-v',
          `${root}:/data`,
          '--user',
          `${process.getuid?.() ?? 0}:${process.getgid?.() ?? 0}`,
          RCLONE_IMAGE,
          'serve',
          'sftp',
          '/data',
          '--addr',
          ':2022',
          '--user',
          server.username,
          '--key',
          '/keys/host',
          '--authorized-keys',
          '/keys/authorized_keys',
        ],
        { encoding: 'utf8', timeout: 300_000 },
      )
      if (run.status !== 0)
        throw new Error(`rclone did not start: ${run.stderr}`)
      container = run.stdout.trim()
      const deadline = Date.now() + 60_000
      for (;;) {
        const logs = spawnSync('docker', ['logs', container], {
          encoding: 'utf8',
        })
        if (/SFTP server listening/i.test(`${logs.stdout}${logs.stderr}`))
          return
        if (Date.now() > deadline) throw new Error('rclone sftp did not start')
        await new Promise((resolve) => setTimeout(resolve, 300))
      }
    },
    stop() {
      if (container) spawnSync('docker', ['rm', '-f', container])
      void reserved.release()
      rmSync(base, { recursive: true, force: true })
    },
  }
  return server
}
