import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { createAndUnlock, openChat } from '../lib/flows.ts'
import { openVault } from '../lib/sync-flows.ts'
import {
  backgroundContrast,
  choose,
  openSettings,
  textContrast,
  waitForLocation,
} from '../lib/settings.ts'

type Scheme = { dark: boolean; colorScheme: string }
const scheme = (instance: { exec<T>(s: string): Promise<T> }) =>
  instance.exec<Scheme>(
    `const root = document.documentElement
     return { dark: root.classList.contains('dark'), colorScheme: root.style.colorScheme }`,
  )

// Spec 023-settings-app, quickstart S11, S12 and S20 (FR-013, FR-014, FR-021, SC-005). The operating
// system is dark here. Each choice applies at once without a save button, to every open window (one
// document); "System" follows the operating system; the choice is the vault's, so the start page follows
// the system and the workspace gets it back. In dark mode text, select values and entries, and the off
// switch stay readable against what is behind them.
scenario('settings-color-scheme', {}, async (ctx) => {
  const instance = await ctx.startInstance({ colorScheme: 'dark' })
  const name = 'e2e-settings-scheme'
  const { passphrase } = ctx.credentials()
  await createAndUnlock(instance, { name, passphrase })
  await openChat(instance)
  await openSettings(instance)
  await instance.click('settings-category-general')
  await instance.click('settings-row-general.appearance')
  await waitForLocation(instance, 'general.appearance')

  // Non-text contrast of the off switch's track against its card, at least 3:1 (WCAG 1.4.11).
  const trackContrast = async () => {
    // General settings load the session-restore preference asynchronously after navigation.
    // Wait for the control before measuring it so the assertion does not race that load.
    await instance.waitForDisplayed('session-restore-switch')
    const ratio = await backgroundContrast(
      instance,
      '[data-testid="session-restore-switch"]',
      'ul:has([data-testid="session-restore-switch"])',
    )
    assert.ok(
      ratio >= 3,
      `off switch track: contrast ${ratio.toFixed(2)} < 3 against its card`,
    )
    return ratio
  }

  await choose(instance, 'settings-color-scheme', 'light')
  await ctx.waitFor('light', async () => !(await scheme(instance)).dark)
  assert.equal((await scheme(instance)).colorScheme, 'light')
  await choose(instance, 'settings-color-scheme', 'system')
  await ctx.waitFor('system (dark)', async () => (await scheme(instance)).dark)
  await choose(instance, 'settings-color-scheme', 'dark')
  await ctx.waitFor('dark', async () => (await scheme(instance)).dark)
  assert.equal((await scheme(instance)).colorScheme, 'dark')
  const pref = await instance.invoke('get_pref', {
    args: { scope: { kind: 'vault' }, key: 'appearance.color_scheme' },
  })
  assert.deepEqual(pref, { ok: true, data: 'dark' })
  ctx.step('S11 each choice at once, saved for the vault')

  // S20: readable in dark mode.
  const text: Array<[string, string]> = [
    ['title', '[data-testid="settings-title"]'],
    ['select value', '[data-testid="settings-color-scheme"] span'],
  ]
  for (const [what, selector] of text) {
    const ratio = await textContrast(instance, selector)
    assert.ok(ratio >= 4.5, `${what}: contrast ${ratio.toFixed(2)} < 4.5`)
  }
  await instance.click('settings-color-scheme')
  await instance.waitForDisplayed('[role="option"][data-value="light"]')
  for (const value of ['light', 'dark', 'system']) {
    const ratio = await textContrast(
      instance,
      `[role="option"][data-value="${value}"]`,
    )
    assert.ok(
      ratio >= 4.5,
      `select entry ${value}: contrast ${ratio.toFixed(2)} < 4.5`,
    )
  }
  await instance.click('[role="option"][data-value="dark"]')
  await instance.click('settings-category-general')
  await instance.click('settings-row-general.basic')
  await waitForLocation(instance, 'general.basic')
  const alias = await textContrast(instance, '[data-testid="settings-alias"]')
  assert.ok(alias >= 4.5, `name field: contrast ${alias.toFixed(2)} < 4.5`)
  const track = await trackContrast()
  ctx.step(`S20 readable in dark mode (switch track ${track.toFixed(2)})`)

  // The off switch's track also stands out from its card in the light scheme.
  await instance.click('settings-category-general')
  await instance.click('settings-row-general.appearance')
  await waitForLocation(instance, 'general.appearance')
  await choose(instance, 'settings-color-scheme', 'light')
  await ctx.waitFor('light again', async () => !(await scheme(instance)).dark)
  await instance.click('settings-category-general')
  await instance.click('settings-row-general.basic')
  await waitForLocation(instance, 'general.basic')
  const lightTrack = await trackContrast()
  ctx.step(`switch track in light mode ${lightTrack.toFixed(2)}`)

  // S12: the start page follows the system, the workspace brings the vault's choice back. One process
  // holds one vault session (spec 013), and the start page sends an open vault to its workspace at once:
  // it shows only before the vault opens, so the check starts the app again over the same data.
  await instance.stop()
  const next = await ctx.startInstance({
    colorScheme: 'dark',
    reusesRoot: instance.root,
  })
  await ctx.waitFor('the start page follows the system', async () => {
    const path = await next.exec<string>('return location.pathname')
    return path === '/' && (await scheme(next)).dark
  })
  await openVault(next, name, passphrase)
  await ctx.waitFor('the workspace brings light back', async () => {
    const path = await next.exec<string>('return location.pathname')
    return path.startsWith('/workspace/') && !(await scheme(next)).dark
  })
  ctx.step('S12 start page and workspace')
})
