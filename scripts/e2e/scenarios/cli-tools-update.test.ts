import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { createServer } from 'node:http'
import type { AddressInfo } from 'node:net'
import { scenario } from '../lib/scenario.ts'
import { createAndUnlock, unwrap } from '../lib/flows.ts'
import { runAction } from '../lib/settings.ts'

// Spec 094 US3 (quickstart §6): an update is a click, never silent. With a newer version listed the
// row offers the update; the update installs next to the version in use and the row offers to go
// back to it; going back removes the new version; updating again and a finished run with the new
// version (the debug-only `e2e_confirm_cli_tool` stands in for a request that ends normally, as no
// model account exists here) makes the new version the only one and takes the way back away.
// The programs are scripts that name their version, served from this device.

const OLD = '1.0.0'
const NEW = '1.1.0'
const programOf = (version: string) => `#!/bin/sh\necho "claude ${version}"\n`

interface ToolStatus {
  vendor: string
  state: string
  installedVersion: string | null
  previousVersion: string | null
  confirmed: boolean
}

scenario('cli-tools-update', { timeoutMs: 300_000 }, async (ctx) => {
  const instance = await ctx.startInstance()
  await createAndUnlock(instance, { name: 'cli-tools-update-vault' })

  const requests: string[] = []
  const server = createServer((request, response) => {
    requests.push(request.url ?? '')
    const version = /\/claude-([0-9.]+)$/.exec(request.url ?? '')?.[1]
    if (!version) {
      response.writeHead(404).end()
      return
    }
    response.writeHead(200, { 'content-type': 'application/octet-stream' })
    response.end(programOf(version))
  })
  await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve))
  const { port } = server.address() as AddressInfo
  try {
    // The catalog lists `versions`, newest first.
    const listVersions = async (versions: string[]) => {
      const platforms = (version: string) => {
        const program = programOf(version)
        const artifact = {
          url: `http://127.0.0.1:${port}/claude-${version}`,
          sha256: createHash('sha256').update(program).digest('hex'),
          size: program.length,
          kind: 'file',
        }
        return {
          'linux-x64': artifact,
          'linux-arm64': artifact,
          'darwin-x64': artifact,
          'darwin-arm64': artifact,
        }
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
            versions: versions.map((version) => ({
              version,
              platforms: platforms(version),
            })),
          },
        ],
      }
      unwrap(
        'e2e_stand_in_cli_tools',
        await instance.invoke('e2e_stand_in_cli_tools', {
          catalogJson: JSON.stringify(catalog),
        }),
      )
    }

    const status = async (): Promise<ToolStatus> => {
      const all = unwrap(
        'cli_tools_status',
        await instance.invoke('cli_tools_status', {}),
      ) as ToolStatus[]
      const claude = all.find((tool) => tool.vendor === 'claude')
      assert.ok(claude, 'the catalog lists Claude Code')
      return claude
    }

    const displayed = (testId: string) =>
      instance.exec<boolean>(
        `return document.querySelector('[data-testid="${testId}"]') !== null`,
        [],
      )

    // The row shows its actions as buttons from a container width of 48 rem and in a menu below
    // it, as the window of the suite is narrow: click the button when it is laid out, else open
    // the menu and take the item.
    const rowAction = async (action: 'update' | 'revert') => {
      const button = `cli-tool-${action}-claude`
      const laidOut = await instance.exec<boolean>(
        `const el = document.querySelector('[data-testid="${button}"]'); return el !== null && el.getClientRects().length > 0`,
        [],
      )
      if (laidOut) {
        await instance.click(button)
        return
      }
      await instance.click('cli-tool-actions-claude')
      await instance.waitForDisplayed(`cli-tool-${action}-menu-claude`)
      await instance.click(`cli-tool-${action}-menu-claude`)
    }

    const openProviders = async () => {
      const opened = await runAction(instance, 'wm.app.open', {
        appId: 'system.settings',
        at: '/ai/providers',
      })
      assert.ok(opened.ok, JSON.stringify(opened))
      await instance.waitForDisplayed('settings-row-cli-tool-claude')
    }

    // The page reads the tools when it opens, so a change of the stand-in catalog is seen by
    // leaving the page and coming back.
    const reopenProviders = async () => {
      const left = await runAction(instance, 'wm.app.open', {
        appId: 'system.settings',
        at: '/ai/access',
      })
      assert.ok(left.ok, JSON.stringify(left))
      await ctx.waitFor(
        'the providers page is left',
        async () => !(await displayed('settings-row-cli-tool-claude')),
      )
      await openProviders()
    }

    // 1. The first install of the old version.
    await listVersions([OLD])
    await openProviders()
    await instance.click('cli-tool-install-claude')
    await ctx.waitFor(
      'the old version is installed',
      async () => (await status()).installedVersion === OLD,
    )
    assert.equal((await status()).state, 'installed')
    ctx.step('the first install puts the old version in use')

    // 2. A newer version is listed: the row offers the update, and nothing is downloaded by itself.
    await listVersions([NEW, OLD])
    await reopenProviders()
    await ctx.waitFor('the update is offered', () =>
      displayed('cli-tool-update-claude'),
    )
    assert.equal((await status()).state, 'update_available')
    assert.deepEqual(
      requests,
      [`/claude-${OLD}`],
      'listing a version downloads nothing',
    )
    assert.equal(await displayed('cli-tool-revert-claude'), false)
    ctx.step('a newer listed version is offered, never installed by itself')

    // 3. The update installs next to the version in use and offers the way back.
    await rowAction('update')
    await ctx.waitFor('the way back is offered', () =>
      displayed('cli-tool-revert-claude'),
    )
    const updated = await status()
    assert.equal(updated.installedVersion, NEW)
    assert.equal(updated.previousVersion, OLD)
    assert.equal(updated.confirmed, false)
    assert.equal(
      await displayed('cli-tool-update-claude'),
      false,
      'no second update while the new version has not finished a run',
    )
    const revertLabel = await instance.exec<string>(
      `return document.querySelector('[data-testid="cli-tool-revert-claude"]')?.textContent ?? ''`,
      [],
    )
    assert.ok(
      revertLabel.includes(OLD),
      `the way back names the version: ${revertLabel}`,
    )
    ctx.step('the update keeps the old version and offers to go back to it')

    // 4. Going back removes the new version; the update is offered again.
    await rowAction('revert')
    await ctx.waitFor('the update is offered', () =>
      displayed('cli-tool-update-claude'),
    )
    const reverted = await status()
    assert.equal(reverted.installedVersion, OLD)
    assert.equal(reverted.previousVersion, null)
    assert.equal(await displayed('cli-tool-revert-claude'), false)
    ctx.step('going back leaves the old version in use')

    // 5. Update again, and let a run end normally with the new version.
    await rowAction('update')
    await ctx.waitFor('the way back is offered', () =>
      displayed('cli-tool-revert-claude'),
    )
    assert.equal((await status()).installedVersion, NEW)
    unwrap(
      'e2e_confirm_cli_tool',
      await instance.invoke('e2e_confirm_cli_tool', { vendor: 'claude' }),
    )
    await ctx.waitFor(
      'the row drops the way back once the new version has finished a run',
      async () => !(await displayed('cli-tool-revert-claude')),
    )
    const confirmed = await status()
    assert.equal(confirmed.installedVersion, NEW)
    assert.equal(confirmed.previousVersion, null)
    assert.equal(confirmed.confirmed, true)
    assert.equal(
      confirmed.state,
      'installed',
      'the newest listed version is in use',
    )
    assert.equal(await displayed('cli-tool-update-claude'), false)
    assert.deepEqual(
      requests,
      [`/claude-${OLD}`, `/claude-${NEW}`, `/claude-${NEW}`],
      'one download per click, none on its own',
    )
    ctx.step('a finished run confirms the new version and the way back is gone')
  } finally {
    await new Promise<void>((resolve) => server.close(() => resolve()))
  }
})
