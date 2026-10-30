import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { readFileSync } from 'node:fs'
import { after, describe, it } from 'node:test'
import { fixture } from './extensions.ts'
import {
  ADDRESS_VARIABLE,
  pointPresetAt,
  startMarketplace,
  type TestMarketplace,
} from './marketplace.ts'

const open: TestMarketplace[] = []
after(async () => {
  for (const market of open) await market.close()
})

async function start(
  options?: Parameters<typeof startMarketplace>[0],
): Promise<TestMarketplace> {
  const market = await startMarketplace(options)
  open.push(market)
  return market
}

const get = async (url: string, headers: Record<string, string> = {}) => {
  const response = await fetch(url, { headers })
  return { status: response.status, response }
}

describe('the test marketplace', () => {
  it('lists the extensions by downloads, twenty to a page, with real checksums', async () => {
    const market = await start()
    const first = (await (await fetch(`${market.url}/extensions`)).json()) as {
      extensions: Array<{
        slug: string
        versions: Array<{ bundleSize: number; bundleHash: string }>
      }>
      pagination: { total: number; totalPages: number }
    }
    assert.equal(first.extensions.length, 20)
    assert.deepEqual(
      first.extensions.slice(0, 2).map((entry) => entry.slug),
      ['probe', 'agent-tools'],
    )
    assert.equal(first.pagination.totalPages, 2)
    const bytes = readFileSync(fixture('e2e', 'probe'))
    const [version] = first.extensions[0]!.versions
    assert.equal(version!.bundleSize, bytes.length)
    assert.equal(
      version!.bundleHash,
      createHash('sha256').update(bytes).digest('hex'),
    )
    const second = (await (
      await fetch(`${market.url}/extensions?page=2`)
    ).json()) as { extensions: unknown[] }
    assert.equal(second.extensions.length, 6)
  })

  it('filters by search and category', async () => {
    const market = await start()
    const found = (await (
      await fetch(`${market.url}/extensions?search=AGENT`)
    ).json()) as { extensions: Array<{ slug: string }> }
    assert.deepEqual(
      found.extensions.map((entry) => entry.slug),
      ['agent-tools'],
    )
    const games = (await (
      await fetch(`${market.url}/extensions?category=games`)
    ).json()) as { extensions: Array<{ category: { slug: string } }> }
    assert.ok(games.extensions.length > 0)
    assert.ok(
      games.extensions.every((entry) => entry.category.slug === 'games'),
    )
    const empty = (await (
      await fetch(`${market.url}/extensions?search=no-such-extension`)
    ).json()) as { extensions: unknown[]; pagination: { totalPages: number } }
    assert.deepEqual(empty.extensions, [])
    assert.equal(empty.pagination.totalPages, 0)
  })

  it('answers the download call with an address that serves the bytes', async () => {
    const market = await start()
    const answer = (await (
      await fetch(`${market.url}/extensions/probe/download?version=1.0.0`)
    ).json()) as { downloadUrl: string; bundleSize: number; bundleHash: string }
    const body = Buffer.from(
      await (await fetch(answer.downloadUrl)).arrayBuffer(),
    )
    assert.equal(body.length, answer.bundleSize)
    assert.equal(
      createHash('sha256').update(body).digest('hex'),
      answer.bundleHash,
    )
  })

  it('has switches for a wrong, a missing hash and a wrong size', async () => {
    const market = await start()
    const call = async () =>
      (await (
        await fetch(`${market.url}/extensions/probe/download?version=1.0.0`)
      ).json()) as { bundleSize: number; bundleHash?: string }
    const honest = await call()

    market.hash = 'wrong'
    const wrong = await call()
    assert.notEqual(wrong.bundleHash, honest.bundleHash)
    assert.equal(wrong.bundleHash?.length, 64)

    market.hash = 'missing'
    assert.equal('bundleHash' in (await call()), false)
    const detail = (await (
      await fetch(`${market.url}/extensions/probe`)
    ).json()) as { versions: Array<Record<string, unknown>> }
    assert.equal('bundleHash' in detail.versions[0]!, false)

    market.hash = 'ok'
    market.size = 'wrong'
    assert.equal((await call()).bundleSize, honest.bundleSize + 1)
  })

  it('serves downloads behind a path prefix and 503s while down', async () => {
    const market = await start({ prefix: '/market' })
    assert.equal(market.url, `${market.origin}/market`)
    assert.equal((await get(`${market.url}/health`)).status, 200)
    assert.equal((await get(`${market.origin}/health`)).status, 503)
    const answer = (await (
      await fetch(`${market.url}/extensions/probe/download?version=1.0.0`)
    ).json()) as { downloadUrl: string }
    assert.equal(answer.downloadUrl, `${market.url}/files/probe/1.0.0.xt`)
    assert.equal((await get(answer.downloadUrl)).status, 200)
    market.down = true
    assert.equal((await get(`${market.url}/health`)).status, 503)
  })

  it('requires the bearer token for the catalog but not for the files', async () => {
    const market = await start({ token: true })
    assert.ok(market.token)
    assert.equal((await get(`${market.url}/extensions`)).status, 401)
    assert.equal(
      (
        await get(`${market.url}/extensions`, {
          authorization: `Bearer ${market.token}`,
        })
      ).status,
      200,
    )
    assert.equal((await get(`${market.url}/files/probe/1.0.0.xt`)).status, 200)
  })

  it('records every request with its Authorization header', async () => {
    const market = await start({ token: true })
    await get(`${market.url}/categories`, {
      authorization: `Bearer ${market.token}`,
    })
    await get(`${market.url}/files/probe/1.0.0.xt`)
    assert.deepEqual(
      market.requests.map((request) => [request.path, request.authorization]),
      [
        ['/categories', `Bearer ${market.token}`],
        ['/files/probe/1.0.0.xt', undefined],
      ],
    )
    assert.equal(market.hits('/categories'), 1)
  })
})

describe('pointing the preset', () => {
  it('sets the variable and puts the previous value back', () => {
    const before = process.env[ADDRESS_VARIABLE]
    const restore = pointPresetAt('http://127.0.0.1:1')
    assert.equal(process.env[ADDRESS_VARIABLE], 'http://127.0.0.1:1')
    restore()
    assert.equal(process.env[ADDRESS_VARIABLE], before)
  })
})
