import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { openSettings } from '../lib/settings.ts'
import { unwrap } from '../lib/flows.ts'
import { onDevice } from '../lib/platform/device-files.ts'
import {
  fixture,
  inFrame,
  openFromLauncher,
  type InstalledExtension,
} from '../lib/extensions.ts'

interface DeclaredRow {
  kind: string
  action: string
  target: string
  titleDe?: string
  supported?: boolean
}

// Spec 018, US2 (T032, quickstart step 1): the install preview the dialog shows lists the test
// extension's tools with their effect, a tool outside holzi's schema subset marked; the confirmed
// tools are stored as `agentTool` permissions; and the extension's code, the released vault-sdk
// bundled in, starts in holzi's frame. The agent's call is covered by
// `src-tauri/tests/extension_agent_tools.rs`, since this harness runs no model. The dialog itself
// picks its file with the system's file dialog, which the harness cannot drive.
scenario('extension-agent-tools', { timeoutMs: 300_000 }, async (ctx) => {
  const group = await ctx.group({ users: { anna: ['laptop'] } })
  const page = group.device('anna/laptop').page
  const file = onDevice(fixture('e2e', 'agent-tools'), page)

  const preview = unwrap<{
    signatureValid: boolean
    displayName?: string
    declared: DeclaredRow[]
  }>(
    'extension_install_preview',
    await page.invoke('extension_install_preview', {
      source: { kind: 'file', file },
    }),
  )
  assert.ok(preview.signatureValid)
  assert.equal(preview.displayName, 'Test-Erweiterung')
  const tools = preview.declared
    .filter((row) => row.kind === 'agentTool')
    .map((row) => [row.target, row.action, row.titleDe, row.supported])
  assert.deepEqual(tools, [
    ['count_entries', 'read', 'Einträge zählen', true],
    ['add_entry', 'change', 'Eintrag hinzufügen', true],
    ['slow_echo', 'read', 'Langsames Echo', true],
    ['optional_note', 'read', 'Optionale Notiz', false],
  ])
  ctx.step('the preview lists the tools with their effect')

  const ticked = ['count_entries', 'add_entry', 'slow_echo']
  const accepted = preview.declared.map((row) => ({
    kind: row.kind,
    action: row.action,
    target: row.target,
    granted:
      row.kind === 'database' ||
      (row.kind === 'agentTool' && ticked.includes(row.target)),
  }))
  const extension = unwrap<InstalledExtension>(
    'extension_install',
    await page.invoke('extension_install', {
      args: { source: { kind: 'file', file }, accepted, confirmed: false },
    }),
  )
  const views = unwrap<Array<{ kind: string; target: string; status: string }>>(
    'extension_permissions_list',
    await page.invoke('extension_permissions_list', {
      extensionId: extension.id,
    }),
  )
  const statuses = Object.fromEntries(
    views
      .filter((view) => view.kind === 'agentTool')
      .map((view) => [view.target, view.status]),
  )
  assert.deepEqual(statuses, {
    count_entries: 'granted',
    add_entry: 'granted',
    slow_echo: 'granted',
    optional_note: 'ask',
  })
  ctx.step('the confirmed tools are stored as permissions')

  await openFromLauncher(page, extension)
  await ctx.waitFor(
    'the bundled vault-sdk to count the entries in the frame',
    async () =>
      (await inFrame<string>(
        page,
        extension,
        `return document.getElementById('count').textContent`,
      )) === '0',
    { timeoutMs: 20_000 },
  )
  ctx.step('the extension starts with the released vault-sdk')

  // Quickstart step 7: the switch takes every tool from the agent and gives them back.
  const switchedOff = async () =>
    unwrap<Array<{ kind: string; target: string; status: string }>>(
      'extension_permissions_list',
      await page.invoke('extension_permissions_list', {
        extensionId: extension.id,
      }),
    ).some(
      (view) =>
        view.kind === 'agentTool' &&
        view.target === '*' &&
        view.status === 'denied',
    )
  await openSettings(page)
  await page.click('settings-category-extensions')
  await page.click(
    `[data-testid="extension-row"][data-extension-id="${extension.id}"]`,
  )
  await page.click('extension-agent-tools-available')
  await ctx.waitFor('the tools to be switched off', switchedOff, {
    timeoutMs: 10_000,
  })
  await page.click('extension-agent-tools-available')
  await ctx.waitFor(
    'the tools to be switched on again',
    async () => !(await switchedOff()),
    { timeoutMs: 10_000 },
  )
  ctx.step('the switch takes the tools from the agent and gives them back')
})
