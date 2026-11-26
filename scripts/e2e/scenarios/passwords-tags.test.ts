import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { createAndUnlock, unwrap } from '../lib/flows.ts'
import type { FlowInstance } from '../lib/flows.ts'
import { dialogClosed } from '../lib/appearance.ts'
import { resizeAppWindow } from '../lib/settings.ts'
import { openWallet } from '../lib/wallet.ts'
import {
  createEntry,
  createPass,
  openPasswords,
  overview,
} from '../lib/passwords.ts'

// Spec 100 (quickstart M1, M2, M7, M8): the tag manager is one dropdown, not a list. 500 tags are
// reached by typing; the chosen tag has its editor with the usage per app; a blank name is refused
// under its field; a delete leaves no selection; the same manager opens from Wallet and shows the
// same numbers. The picker holds the tag ids in its test ids (`passwords-tag-picker-<id>`).

/** A tag row of the old list: `passwords-tag-<uuid>`; the picker and the filter do not match. */
const TAG_ROW = '^passwords-tag-[0-9a-f]{8}-[0-9a-f]{4}-'
const TAG_COUNT = 500

/** The test ids on screen that match `pattern`. */
const testIds = (instance: FlowInstance, pattern: string) =>
  instance.exec<string[]>(
    `const re = new RegExp(arguments[0])
     return [...document.querySelectorAll('[data-testid]')]
       .map((el) => el.getAttribute('data-testid') || '')
       .filter((id) => re.test(id))`,
    [pattern],
  )

/** The trimmed text of the element with the test id, `null` when it is not on screen. */
const textOf = (instance: FlowInstance, hook: string) =>
  instance.exec<string | null>(
    `const el = document.querySelector('[data-testid="' + arguments[0] + '"]')
     return el ? (el.textContent || '').trim() : null`,
    [hook],
  )

/** The value of the field with the test id (or of the input inside it). */
const valueOf = (instance: FlowInstance, hook: string) =>
  instance.exec<string | null>(
    `const host = document.querySelector('[data-testid="' + arguments[0] + '"]')
     const input = host && (host.matches('input, textarea') ? host : host.querySelector('input'))
     return input ? input.value : null`,
    [hook],
  )

/** Puts `text` into the field with the test id (or the input inside it), as typing would. */
const setField = (instance: FlowInstance, hook: string, text: string) =>
  instance.exec<boolean>(
    `const host = document.querySelector('[data-testid="' + arguments[0] + '"]')
     const input = host.matches('input, textarea') ? host : host.querySelector('input')
     Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(input, arguments[1])
     input.dispatchEvent(new Event('input', { bubbles: true }))
     return true`,
    [hook, text],
  )

/** Closes the open dialog the way a user does: with Escape. */
const pressEscape = (instance: FlowInstance) =>
  instance.exec<boolean>(
    `(document.activeElement ?? document).dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', keyCode: 27, bubbles: true }))
     return true`,
  )

const tagNamed = async (instance: FlowInstance, name: string) => {
  const tag = (await overview(instance)).tags.find((t) => t.name === name)
  assert.ok(tag, `no tag named "${name}" in the vault`)
  return tag
}

/** Chooses the tag with this name in the open manager and waits for its editor. */
async function pick(instance: FlowInstance, name: string): Promise<string> {
  const tag = await tagNamed(instance, name)
  await instance.click('passwords-tag-picker')
  await instance.type('passwords-tag-picker-search', name)
  await instance.click(`passwords-tag-picker-${tag.id}`)
  await instance.waitForDisplayed(`passwords-tag-${tag.id}`)
  return tag.id
}

/** The usage badges of the editor as numbers. */
async function usage(instance: FlowInstance) {
  return {
    login: await textOf(instance, 'passwords-tag-usage-login'),
    pass: await textOf(instance, 'passwords-tag-usage-pass'),
  }
}

