import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { createAndUnlock, unwrap } from '../lib/flows.ts'
import { exists } from '../lib/sync-ui.ts'
import {
  activeTab,
  openPasswords,
  overview,
  selectTab,
} from '../lib/passwords.ts'
import { KEY } from '../lib/settings.ts'
import type { FlowInstance } from '../lib/flows.ts'

async function storedAliases(
  instance: FlowInstance,
  id: string,
): Promise<string | null> {
  return unwrap<{ autofillAliases: string | null }>(
    'passwords_get_item',
    await instance.invoke('passwords_get_item', { args: { itemId: id } }),
  ).autofillAliases
}

async function shownAliases(instance: FlowInstance): Promise<string[]> {
  return instance.exec<string[]>(
    `return [...document.querySelectorAll('[data-testid^="passwords-alias-"]')]
       .filter((el) => /^passwords-alias-\\d+$/.test(el.dataset.testid))
       .map((el) => el.textContent.trim())`,
  )
}

// Spec 036 FR-001a: the autofill aliases in the tab Extra. A name added to the user name with Enter
// is saved as the JSON object the browser extension reads and shown in the view; removing it again
// stores nothing and hides the group in the view.
scenario('passwords-autofill-aliases', {}, async (ctx) => {
  const instance = await ctx.startInstance()
  await createAndUnlock(instance, { name: 'e2e-passwords-aliases' })
  await openPasswords(instance)

  await instance.click('passwords-new')
  await instance.click('passwords-new-entry')
  await instance.waitForDisplayed('passwords-editor')
  await instance.type('passwords-field-title', 'Hugging Face')
  await selectTab(instance, 'extra')
  await ctx.waitFor(
    'the Extra tab',
    async () => (await activeTab(instance)) === 'extra',
  )
  // Row 0 is the user name.
  await instance.type('passwords-alias-input-0', `account_name${KEY.enter}`)
  await ctx.waitFor('the name to show as a chip', async () =>
    (await shownAliases(instance)).includes('account_name'),
  )
  assert.ok(
    await exists(instance, 'passwords-editor'),
    'Enter in the alias input left the editor',
  )
  await instance.click('passwords-editor-save')
  await instance.waitForDisplayed('passwords-edit')
  const id = (await overview(instance)).headers[0]?.id
  assert.ok(id, 'the entry exists in the overview')
  assert.equal(
    await storedAliases(instance, id),
    '{"username":["account_name"]}',
  )
  await selectTab(instance, 'extra')
  await ctx.waitFor('the alias in the view', async () =>
    (await shownAliases(instance)).includes('account_name'),
  )
  ctx.step('an alias added in the editor is saved and shown')

  await instance.click('passwords-edit')
  await instance.waitForDisplayed('passwords-editor')
  await selectTab(instance, 'extra')
  await instance.click('passwords-alias-remove-0')
  await ctx.waitFor(
    'the chip to go',
    async () => (await shownAliases(instance)).length === 0,
  )
  await instance.click('passwords-editor-save')
  await instance.waitForDisplayed('passwords-edit')
  assert.equal(await storedAliases(instance, id), null)
  await selectTab(instance, 'extra')
  assert.equal(
    await exists(instance, 'passwords-aliases'),
    false,
    'the view hides the group without aliases',
  )
  ctx.step('removing the last alias stores none')
})
