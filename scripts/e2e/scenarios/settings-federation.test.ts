import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { createAndUnlock } from '../lib/flows.ts'
import {
  KEY,
  clearAlias,
  openSettings,
  runAction,
  waitForLocation,
} from '../lib/settings.ts'

type Row = { title: string; current: boolean; mark: boolean }
const rows = (instance: { exec<T>(s: string): Promise<T> }) =>
  instance.exec<Row[]>(
    `return [...document.querySelectorAll('[data-testid="settings-device"]')].map((row) => ({
       title: row.querySelector('span, label').textContent.trim(),
       current: row.dataset.current === 'true',
       mark: row.querySelector('[data-testid="settings-device-current"]') !== null,
     }))`,
  )

// Spec 023-settings-app, quickstart S13a without the second installation (FR-022): the category
// "Föderation" lists this device once, first and marked; after a rename in "Allgemein" it shows the new
// name. The same list is readable as the action `settings.devices.list`. Ordering with other and
// unnamed devices is covered by the Rust tests (`sync/device_view_tests.rs`, `known_devices_tests.rs`).
scenario('settings-federation', {}, async (ctx) => {
  const instance = await ctx.startInstance()
  await createAndUnlock(instance, { name: 'e2e-settings-federation' })
  await openSettings(instance)
  await instance.click('settings-row-general.basic')
  await waitForLocation(instance, 'general.basic')

  const renamed = 'E2E Föderation'
  await clearAlias(instance)
  await instance.type('settings-alias', renamed)
  await instance.type('settings-alias', KEY.enter)
  await ctx.waitFor('the name to be saved', () =>
    instance.exec<boolean>(
      'return document.querySelector(\'ul:has([data-testid="settings-alias"]) [role="status"]\') !== null',
    ),
  )

  await instance.click('settings-category-federation')
  await waitForLocation(instance, 'federation')
  await instance.waitForDisplayed('settings-device')
  const listed = await rows(instance)
  assert.deepEqual(listed, [{ title: renamed, current: true, mark: true }])
  ctx.step('this device, marked, with its new name')

  // Spec 059 FR-002: this device's row shows its three version values, none of them marked, and no
  // "not known yet" hint (that one is for other devices).
  const schemaText = await instance.exec<string>(
    `return document.querySelector('[data-testid="settings-device"][data-current] [data-testid="settings-device-schema"]')?.textContent ?? ''`,
  )
  assert.match(schemaText, /\d+\D+\d+\D+\d+/, schemaText)
  const marked = await instance.exec<number>(
    `return document.querySelectorAll('[data-testid="settings-device-schema"] [data-order="lower"], [data-testid="settings-device-schema"] [data-order="higher"], [data-testid="settings-device-schema-unknown"]').length`,
  )
  assert.equal(marked, 0)
  ctx.step('this device shows its version state, nothing marked')

  const outcome = await runAction(instance, 'settings.devices.list')
  assert.ok(outcome.ok, `settings.devices.list: ${JSON.stringify(outcome)}`)
  const devices = (
    outcome.result as {
      devices: Array<{
        alias?: string
        isCurrent: boolean
        schema?: {
          protocol: number
          holziMigration: number
          crdtTrigger: number
        }
      }>
    }
  ).devices
  assert.equal(devices.length, 1)
  assert.equal(devices[0]?.alias, renamed)
  assert.equal(devices[0]?.isCurrent, true)
  // Spec 059 FR-011: the action carries the version state too.
  assert.equal(typeof devices[0]?.schema?.holziMigration, 'number')
  ctx.step('settings.devices.list')
  // FR-008: the built-in servers are always listed and can be switched off, not removed; added ones
  // can be switched off or removed.
  const serverRows = (kind: string, part: string, enabled?: boolean) =>
    instance.exec<number>(
      `return document.querySelectorAll('[data-testid="settings-servers-${kind}-${part}"]${
        enabled === undefined ? '' : `[data-enabled="${enabled}"]`
      }').length`,
    )
  await instance.waitForDisplayed('settings-servers-nostrRelays-input')
  assert.equal(await serverRows('nostrRelays', 'default', true), 3)
  assert.equal(await serverRows('irohRelays', 'default', true), 4)
  assert.equal(await serverRows('nostrRelays', 'remove'), 0)
  ctx.step(
    'the built-in Nostr and iroh servers are listed, all in use, none deletable',
  )

  await instance.click('settings-servers-nostrRelays-toggle')
  await ctx.waitFor('the first Nostr server to be switched off', async () => {
    return (await serverRows('nostrRelays', 'default', false)) === 1
  })
  assert.equal(await serverRows('nostrRelays', 'default'), 3)
  ctx.step('a built-in server is switched off and stays listed')

  await instance.type(
    'settings-servers-irohRelays-input',
    'https://iroh.example.org',
  )
  await instance.click('settings-servers-irohRelays-add')
  await ctx.waitFor('the added iroh server to be listed', async () => {
    return (await serverRows('irohRelays', 'added', true)) === 1
  })
  assert.equal(await serverRows('irohRelays', 'default'), 4)
  ctx.step('an added iroh server is listed after the built-in ones')

  await instance.click('settings-servers-irohRelays-remove')
  await ctx.waitFor('the added iroh server to be deleted', async () => {
    return (await serverRows('irohRelays', 'added')) === 0
  })
  assert.equal(await serverRows('irohRelays', 'default'), 4)
  ctx.step('deleting it leaves the built-in ones')
})
