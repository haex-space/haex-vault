// A local RustFS for the storage scenarios (spec 038, T031): started in Docker on a free port of the
// loopback interface, with a generated access key, and one bucket made before the app sees it. The
// bucket is created with a hand-signed request (AWS Signature V4), since holzi itself never
// creates buckets. Ended with the scenario.
import { spawnSync } from 'node:child_process'
import { createHash, createHmac, randomBytes } from 'node:crypto'
import { freePort } from './ports.ts'
import { reachFromDevice } from './platform/reach.ts'

export const RUSTFS_IMAGE = 'rustfs/rustfs:latest'
const REGION = 'us-east-1'

export interface Rustfs {
  /** `http://127.0.0.1:<port>`: a local address, so the connection gets the endpoint scope `local`. */
  endpoint: string
  accessKeyId: string
  secretAccessKey: string
  region: string
  stop(): void
}

let available: boolean | undefined

/** Whether `docker` answers here; the storage scenarios are skipped without it. */
export function containerRuntimeAvailable(): boolean {
  available ??=
    spawnSync('docker', ['info'], { stdio: 'ignore', timeout: 20_000 })
      .status === 0
  return available
}

const sha256 = (data: string | Uint8Array) =>
  createHash('sha256').update(data).digest('hex')
const hmac = (key: Buffer | string, data: string) =>
  createHmac('sha256', key).update(data).digest()

/** Sends one request with an empty body, signed with AWS Signature V4. */
export async function signedRequest(
  server: Pick<
    Rustfs,
    'endpoint' | 'accessKeyId' | 'secretAccessKey' | 'region'
  >,
  method: string,
  path: string,
  body?: Uint8Array,
): Promise<Response> {
  const url = new URL(path, server.endpoint)
  const stamp = new Date()
    .toISOString()
    .replace(/[-:]/g, '')
    .replace(/\.\d+/, '')
  const day = stamp.slice(0, 8)
  const payload = sha256(body ?? '')
  const headers = `host:${url.host}\nx-amz-content-sha256:${payload}\nx-amz-date:${stamp}\n`
  const signed = 'host;x-amz-content-sha256;x-amz-date'
  const canonical = [method, url.pathname, '', headers, signed, payload].join(
    '\n',
  )
  const scope = `${day}/${server.region}/s3/aws4_request`
  const toSign = ['AWS4-HMAC-SHA256', stamp, scope, sha256(canonical)].join(
    '\n',
  )
  let key: Buffer = hmac(`AWS4${server.secretAccessKey}`, day)
  for (const part of [server.region, 's3', 'aws4_request'])
    key = hmac(key, part)
  const signature = createHmac('sha256', key).update(toSign).digest('hex')
  return fetch(url, {
    method,
    body,
    headers: {
      'x-amz-content-sha256': payload,
      'x-amz-date': stamp,
      authorization: `AWS4-HMAC-SHA256 Credential=${server.accessKeyId}/${scope}, SignedHeaders=${signed}, Signature=${signature}`,
    },
  })
}

/** Starts RustFS and makes `buckets`; `stop` removes the container with its data. */
export async function startRustfs(buckets: readonly string[]): Promise<Rustfs> {
  const port = await freePort()
  reachFromDevice(port)
  const accessKeyId = `e2e${randomBytes(6).toString('hex')}`
  const secretAccessKey = randomBytes(18).toString('hex')
  const run = spawnSync(
    'docker',
    [
      'run',
      '-d',
      '--rm',
      '-p',
      `127.0.0.1:${port}:9000`,
      '-e',
      `RUSTFS_ACCESS_KEY=${accessKeyId}`,
      '-e',
      `RUSTFS_SECRET_KEY=${secretAccessKey}`,
      RUSTFS_IMAGE,
    ],
    { encoding: 'utf8', timeout: 300_000 },
  )
  if (run.status !== 0) throw new Error(`RustFS did not start: ${run.stderr}`)
  const container = run.stdout.trim()
  const server: Rustfs = {
    endpoint: `http://127.0.0.1:${port}`,
    accessKeyId,
    secretAccessKey,
    region: REGION,
    stop: () => {
      spawnSync('docker', ['rm', '-f', container], { stdio: 'ignore' })
    },
  }
  try {
    const deadline = Date.now() + 60_000
    for (;;) {
      const ready = await signedRequest(server, 'GET', '/').then(
        (r) => r.ok,
        () => false,
      )
      if (ready) break
      if (Date.now() > deadline) throw new Error('RustFS did not answer')
      await new Promise((resolve) => setTimeout(resolve, 500))
    }
    for (const bucket of buckets) {
      const made = await signedRequest(server, 'PUT', `/${bucket}`)
      if (!made.ok) throw new Error(`bucket ${bucket}: ${made.status}`)
    }
  } catch (error) {
    server.stop()
    throw error
  }
  return server
}

/** The keys in `bucket` (one page is enough for a scenario). */
export async function listKeys(
  server: Rustfs,
  bucket: string,
): Promise<string[]> {
  const answer = await signedRequest(server, 'GET', `/${bucket}`)
  const text = await answer.text()
  return [...text.matchAll(/<Key>([^<]*)<\/Key>/g)].map((m) => m[1] ?? '')
}

/** Stores `body` at `key` in `bucket`, as another program would (spec 044, `files-storage`). */
export async function putObject(
  server: Rustfs,
  bucket: string,
  key: string,
  body: Uint8Array | string,
): Promise<void> {
  const bytes = typeof body === 'string' ? new TextEncoder().encode(body) : body
  const answer = await signedRequest(server, 'PUT', `/${bucket}/${key}`, bytes)
  if (!answer.ok) throw new Error(`PUT ${key}: ${answer.status}`)
}

/** The keys in `bucket` with their sizes (one page is enough for a scenario). */
export async function listObjects(
  server: Rustfs,
  bucket: string,
): Promise<{ key: string; size: number }[]> {
  const answer = await signedRequest(server, 'GET', `/${bucket}`)
  const text = await answer.text()
  return [...text.matchAll(/<Contents>([\s\S]*?)<\/Contents>/g)].map((m) => ({
    key: /<Key>([^<]*)<\/Key>/.exec(m[1] ?? '')?.[1] ?? '',
    size: Number(/<Size>(\d+)<\/Size>/.exec(m[1] ?? '')?.[1] ?? '0'),
  }))
}

/** The bytes at `key` in `bucket`, as another program would read them (spec 048). */
export async function getObject(
  server: Rustfs,
  bucket: string,
  key: string,
): Promise<Uint8Array> {
  const answer = await signedRequest(server, 'GET', `/${bucket}/${key}`)
  if (!answer.ok) throw new Error(`GET ${key}: ${answer.status}`)
  return new Uint8Array(await answer.arrayBuffer())
}
