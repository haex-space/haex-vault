import assert from 'node:assert/strict'
import { createServer, type Server } from 'node:http'
import type { AddressInfo } from 'node:net'
import { scenario } from '../lib/scenario.ts'
import {
  fixture,
  install,
  openFromLauncher,
  probeRequest,
  reachableFromDevice,
} from '../lib/extensions.ts'

// Spec 017, US8, T097 (quickstart §8): network only through haex-vault. A request to a local server asks
// for the server's origin, runs once allowed and answers with status, headers, body and final
// address; a redirect to a second server asks again for that one. The frame's own network is
// covered by `extension-isolation`.

interface WebAnswer {
  status: number
  headers: Record<string, string>
  body: string
  url: string
}

async function listen(
  handler: Parameters<typeof createServer>[1],
): Promise<{ server: Server; origin: string }> {
  const server = createServer(handler)
  await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve))
  const { port } = server.address() as AddressInfo
  reachableFromDevice(port)
  return { server, origin: `http://127.0.0.1:${port}` }
}

scenario('extension-web', { timeoutMs: 240_000 }, async (ctx) => {
  const second = await listen((_req, res) => {
    res.writeHead(200, { 'content-type': 'text/plain' })
    res.end('from the second server')
  })
  const first = await listen((req, res) => {
    if (req.url === '/elsewhere') {
      res.writeHead(302, { location: `${second.origin}/there` })
      res.end()
      return
    }
    res.writeHead(200, { 'content-type': 'text/plain', 'x-probe': 'yes' })
    res.end('hello probe')
  })
  try {
    const group = await ctx.group({ users: { anna: ['laptop'] } })
    const page = group.device('anna/laptop').page
    const probe = await install(page, fixture('e2e', 'probe'))
    await openFromLauncher(page, probe)
    const fetch = (url: string) =>
      probeRequest(page, probe, 'extension_web_fetch', { url, method: 'GET' })

    const asked = await fetch(`${first.origin}/hello`)
    assert.equal(asked.error?.code, 1004, JSON.stringify(asked))
    assert.deepEqual(asked.error?.details, {
      resourceType: 'web',
      action: 'GET',
      target: `${first.origin}/*`,
    })
    await page.waitForDisplayed('extension-permission-request', 10_000)
    await page.click('extension-permission-allow')
    const answer = (await fetch(`${first.origin}/hello`)).result as WebAnswer
    assert.equal(answer.status, 200)
    assert.equal(answer.headers['x-probe'], 'yes')
    assert.equal(Buffer.from(answer.body, 'base64').toString(), 'hello probe')
    assert.equal(answer.url, `${first.origin}/hello`)
    ctx.step('a request to a local server asks once and then runs')

    const redirected = await fetch(`${first.origin}/elsewhere`)
    assert.equal(redirected.error?.code, 1004, JSON.stringify(redirected))
    assert.equal(
      (redirected.error?.details as { target?: string } | undefined)?.target,
      `${second.origin}/*`,
    )
    await page.waitForDisplayed('extension-permission-request', 10_000)
    await page.click('extension-permission-allow')
    const followed = (await fetch(`${first.origin}/elsewhere`))
      .result as WebAnswer
    assert.equal(followed.url, `${second.origin}/there`)
    assert.equal(
      Buffer.from(followed.body, 'base64').toString(),
      'from the second server',
    )
    ctx.step('a redirect to another server asks for that server')
  } finally {
    first.server.close()
    second.server.close()
  }
})
