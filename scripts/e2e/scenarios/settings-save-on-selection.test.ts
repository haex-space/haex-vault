import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { createAndUnlock } from '../lib/flows.ts'
import type { InvokeResult } from '../lib/platform/host.ts'
import {
  KEY,
  clearAlias,
  openSettings,
  waitForLocation,
} from '../lib/settings.ts'

function data<T>(result: InvokeResult): T {
  if (!('ok' in result) || !result.ok) {
    throw new Error(`backend call failed: ${JSON.stringify(result)}`)
  }
  return result.data as T
}

// Spec 023-settings-app, quickstart S17 and S21 (FR-021, FR-024): every setting saves on selection,
// without a button — the device name on Enter or when the field is left (an empty name is not saved and
// the field says why), deny rules, autonomy mode and session restore at once. What applies to the
// vault is stored for the vault, not for this device.
scenario('settings-save-on-selection', {}, async (ctx) => {
  const instance = await ctx.startInstance()
  await createAndUnlock(instance, { name: 'e2e-settings-save' })
  await openSettings(instance)
  await instance.click('settings-row-general.basic')
  await waitForLocation(instance, 'general.basic')
  const vaultPref = async (key: string) =>
    data<string | null>(
      await instance.invoke('get_pref', {
        args: { scope: { kind: 'vault' }, key },
      }),
    )
  const alias = async () =>
    data<{ alias: string | null }>(await instance.invoke('current_device_info'))
      .alias

  const buttons = await instance.exec<number>(
    'return document.querySelectorAll(\'[data-testid="settings-title"] ~ * button[type="submit"], form button[type="submit"]\').length',
  )
  assert.equal(buttons, 0, 'the settings show a save button')

  const renamed = 'E2E Zweitgerät'
  await clearAlias(instance)
  await instance.type('settings-alias', renamed)
  await instance.type('settings-alias', KEY.enter)
  await ctx.waitFor(
    'the name to be saved',
    async () => (await alias()) === renamed,
  )
  await ctx.waitFor('the saved notice at the name', () =>
    instance.exec<boolean>(
      'return document.querySelector(\'ul:has([data-testid="settings-alias"]) [role="status"]\') !== null',
    ),
  )
  await clearAlias(instance)
  await instance.type('settings-alias', KEY.enter)
  await instance.waitForDisplayed(
    '[data-testid="settings-alias"][aria-invalid="true"]',
  )
  assert.equal(await alias(), renamed, 'an empty name was saved')
  ctx.step('S17 name on Enter, empty name refused')

  await instance.click('session-restore-switch')
  await ctx.waitFor(
    'session restore on for the vault',
    async () => (await vaultPref('wm.session_restore')) !== null,
  )
  ctx.step('session restore saved for the vault')

  await instance.click('settings-category-ai')
  await instance.click('settings-row-ai.denyRules')
  await waitForLocation(instance, 'ai.denyRules')
  await instance.waitForDisplayed('settings-deny-network_access')
  const wasChecked = await instance.exec<boolean>(
    'return document.querySelector(\'[data-testid="settings-deny-network_access"]\').getAttribute("aria-checked") === "true"',
  )
  await instance.click('settings-deny-network_access')
  await instance.click('settings-category-ai')
  await waitForLocation(instance, 'ai')
  await ctx.waitFor('the deny rules to be saved for the vault', async () => {
    const raw = await vaultPref('cli_delegate.deny_rules')
    const rules = raw ? (JSON.parse(raw) as string[]) : []
    return rules.includes('network_access') !== wasChecked
  })
  ctx.step('S17 deny rules saved on change')

  await instance.click('settings-category-ai')
  await instance.click('settings-row-ai.autonomy')
  await waitForLocation(instance, 'ai.autonomy')
  await instance.click('settings-autonomy-gated_permissive')
  await ctx.waitFor('the autonomy mode to be saved for the vault', async () =>
    (await vaultPref('chat.autonomy_mode'))?.includes('gated_permissive'),
  )
  ctx.step('S17 autonomy mode saved on choice')

  const device = data<{ vaultDeviceUuid: string }>(
    await instance.invoke('current_device_info'),
  ).vaultDeviceUuid
  for (const key of [
    'chat.autonomy_mode',
    'cli_delegate.deny_rules',
    'wm.session_restore',
  ]) {
    const onDevice = data<string | null>(
      await instance.invoke('get_pref', {
        args: { scope: { kind: 'device', uuid: device }, key },
      }),
    )
    assert.equal(onDevice, null, `${key} was stored for this device`)
  }
  ctx.step('S21 vault settings are not stored per device')
})
