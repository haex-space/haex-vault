import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import {
  activeTab,
  createEntry,
  openPasswords,
  selectTab,
} from '../lib/passwords.ts'
import { KEY, setSessionRestore } from '../lib/settings.ts'

const MARKER = 'SECRET-MARKER-E2E-RESTORE'

// Spec 034, FR-039 and FR-040 (quickstart §9): with session restore on, a password manager tab that
// shows an entry with its password revealed is restored after a restart at the same place, with the
// password masked again, and the stored session holds neither the password nor any title: places
// carry opaque ids only.
scenario('passwords-session-restore', { timeoutMs: 240_000 }, async (ctx) => {
  const group = await ctx.group({ users: { anna: ['laptop'] } })
  const device = group.device('anna/laptop')
  const page = device.page

  await setSessionRestore(page, true)
  const id = await createEntry(page, {
    title: 'Restore me',
    username: 'anna',
    password: MARKER,
  })
  await openPasswords(page)
  await page.click(`passwords-entry-${id}`)
  await page.waitForDisplayed('passwords-edit')
  // Enter toggles the value (a click does the same).
  await page.type('passwords-reveal-password', KEY.enter)
  await ctx.waitFor(
    'the password to show after "reveal"',
    () =>
      page.exec<boolean>(
        `return document.querySelector('[data-testid="passwords-value-password"]')?.innerText.includes(arguments[0]) ?? false`,
        [MARKER],
      ),
    { fixed: true, timeoutMs: 10_000 },
  )
  ctx.step('revealed')

  // Spec 036 (FR-006): the tab is part of the place and comes back with the session.
  await selectTab(page, 'extra')
  await ctx.waitFor(
    'the Extra tab',
    async () => (await activeTab(page)) === 'extra',
  )

  // The saved session names the place by its id and holds no value.
  const saved = (await ctx.waitFor(
    'the saved session to hold the entry place',
    async () => {
      const text = JSON.stringify(await page.invoke('wm_session_load'))
      return text.includes(`/entry/${id}`) && text.includes('extra')
        ? text
        : false
    },
    { fixed: true, timeoutMs: 15_000 },
  )) as string
  assert.ok(!saved.includes(MARKER), 'the saved session holds the password')
  assert.ok(!saved.includes('Restore me'), 'the saved session holds a title')
  ctx.step('session saved without a value')

  await device.restart()
  const restored = device.page
  await restored.waitForDisplayed('passwords-edit')
  await ctx.waitFor(
    'the Extra tab to come back',
    async () => (await activeTab(restored)) === 'extra',
    { fixed: true, timeoutMs: 15_000 },
  )
  const text = await restored.exec<string>('return document.body.innerText')
  assert.ok(text.includes('Restore me'), 'the entry is shown again')
  assert.ok(!text.includes(MARKER), 'the password is shown after a restart')
  await selectTab(restored, 'details')
  await restored.waitForDisplayed('passwords-reveal-password')
  ctx.step('restored at the same place, password masked')
})