scenario('passwords-tags', {}, async (ctx) => {
  const instance = await ctx.startInstance()
  await createAndUnlock(instance, { name: 'e2e-passwords-tags' })
  const names = Array.from(
    { length: TAG_COUNT },
    (_, i) => `tag-${String(i + 1).padStart(3, '0')}`,
  )
  await createEntry(instance, { title: 'Carries every tag', tags: names })
  // A vault holds tags of its own too (the cloud storage tag), so the scenario's tags are checked by
  // name, not by the number of all tags.
  const held = new Set((await overview(instance)).tags.map((t) => t.name))
  assert.deepEqual(
    names.filter((name) => !held.has(name)),
    [],
    'the vault should hold the tags made for the scenario',
  )
  await openPasswords(instance)

  // US1: one dropdown, no list.
  await instance.click('passwords-menu')
  await instance.click('passwords-menu-tags')
  await instance.waitForDisplayed('passwords-tag-picker')
  assert.deepEqual(
    await testIds(instance, TAG_ROW),
    [],
    'the manager should show no row per tag before one is chosen',
  )
  ctx.step('manager: a dropdown and no list')

  // Typing narrows the list; "tag-42" is tag-420 to tag-429.
  await instance.click('passwords-tag-picker')
  await instance.type('passwords-tag-picker-search', 'tag-42')
  const offered = (
    await testIds(instance, '^passwords-tag-picker-[0-9a-f]{8}-')
  ).length
  assert.equal(
    offered,
    10,
    `typing "tag-42" among ${TAG_COUNT} tags should offer 10, the picker offered ${offered}`,
  )
  await pressEscape(instance)
  ctx.step('search: 500 tags narrowed to 10')

  // Choose one: its editor shows, the usage is per app.
  const id = await pick(instance, 'tag-420')
  assert.deepEqual(
    await usage(instance),
    { login: '1', pass: '0' },
    'tag-420 is on one login and on no pass',
  )
  assert.deepEqual(
    (await testIds(instance, TAG_ROW)).length,
    1,
    'only the editor of the chosen tag should be on screen',
  )
  ctx.step('chosen tag: editor and usage per app')

  // FR-017 and the rules of rename: a name of other case that exists, nothing, spaces.
  // The editor stays in its rename mode after a refusal, so the field is opened once.
  await instance.click(`passwords-tag-rename-${id}`)
  const rename = async (text: string) => {
    await setField(instance, `passwords-tag-name-${id}`, text)
    await instance.click(`passwords-tag-save-${id}`)
  }
  const refused = (text: string) =>
    ctx.waitFor(`the hint for the name "${text}" under the field`, () =>
      instance.exec<boolean>(
        `return !!document.querySelector('[data-testid="passwords-tag-${id}"] [role="alert"]')`,
      ),
    )
  for (const text of ['TAG-421', '', '   ']) {
    await rename(text)
    await refused(text)
    assert.equal(
      (await overview(instance)).tags.find((t) => t.id === id)?.name,
      'tag-420',
      `the name "${text}" must be refused and leave tag-420 as it was`,
    )
  }
  ctx.step('rename: exists, empty and blank are refused')

  await rename('Renamed')
  await ctx.waitFor('the tag to carry its new name', async () =>
    (await overview(instance)).tags.some(
      (t) => t.id === id && t.name === 'Renamed',
    ),
  )
  ctx.step('rename: a proper name')

  // Delete: confirm, the selection is gone and the dropdown does not offer the tag.
  await instance.click(`passwords-tag-delete-${id}`)
  await instance.click('passwords-tag-delete-confirm')
  await ctx.waitFor('the tag to be deleted', async () =>
    (await overview(instance)).tags.every((t) => t.id !== id),
  )
  await ctx.waitFor(
    'the editor to leave with the tag',
    async () => (await testIds(instance, TAG_ROW)).length === 0,
  )
  await instance.click('passwords-tag-picker')
  await instance.type('passwords-tag-picker-search', 'Renamed')
  assert.equal(
    (await testIds(instance, '^passwords-tag-picker-[0-9a-f]{8}-')).length,
    0,
    'the deleted tag should no longer be offered',
  )
  // The first Escape closes the picker, the second the manager. Sent at once, the second reaches the
  // picker while it closes and the manager stays open over the window.
  await pressEscape(instance)
  await ctx.waitFor(
    'the picker to close',
    async () =>
      (await testIds(instance, '^passwords-tag-picker-search$')).length === 0,
  )
  await pressEscape(instance)
  await ctx.waitFor('the manager to close', () => dialogClosed(instance))
  ctx.step('delete: no selection, not offered')

  // M7: the usage is per app and matches the filter of each app; Wallet opens the same manager.
  const concert = [] as string[]
  for (let i = 0; i < 5; i += 1)
    concert.push(await createEntry(instance, { title: `Login ${i}` }))
  for (let i = 0; i < 5; i += 1)
    concert.push(await createPass(instance, { title: `Pass ${i}` }))
  unwrap(
    'passwords_set_tags',
    await instance.invoke('passwords_set_tags', {
      args: { itemIds: concert, add: ['Konzert'], remove: [] },
    }),
  )
  const concertId = (await tagNamed(instance, 'Konzert')).id

  await ctx.waitFor(
    'the filter of the password manager to count 5',
    async () => {
      await instance.click('passwords-tag-filter')
      const label = await textOf(instance, `passwords-tag-filter-${concertId}`)
      await instance.click('passwords-tag-filter-all')
      return !!label?.includes('(5)')
    },
  )
  await instance.click('passwords-menu')
  await instance.click('passwords-menu-tags')
  await pick(instance, 'Konzert')
  await ctx.waitFor('the manager to show 5 and 5', async () => {
    const shown = await usage(instance)
    return shown.login === '5' && shown.pass === '5'
  })
  ctx.step('usage: 5 in the password manager and 5 in Wallet, not 10')
  // Choosing a tag closes the picker. An Escape sent while it still closes does not reach the
  // manager, which then stays open over the window.
  await ctx.waitFor(
    'the picker to close',
    async () =>
      (await testIds(instance, '^passwords-tag-picker-search$')).length === 0,
  )
  await pressEscape(instance)
  await ctx.waitFor('the manager to close', () => dialogClosed(instance))

  await openWallet(instance)
  await resizeAppWindow(instance, 'system.wallet', 760, 560)
  await instance.click('wallet-tags')
  await instance.waitForDisplayed('passwords-tag-picker')
  await pick(instance, 'Konzert')
  await ctx.waitFor('Wallet to show the same 5 and 5', async () => {
    const shown = await usage(instance)
    return shown.login === '5' && shown.pass === '5'
  })
  ctx.step('Wallet opens the same manager with the same numbers')
})

