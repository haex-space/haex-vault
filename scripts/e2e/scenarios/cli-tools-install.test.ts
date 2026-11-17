import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { createServer } from 'node:http'
import type { AddressInfo } from 'node:net'
import { scenario } from '../lib/scenario.ts'
import { createAndUnlock, unwrap } from '../lib/flows.ts'
import { runAction, waitForLocation } from '../lib/settings.ts'

// Spec 094 US1 (quickstart §1): a vault without Claude Code shows the install with its size; the
// install downloads the program from a server on this device (a stand-in catalog and a script for
// the program, `cli_tools/stand_in.rs`, debug builds only), checks it, starts it once for its
// version and publishes it; the row then shows the version and offers no install. The connect
// button waits for the program: it is disabled until the tool is installed.

const VERSION = '9.9.9'
const PROGRAM = `#!/bin/sh\necho "claude ${VERSION}"\n`

interface ToolStatus {
  vendor: string
  state: string
  installedVersion: string | null
}

scenario('cli-tools-install', { timeoutMs: 240_000 }, async (ctx) => {
  const instance = await ctx.startInstance()
  await createAndUnlock(instance, { name: 'cli-tools-vault' })

  const requests: string[] = []
  const server = createServer((request, response) => {
    requests.push(request.url ?? '')
    response.writeHead(200, { 'content-type': 'application/octet-stream' })
    response.end(PROGRAM)
  })
  await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve))
  const { port } = server.address() as AddressInfo
  try {
    const sha256 = createHash('sha256').update(PROGRAM).digest('hex')
    const artifact = {
      url: `http://127.0.0.1:${port}/claude`,
      sha256,
      size: PROGRAM.length,
      kind: 'file',
    }
    const catalog = {
      schema: 1,
      vendors: [
        {
          vendor: 'claude',
          display_name: 'Claude Code',
          download_hosts: ['downloads.claude.ai'],
          executable: {
            'linux-x64': 'claude',
            'linux-arm64': 'claude',
            'darwin-x64': 'claude',
            'darwin-arm64': 'claude',
            'win32-x64': 'claude.exe',
            'win32-arm64': 'claude.exe',
          },
          env_set: [{ name: 'DISABLE_UPDATES', value: '1' }],
          config_env: 'CLAUDE_CONFIG_DIR',
          versions: [
            {
              version: VERSION,
              platforms: {
                'linux-x64': artifact,
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

    const status = async (): Promise<ToolStatus> => {
      const all = unwrap(
        'cli_tools_status',
        await instance.invoke('cli_tools_status', {}),
      ) as ToolStatus[]
      const claude = all.find((tool) => tool.vendor === 'claude')
      assert.ok(claude, 'the catalog lists Claude Code')
      return claude
    }

    assert.equal((await status()).state, 'not_installed')
    const opened = await runAction(instance, 'wm.app.open', {
      appId: 'system.settings',
      at: '/ai/providers',
    })
    assert.ok(opened.ok, JSON.stringify(opened))
    await waitForLocation(instance, 'ai.providers')
    await instance.waitForDisplayed('cli-tool-install-claude')
    assert.deepEqual(requests, [], 'nothing is downloaded before the click')
    ctx.step('the tool is offered, and nothing is downloaded before the click')

    await instance.click('cli-tool-install-claude')
    await ctx.waitFor(
      'the settings row shows the installed version and no install action',
      async () =>
        (await instance.exec<boolean>(
          `const row = document.querySelector('[data-testid="settings-row-cli-tool-claude"]')?.textContent ?? ''
           return row.includes('${VERSION}') && document.querySelector('[data-testid="cli-tool-install-claude"]') === null`,
          [],
        )) === true,
    )
    assert.equal((await status()).installedVersion, VERSION)
    assert.deepEqual(requests, ['/claude'], 'one download')
    const row = await instance.exec<string>(
      `return document.querySelector('[data-testid="settings-row-cli-tool-claude"]')?.textContent ?? ''`,
      [],
    )
    assert.ok(row.includes(VERSION), `the row names the version: ${row}`)
    const stillOffered = await instance.exec<boolean>(
      `return document.querySelector('[data-testid="cli-tool-install-claude"]') !== null`,
      [],
    )
    assert.equal(stillOffered, false, 'an installed tool offers no install')
    ctx.step('the install publishes the tool and the row shows its version')
  } finally {
    await new Promise<void>((resolve) => server.close(() => resolve()))
  }
})
