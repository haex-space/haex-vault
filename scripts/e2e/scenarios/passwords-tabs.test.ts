import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { createAndUnlock } from '../lib/flows.ts'
import { KEY, runAction } from '../lib/settings.ts'
import {
  activeTab,
  createEntry,
  historyIds,
  openPasswords,
  selectTab,
  slideShows,
  updateEntry,
} from '../lib/passwords.ts'

// Spec 036, quickstart M1 and M2 (US1, US2): an entry shows Details and Extra as tabs. A tap or an arrow key
// changes the tab and the slide follows; the tab belongs to the place, so back and forward find it;
// the editor keeps the inputs of Details while Extra is shown; a save that fails at the TOTP field
// jumps to Details; the Verlauf tab offers the states in a dropdown (spec 039), newest chosen, a secret hides again
// when the tab is left, and a restore (after a confirmation) makes a new top state and keeps the older
// ones. The swipe gesture itself is manual (the rig has no reliable touch gestures).
scenario('passwords-tabs', {}, async (ctx) => {
  const instance = await ctx.startInstance()
  await createAndUnlock(instance, { name: 'e2e-passwords-tabs' })
  await openPasswords(instance)
  const id = await createEntry(instance, {
    title: 'Mail',
    username: 'anna',
    password: 'SECRET-MARKER-E2E-TABS',
  })
  await instance.click(`passwords-entry-${id}`)
  await instance.waitForDisplayed('entry-tabs')
  assert.equal(
    await activeTab(instance),
    'details',
    'an entry opens on Details',
  )
  assert.ok(await slideShows(instance, 'details'))
  ctx.step('opens on details')

  // A tap changes the tab and the slide follows.
  await selectTab(instance, 'extra')
  await ctx.waitFor(
    'the Extra tab',
    async () => (await activeTab(instance)) === 'extra',
  )
  await ctx.waitFor('the Extra slide', async () =>
    slideShows(instance, 'extra'),
  )
  ctx.step('tap changes the tab and the slide')

  // An arrow key on the tab bar changes it back.
  await instance.type('entry-tab-extra', KEY.arrowLeft)
  await ctx.waitFor(
    'Details again',
    async () => (await activeTab(instance)) === 'details',
  )
  await ctx.waitFor('the Details slide', async () =>
    slideShows(instance, 'details'),
  )
  ctx.step('arrow key changes the tab')

  // The tab belongs to the place: Extra survives going back to the list and forward again.
  await selectTab(instance, 'extra')
  await ctx.waitFor(
    'the Extra tab',
    async () => (await activeTab(instance)) === 'extra',
  )
  assert.ok((await runAction(instance, 'wm.tab.back')).ok)
  await instance.waitForDisplayed('passwords-new')
  assert.ok((await runAction(instance, 'wm.tab.forward')).ok)
  await instance.waitForDisplayed('entry-tabs')
  await ctx.waitFor(
    'Extra after forward',
    async () => (await activeTab(instance)) === 'extra',
  )
  ctx.step('the tab survives back and forward')

  // Editing keeps the tab; the inputs of Details survive a visit to Extra.
  await instance.click('passwords-edit')
  await instance.waitForDisplayed('passwords-editor')
  assert.equal(
    await activeTab(instance),
    'extra',
    'the editor opens on the same tab',
  )
  await selectTab(instance, 'details')
  await instance.type('passwords-field-username', '-changed')
  await selectTab(instance, 'extra')
  await ctx.waitFor(
    'the Extra tab',
    async () => (await activeTab(instance)) === 'extra',
  )
  await selectTab(instance, 'details')
  await ctx.waitFor(
    'the Details tab',
    async () => (await activeTab(instance)) === 'details',
  )
  const username = await instance.exec<string>(
    `return document.querySelector('[data-testid="passwords-field-username"]').value`,
  )
  assert.equal(username, 'anna-changed', 'the inputs survive the tab change')
  ctx.step('editing keeps inputs across tabs')

  // A save that fails at the TOTP field (on Details) jumps there from Extra.
  await instance.type('passwords-field-otp', 'not a secret!!')
  await selectTab(instance, 'extra')
  await ctx.waitFor(
    'the Extra tab',
    async () => (await activeTab(instance)) === 'extra',
  )
  await instance.click('passwords-editor-save')
  await ctx.waitFor(
    'the jump to Details',
    async () => (await activeTab(instance)) === 'details',
  )
  await instance.waitForDisplayed('passwords-field-otp')
  ctx.step('a failed save jumps to the tab holding the field')

  // Leave the editor; the Verlauf tab exists for the saved entry only.
  await instance.click('passwords-editor-cancel')
  // The draft is dirty (a changed user name, a rejected TOTP value): leaving asks first.
  await instance.waitForDisplayed('passwords-unsaved-discard')
  await instance.click('passwords-unsaved-discard')
  await instance.waitForDisplayed('entry-tabs')
  assert.equal(
    await instance.exec<boolean>(
      `return Boolean(document.querySelector('[data-testid="entry-tab-history"]'))`,
    ),
    true,
    'the saved entry offers Verlauf',
  )

  // The history: three changes make four states; the dropdown offers them newest first.
  await updateEntry(instance, id, { username: 'anna-2' })
  await updateEntry(instance, id, { username: 'anna-3' })
  await updateEntry(instance, id, { password: 'SECRET-MARKER-E2E-TABS-NEW' })
  const states = await historyIds(instance, id)
  assert.ok(
    states.length >= 4,
    `expected 4 or more states, got ${states.length}`,
  )
  await instance.click('passwords-all')
  await instance.waitForDisplayed(`passwords-entry-${id}`)
  await instance.click(`passwords-entry-${id}`)
  await instance.waitForDisplayed('entry-tabs')
  await selectTab(instance, 'history')
  await ctx.waitFor(
    'the Verlauf tab',
    async () => (await activeTab(instance)) === 'history',
  )
  await ctx.waitFor('the Verlauf slide', async () =>
    slideShows(instance, 'history'),
  )
  // The states are offered by a dropdown (spec 039); its options follow the history's order.
  const offered = async () =>
    instance.exec<{ count: number; checked: number; focused: boolean }>(
      `const all = [...document.querySelectorAll('[role="option"]')]
       return {
         count: all.length,
         checked: all.findIndex((o) => o.getAttribute('data-state') === 'checked'),
         focused: all.includes(document.activeElement),
       }`,
    )
  const openStates = async () => {
    await instance.click('passwords-history-select')
    await ctx.waitFor(
      'the history states to open with a focused option',
      async () => (await offered()).focused,
    )
  }
  const closedStates = async () =>
    ctx.waitFor(
      'the history states to close',
      async () => (await offered()).count === 0,
    )
  await instance.waitForDisplayed('passwords-history-select')
  await openStates()
  const first = await offered()
  assert.equal(first.count, states.length, 'one option per state')
  assert.equal(first.checked, 0, 'the newest state is chosen first')
  await instance.typeToFocused(KEY.arrowDown)
  await instance.typeToFocused(KEY.enter)
  await closedStates()
  await openStates()
  assert.equal(
    (await offered()).checked,
    1,
    'the arrow key and Enter choose the next state',
  )
  ctx.step('the dropdown offers every state, newest chosen')

  // A secret of a state hides again when the tab is left.
  await instance.typeToFocused(KEY.end)
  await instance.typeToFocused(KEY.enter)
  await closedStates()
  await instance.waitForDisplayed('passwords-history-snapshot')
  // A click or the keyboard toggles a reveal.
  await instance.type('passwords-reveal-history-password', KEY.enter)
  await ctx.waitFor('the secret to show', async () =>
    instance.exec<boolean>(
      `return document.querySelector('[data-testid="passwords-value-history-password"]').textContent.includes('SECRET-MARKER-E2E-TABS')`,
    ),
  )
  await selectTab(instance, 'details')
  await ctx.waitFor(
    'Details',
    async () => (await activeTab(instance)) === 'details',
  )
  await ctx.waitFor('the secret to hide again', async () =>
    instance.exec<boolean>(
      `return !document.querySelector('[data-testid="passwords-value-history-password"]').textContent.includes('SECRET-MARKER-E2E-TABS')`,
    ),
  )
  ctx.step('a secret hides again when the tab is left')

  // Restore the oldest state after a confirmation: a new top state, nothing shortened.
  const before = (await historyIds(instance, id)).length
  await selectTab(instance, 'history')
  await openStates()
  await instance.typeToFocused(KEY.end)
  await instance.typeToFocused(KEY.enter)
  await closedStates()
  await instance.click('passwords-history-restore')
  await instance.waitForDisplayed('passwords-history-restore-confirm')
  await instance.click('passwords-history-restore-confirm')
  await ctx.waitFor(
    'the restore to land on Details',
    async () => (await activeTab(instance)) === 'details',
  )
  const after = await historyIds(instance, id)
  assert.equal(after.length, before + 1, 'a restore adds a state')
  for (const state of states)
    assert.ok(after.includes(state), 'older states stay')
  ctx.step('restore makes a new top state and keeps the older ones')
})