/** The tag names an item carries, sorted, as the backend holds them. */
async function tagNamesOf(instance: FlowInstance, itemId: string) {
  const header = (await overview(instance)).headers.find((h) => h.id === itemId)
  assert.ok(header, `no item ${itemId} in the vault`)
  return header.tags.map((tag) => tag.name).sort()
}

/** Tags an item (login or pass) through the command the selection bar uses. */
async function tagItem(instance: FlowInstance, itemId: string, name: string) {
  unwrap(
    'passwords_set_tags',
    await instance.invoke('passwords_set_tags', {
      args: { itemIds: [itemId], add: [name], remove: [] },
    }),
  )
}

// Spec 100, US2 (quickstart M3): "Job" is merged into "Arbeit". Merging is not offered with one tag;
// the confirmation names both tags and the numbers of both apps; cancelling changes nothing; after
// the confirmation every entry and pass carries "Arbeit" once, "Job" is gone, "Arbeit" keeps its
// name and color and is the selected tag with the union of the counts.
scenario('passwords-tags-merge', {}, async (ctx) => {
  const instance = await ctx.startInstance()
  await createAndUnlock(instance, { name: 'e2e-passwords-tags-merge' })
  const alone = await createEntry(instance, {
    title: 'Alone',
    tags: ['Solo'],
  })
  // A fresh vault holds a tag of its own (the cloud storage tag); "one tag" needs it gone.
  const others = (await overview(instance)).tags.filter(
    (t) => t.name !== 'Solo',
  )
  for (const other of others)
    unwrap(
      'passwords_delete_tag',
      await instance.invoke('passwords_delete_tag', {
        args: { tagIds: [other.id] },
      }),
    )
  await openPasswords(instance)
  await instance.click('passwords-menu')
  await instance.click('passwords-menu-tags')
  await pick(instance, 'Solo')
  assert.deepEqual(
    await testIds(instance, '^passwords-tag-merge$'),
    [],
    'with a single tag there is nobody to merge into, so no merge button should show',
  )
  ctx.step('one tag: no merge offered')

  const onlyJob = await createEntry(instance, {
    title: 'Only Job',
    tags: ['Job'],
  })
  const both = await createEntry(instance, {
    title: 'Both',
    tags: ['Job', 'Arbeit'],
  })
  const onlyWork = await createEntry(instance, {
    title: 'Only Arbeit',
    tags: ['Arbeit'],
  })
  const pass = await createPass(instance, { title: 'Job pass' })
  await tagItem(instance, pass, 'Job')
  const job = (await tagNamed(instance, 'Job')).id
  const arbeit = (await tagNamed(instance, 'Arbeit')).id
  unwrap(
    'passwords_set_tag_color',
    await instance.invoke('passwords_set_tag_color', {
      args: { tagIds: [arbeit], color: '#336699' },
    }),
  )

  // Choose "Job", start the merge, choose "Arbeit": the confirmation names the numbers.
  await pick(instance, 'Job')
  await instance.click('passwords-tag-merge')
  await instance.waitForDisplayed('passwords-tag-merge-target')
  const chooseTarget = async () => {
    await instance.click('passwords-tag-merge-target')
    await instance.type('passwords-tag-picker-search', 'Arbeit')
    await instance.click(`passwords-tag-picker-${arbeit}`)
    await instance.waitForDisplayed('passwords-tag-merge-body')
  }
  await chooseTarget()
  // The merge is a picture: old tag with its numbers per app, an arrow, the name of the result.
  const fromText = await textOf(instance, 'passwords-tag-merge-from')
  assert.ok(
    fromText?.includes('Job'),
    `the old tag should be named in the picture, it showed: ${fromText}`,
  )
  const inFrom = (hook: string) =>
    instance.exec<string | null>(
      `const el = document.querySelector('[data-testid="passwords-tag-merge-from"] [data-testid="' + arguments[0] + '"]')
       return el ? (el.textContent || '').trim() : null`,
      [hook],
    )
  assert.deepEqual(
    [
      await inFrom('passwords-tag-usage-login'),
      await inFrom('passwords-tag-usage-pass'),
    ],
    ['2', '1'],
    'the picture should show 2 logins and 1 pass for "Job"',
  )
  assert.equal(
    await valueOf(instance, 'passwords-tag-merge-name'),
    'Arbeit',
    'the name of the result should start as the name of the target',
  )
  // What the merge means for both apps is behind the info button, not in the way.
  assert.deepEqual(
    await testIds(instance, '^passwords-tag-merge-info-text$'),
    [],
    'the explanation should not show before the info button is pressed',
  )
  await instance.click('passwords-tag-merge-info')
  await instance.waitForDisplayed('passwords-tag-merge-info-text')
  const info = await textOf(instance, 'passwords-tag-merge-info-text')
  assert.ok(
    info?.includes('Job') && /\b2\b[^0-9]*\b1\b/.test(info),
    `the explanation should name "Job" and 2 and 1, it said: ${info}`,
  )
  await pressEscape(instance)
  await ctx.waitFor(
    'the explanation to close',
    async () =>
      (await testIds(instance, '^passwords-tag-merge-info-text$')).length === 0,
  )
  ctx.step(
    'merge: old tag, numbers per app, name of the result, info behind a button',
  )

  // Cancel: nothing changes.
  await pressEscape(instance)
  await ctx.waitFor(
    'the confirmation to close',
    async () =>
      (await testIds(instance, '^passwords-tag-merge-confirm$')).length === 0,
  )
  assert.deepEqual(
    await tagNamesOf(instance, onlyJob),
    ['Job'],
    'a cancelled merge must leave the entries as they were',
  )
  assert.ok(
    (await overview(instance)).tags.some((t) => t.id === job),
    'a cancelled merge must leave the tag "Job" in the vault',
  )
  ctx.step('merge: cancelling changes nothing')

  // Confirm.
  await instance.click('passwords-tag-merge')
  await instance.waitForDisplayed('passwords-tag-merge-target')
  await chooseTarget()
  await instance.click('passwords-tag-merge-confirm')
  await ctx.waitFor('"Job" to be merged away', async () =>
    (await overview(instance)).tags.every((t) => t.id !== job),
  )
  for (const [label, id] of [
    ['only Job', onlyJob],
    ['both', both],
    ['only Arbeit', onlyWork],
    ['the pass', pass],
  ] as const) {
    assert.deepEqual(
      await tagNamesOf(instance, id),
      ['Arbeit'],
      `${label} should carry "Arbeit" exactly once after the merge`,
    )
  }
  const target = (await overview(instance)).tags.find((t) => t.id === arbeit)
  assert.equal(target?.name, 'Arbeit', 'the target keeps its name')
  ctx.step(
    'merge: every entry and pass carries the target once, the source is gone',
  )

  // The target is selected with the union of both tags' entries.
  await instance.waitForDisplayed(`passwords-tag-${arbeit}`)
  await ctx.waitFor(
    'the manager to count 3 logins and 1 pass for "Arbeit"',
    async () => {
      const shown = await usage(instance)
      return shown.login === '3' && shown.pass === '1'
    },
  )
  await instance.click('passwords-tag-picker')
  await instance.type('passwords-tag-picker-search', 'Job')
  assert.equal(
    (await testIds(instance, '^passwords-tag-picker-[0-9a-f]{8}-')).length,
    0,
    'the merged tag should no longer be offered',
  )
  ctx.step(
    'merge: the target is selected with the union, the source is not offered',
  )

  // The merged tag can get a new name in the same step.
  await pressEscape(instance)
  await pick(instance, 'Solo')
  await instance.click('passwords-tag-merge')
  await instance.waitForDisplayed('passwords-tag-merge-target')
  await chooseTarget()
  await setField(instance, 'passwords-tag-merge-name', 'Beruf')
  await instance.click('passwords-tag-merge-confirm')
  await ctx.waitFor(
    '"Solo" to be merged into a tag named "Beruf"',
    async () => {
      const { tags } = await overview(instance)
      return (
        tags.every((t) => t.name !== 'Solo') &&
        tags.some((t) => t.id === arbeit && t.name === 'Beruf')
      )
    },
  )
  for (const id of [onlyJob, both, onlyWork, pass, alone])
    assert.deepEqual(
      await tagNamesOf(instance, id),
      ['Beruf'],
      'every entry should carry the renamed result, once',
    )
  ctx.step('merge: the result can be renamed in the same step')
})

