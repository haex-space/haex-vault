import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { createAndUnlock } from '../lib/flows.ts'
import { choose, openSettings, waitForLocation } from '../lib/settings.ts'

const text = (instance: { exec<T>(s: string): Promise<T> }, selector: string) =>
  instance.exec<string>(
    `return document.querySelector(${JSON.stringify(selector)})?.textContent?.trim() ?? ''`,
  )

// Spec 042, quickstart step 2 (FR-007, FR-009, FR-010). The start page's picker switches at once; a
// new vault stores the language active when it is created; the choice in "Grundeinstellung"
// switches at once and is saved for the vault.
scenario('settings-language', {}, async (ctx) => {
  const instance = await ctx.startInstance()

  await choose(instance, 'landing-language', 'en')
  await ctx.waitFor(
    'the start page in English',
    async () => (await text(instance, 'h1')) === 'Welcome to HaexVault',
  )
  await choose(instance, 'landing-language', 'de')
  await ctx.waitFor(
    'the start page in German',
    async () => (await text(instance, 'h1')) === 'Willkommen bei HaexVault',
  )
  ctx.step('FR-007 start page picker')

  await createAndUnlock(instance, { name: 'e2e-settings-language' })
  const stored = await instance.invoke('get_pref', {
    args: { scope: { kind: 'vault' }, key: 'general.language' },
  })
  assert.deepEqual(stored, { ok: true, data: 'de' })
  ctx.step('FR-009 a new vault stores the active language')

  await openSettings(instance)
  await instance.click('settings-row-general.basic')
  await waitForLocation(instance, 'general.basic')
  await choose(instance, 'settings-language', 'en')
  await ctx.waitFor(
    'the settings in English',
    async () =>
      (await text(instance, '[data-testid="settings-title"]')) ===
      'Basic settings',
  )
  const changed = await instance.invoke('get_pref', {
    args: { scope: { kind: 'vault' }, key: 'general.language' },
  })
  assert.deepEqual(changed, { ok: true, data: 'en' })
  ctx.step('FR-010 the choice applies at once and is saved for the vault')
})
