import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { createServer } from 'node:http'
import type { AddressInfo } from 'node:net'
import type { InvokeResult } from '../lib/platform/host.ts'
import { reachFromDevice } from '../lib/platform/reach.ts'
import { scenario } from '../lib/scenario.ts'
import { createAndUnlock, unwrap } from '../lib/flows.ts'

// Spec 096 US1 and US2: Abacus AI is installed into the vault from a server on this device (a
// stand-in catalog and a script for the program) and connected through the device sign-in against
// a stand-in of the sign-in service (`providers/abacus_stand_in.rs`, debug builds only). The
// connect asks for no key and no terminal: the e-mail address starts it, the service confirms
// the code, and the device code lands as a credential entry of the password manager, not in the
// provider row. Disconnecting removes the row and the entry. Sending a request through the stub
// program is the integration test `cli_delegate_abacus_run` (this suite has no delegate chat).
// Written for the manual dispatch of the suite; it was not run locally.

const VERSION = '9.9.9'
const PROGRAM = `#!/bin/sh\necho "Abacus.AI CLI version ${VERSION}"\n`
const EMAIL = 'someone@example.invalid'
const USER_CODE = 'ABCD-1234'
const DEVICE_CODE = 'placeholder-device-code-of-the-e2e-scenario'

/** The command answered and refused. */
function refused(result: InvokeResult): boolean {
  return 'ok' in result && !result.ok
}

interface Provider {
  id: string
  kind: string
  adapter: string | null
  credentials?: unknown
}

scenario('abacus-connect', { timeoutMs: 240_000 }, async (ctx) => {
  const instance = await ctx.startInstance()
  await createAndUnlock(instance, { name: 'abacus-vault' })

  const calls: string[] = []
  const bodies: string[] = []
  const server = createServer((request, response) => {
    const url = request.url ?? ''
    calls.push(`${request.method} ${url}`)
    if (request.method === 'GET' && url === '/abacusai') {
      response.writeHead(200, { 'content-type': 'application/octet-stream' })
      response.end(PROGRAM)
      return
    }
    let body = ''
    request.on('data', (chunk) => (body += String(chunk)))
    request.on('end', () => {
      bodies.push(body)
      response.writeHead(200, { 'content-type': 'application/json' })
      if (url === '/api/_authenticateDeviceForCodeLlm') {
        response.end(
          JSON.stringify({
            success: true,
            result: {
              verificationUri: 'https://example.invalid/device',
              userCode: USER_CODE,
              pollingInterval: 1,
            },
          }),
        )
      } else if (url === '/api/_pollDeviceCode') {
        response.end(
          JSON.stringify({
            success: true,
            result: {
              deviceCode: DEVICE_CODE,
              chatWebUrl: 'https://example.invalid/chat',
            },
          }),
        )
      } else {
        response.end(JSON.stringify({ success: false }))
      }
    })
  })
  await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve))
  const { port } = server.address() as AddressInfo
  reachFromDevice(port)
  try {
    const artifact = {
      url: `http://127.0.0.1:${port}/abacusai`,
      sha256: createHash('sha256').update(PROGRAM).digest('hex'),
      size: PROGRAM.length,
      kind: 'file',
    }
    const catalog = {
      schema: 1,
      vendors: [
        {
          vendor: 'abacusai',
          display_name: 'Abacus AI',
          download_hosts: ['static.abacus.ai'],
          executable: {
            'linux-x64': 'abacusai',
            'linux-x64-baseline': 'abacusai',
            'linux-arm64': 'abacusai',
            'darwin-x64': 'abacusai',
            'darwin-arm64': 'abacusai',
            'win32-x64': 'abacusai.exe',
            'win32-arm64': 'abacusai.exe',
          },
          env_set: [{ name: 'ABACUS_AUTO_UPDATE', value: 'off' }],
          env_remove_prefixes: ['ABACUS_', 'ABACUSAI_', 'CODELLM_'],
          config_env: 'ABACUSAI_CODING_AGENT_DIR',
          versions: [
            {
              version: VERSION,
              platforms: {
                'linux-x64': artifact,
                'linux-x64-baseline': artifact,
                'linux-arm64': artifact,
                'darwin-x64': artifact,
                'darwin-arm64': artifact,
              },
            },
          ],
        },
      ],
    }
    unwrap(
      'e2e_stand_in_cli_tools',
      await instance.invoke('e2e_stand_in_cli_tools', {
        catalogJson: JSON.stringify(catalog),
      }),
    )
    unwrap(
      'e2e_stand_in_abacus_signin',
      await instance.invoke('e2e_stand_in_abacus_signin', {
        baseUrl: `http://127.0.0.1:${port}`,
      }),
    )

    // Connecting needs the tool in this vault first: the flow ends before any request goes out.
    const early = await instance.invoke('connect_cli_delegate', {
      args: { vendor: 'abacusai', name: 'Abacus AI', email: EMAIL },
    })
    assert.ok(refused(early), 'a vault without the tool does not connect')
    assert.deepEqual(
      calls.filter((call) => call.startsWith('POST')),
      [],
      'no sign-in request before the tool is installed',
    )
    ctx.step('connecting waits for the installed tool and sends nothing before')

    unwrap(
      'cli_tools_install',
      await instance.invoke('cli_tools_install', { vendor: 'abacusai' }),
    )

    const missingEmail = await instance.invoke('connect_cli_delegate', {
      args: { vendor: 'abacusai', name: 'Abacus AI' },
    })
    assert.ok(refused(missingEmail), 'the e-mail address is required')

    const connected = unwrap(
      'connect_cli_delegate',
      await instance.invoke('connect_cli_delegate', {
        args: { vendor: 'abacusai', name: 'Abacus AI', email: EMAIL },
      }),
    ) as { status: string }
    assert.equal(connected.status, 'connected')
    const start = bodies.find((body) => body.includes('email'))
    assert.ok(start, 'the sign-in sent the e-mail address')
    assert.equal(JSON.parse(start).email, EMAIL)
    ctx.step('the device sign-in connects without a key and without a terminal')

    const providers = unwrap(
      'list_providers',
      await instance.invoke('list_providers', {}),
    ) as Provider[]
    const rows = providers.filter(
      (row) => row.kind === 'cli_delegate' && row.adapter === 'abacusai',
    )
    assert.equal(rows.length, 1, 'exactly one Abacus row')
    assert.ok(
      !JSON.stringify(rows[0]).includes(DEVICE_CODE),
      'the provider row holds no secret',
    )
    ctx.step('one provider row, and the device code is not in it')

    // Disconnecting is deleting the row; the credential entry goes with the last row.
    unwrap(
      'delete_provider',
      await instance.invoke('delete_provider', { id: rows[0]!.id }),
    )
    const after = unwrap(
      'list_providers',
      await instance.invoke('list_providers', {}),
    ) as Provider[]
    assert.equal(
      after.filter((row) => row.adapter === 'abacusai').length,
      0,
      'the row is gone',
    )
    ctx.step('disconnecting removes the row')
  } finally {
    await new Promise<void>((resolve) => server.close(() => resolve()))
  }
})