// WebDriver key codes, as `sendKeys` takes them.
const ENTER = '\uE007'
const ARROW_DOWN = '\uE015'

// Spec 100, US3 (FR-012, quickstart M6): select, merge and delete with the keyboard alone: Enter
// opens the dropdown from its trigger, typing narrows it, the arrow key highlights, Enter picks, and
// focus returns to the trigger; every button is pressed with Enter.
scenario('passwords-tags-keyboard', {}, async (ctx) => {
  const instance = await ctx.startInstance()
  await createAndUnlock(instance, { name: 'e2e-passwords-tags-keyboard' })
  const entry = await createEntry(instance, {
    title: 'Keyboard',
    tags: ['alpha', 'beta', 'gamma'],
  })
  await openPasswords(instance)
  await instance.click('passwords-menu')
  await instance.click('passwords-menu-tags')
  await instance.waitForDisplayed('passwords-tag-picker')

  // The dropdown opens from its trigger by keyboard, narrows by typing and picks with Enter.
  const beta = (await tagNamed(instance, 'beta')).id
  await instance.type('passwords-tag-picker', ENTER)
  await instance.waitForDisplayed('passwords-tag-picker-search')
  await instance.type('passwords-tag-picker-search', 'bet')
  await instance.type('passwords-tag-picker-search', ARROW_DOWN)
  await instance.type('passwords-tag-picker-search', ENTER)
  await instance.waitForDisplayed(`passwords-tag-${beta}`)
  await ctx.waitFor('focus to return to the dropdown trigger', () =>
    instance.exec<boolean>(
      `return document.activeElement?.getAttribute('data-testid') === 'passwords-tag-picker'`,
    ),
  )
  ctx.step('keyboard: select a tag, focus returns to the trigger')

  // The rename control is a button: Enter opens the field.
  await instance.type(`passwords-tag-rename-${beta}`, ENTER)
  await instance.waitForDisplayed(`passwords-tag-name-${beta}`)
  ctx.step('keyboard: the rename field opens')

  // Merge: the merge button, the target by typing, the confirmation button, all with Enter.
  const gamma = (await tagNamed(instance, 'gamma')).id
  await instance.type(`passwords-tag-save-${beta}`, ENTER)
  await instance.waitForDisplayed(`passwords-tag-rename-${beta}`)
  await instance.type('passwords-tag-merge', ENTER)
  await instance.waitForDisplayed('passwords-tag-merge-target')
  await instance.type('passwords-tag-merge-target', ENTER)
  await instance.type('passwords-tag-picker-search', 'gam')
  await instance.type('passwords-tag-picker-search', ARROW_DOWN)
  await instance.type('passwords-tag-picker-search', ENTER)
  await instance.waitForDisplayed('passwords-tag-merge-body')
  await instance.type('passwords-tag-merge-confirm', ENTER)
  await ctx.waitFor('"beta" to be merged into "gamma"', async () =>
    (await overview(instance)).tags.every((t) => t.id !== beta),
  )
  assert.deepEqual(
    await tagNamesOf(instance, entry),
    ['alpha', 'gamma'],
    'merging beta into gamma by keyboard should leave alpha and gamma on the entry',
  )
  await instance.waitForDisplayed(`passwords-tag-${gamma}`)
  ctx.step('keyboard: merge')

  // Delete: the delete button and the confirmation, with Enter.
  await instance.type(`passwords-tag-delete-${gamma}`, ENTER)
  await instance.type('passwords-tag-delete-confirm', ENTER)
  await ctx.waitFor('"gamma" to be deleted', async () =>
    (await overview(instance)).tags.every((t) => t.id !== gamma),
  )
  ctx.step('keyboard: delete')
})
