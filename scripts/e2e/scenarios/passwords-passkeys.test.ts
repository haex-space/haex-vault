import assert from 'node:assert/strict'
import { generateKeyPairSync } from 'node:crypto'
import { scenario, type ScenarioContext } from '../lib/scenario.ts'
import { KEY } from '../lib/settings.ts'
import { createAndUnlock, unwrap, type FlowInstance } from '../lib/flows.ts'
import {
  contextMenu,
  importExport,
  inEditorExtra,
  openPasswords,
  overview,
  selectTab,
  slideSettled,
} from '../lib/passwords.ts'

// Spec 036, quickstart M5 (US5): a passkey imported from a Bitwarden export shows in the tab
// Extra with relying party and user; it is renamed; a copy with "Passkeys per Verweis" shows it as
// "Verweis auf …" without rename or delete; "Verweis lösen" removes only the link; deleting the
// passkey asks with the relying party named.

interface Detail {
  passkeys: Array<{
    id: string
    nickname: string | null
    linkedFrom: { itemId: string } | null
  }>
}

async function detail(instance: FlowInstance, id: string): Promise<Detail> {
  return unwrap<Detail>(
    'passwords_get_item',
    await instance.invoke('passwords_get_item', { args: { itemId: id } }),
  )
}

/** A Bitwarden JSON export with one login that carries an ES256 passkey. */
function bitwardenExport(): string {
  const { privateKey } = generateKeyPairSync('ec', { namedCurve: 'P-256' })
  const keyValue = privateKey
    .export({ format: 'der', type: 'pkcs8' })
    .toString('base64')
  return JSON.stringify({
    encrypted: false,
    folders: [],
    items: [
      {
        type: 1,
        name: 'Beispiel',
        login: {
          username: 'anna',
          fido2Credentials: [
            {
              credentialId: '11111111-2222-3333-4444-555555555555',
              keyType: 'public-key',
              keyAlgorithm: 'ECDSA',
              keyCurve: 'P-256',
              keyValue,
              rpId: 'example.com',
              rpName: 'Example',
              userHandle: 'dXNlcg',
              userName: 'anna@example.com',
              counter: '0',
              discoverable: 'true',
              creationDate: '2026-01-01T00:00:00.000Z',
            },
          ],
        },
      },
    ],
  })
}

/** Opens the tab Extra of the shown entry and waits until its slide has come to rest. */
async function openExtra(ctx: ScenarioContext, instance: FlowInstance) {
  await selectTab(instance, 'extra')
  await ctx.waitFor('the slide Extra to rest', () =>
    slideSettled(instance, 'extra'),
  )
}

scenario('passwords-passkeys', { timeoutMs: 240_000 }, async (ctx) => {
  const instance = await ctx.startInstance()
  await createAndUnlock(instance, { name: 'e2e-passwords-passkeys' })
  await openPasswords(instance)

  await importExport(instance, 'bitwarden', 'bitwarden.json', bitwardenExport())
  const original = (await ctx.waitFor('the imported entry', async () => {
    const entry = (await overview(instance)).headers.find(
      (h) => h.title === 'Beispiel',
    )
    return entry?.id ?? false
  })) as string
  const [passkey] = (await detail(instance, original)).passkeys
  assert.ok(passkey, 'the import brought the passkey')

  // The tab Extra shows it with relying party and user.
  await instance.click(`passwords-entry-${original}`)
  await openExtra(ctx, instance)
  await instance.waitForDisplayed(`passwords-passkey-row-${passkey.id}`)
  const row = await instance.exec<string>(
    `return document.querySelector('[data-testid="passwords-passkey-row-' + arguments[0] + '"]').textContent`,
    [passkey.id],
  )
  assert.ok(row.includes('example.com'), row)
  assert.ok(row.includes('anna@example.com'), row)
  await instance.waitForDisplayed(
    `passwords-passkey-discoverable-${passkey.id}`,
  )
  ctx.step('the imported passkey shows in the tab Extra')

  // The view lists the passkey with its buttons disabled; the editor changes it.
  assert.equal(
    await instance.exec<boolean>(
      `return document.querySelector('[data-testid="passwords-passkey-rename-' + arguments[0] + '"]').disabled`,
      [passkey.id],
    ),
    true,
    'the view does not rename',
  )
  await inEditorExtra(ctx, instance, async () => {
    await instance.click(`passwords-passkey-rename-${passkey.id}`)
    await instance.type(
      `passwords-passkey-nickname-${passkey.id}`,
      `Laptop${KEY.enter}`,
    )
    await ctx.waitFor(
      'the nickname',
      async () =>
        (await detail(instance, original)).passkeys[0]?.nickname === 'Laptop',
    )
  })
  ctx.step('the passkey is renamed')

  // A copy with "Passkeys per Verweis".
  await instance.click('passwords-all')
  await contextMenu(instance, `passwords-entry-${original}`)
  await instance.click('passwords-menu-copy')
  await instance.click('passwords-ablage-paste')
  await instance.waitForDisplayed('passwords-copy-title')
  await instance.click('passwords-copy-passkey-links')
  await instance.click('passwords-copy-confirm')
  const copy = (await ctx.waitFor('the copy', async () => {
    const entry = (await overview(instance)).headers.find(
      (h) => h.id !== original && (h.title ?? '').startsWith('Beispiel'),
    )
    return entry?.id ?? false
  })) as string
  const linked = (await detail(instance, copy)).passkeys
  assert.equal(linked.length, 1)
  assert.equal(linked[0]?.linkedFrom?.itemId, original)
  assert.equal(linked[0]?.id, passkey.id, 'the same passkey, not a copy')

  await instance.click(`passwords-entry-${copy}`)
  await openExtra(ctx, instance)
  await instance.waitForDisplayed(`passwords-passkey-source-${passkey.id}`)
  const offers = await instance.exec<boolean[]>(
    `return ['rename', 'delete', 'unlink'].map((action) =>
       !!document.querySelector('[data-testid="passwords-passkey-' + action + '-' + arguments[0] + '"]'))`,
    [passkey.id],
  )
  assert.deepEqual(offers, [false, false, true], 'a link offers only unlink')
  ctx.step('the copy shows the passkey by a link')

  await inEditorExtra(ctx, instance, async () => {
    await instance.click(`passwords-passkey-unlink-${passkey.id}`)
    await ctx.waitFor(
      'the link to go',
      async () => (await detail(instance, copy)).passkeys.length === 0,
    )
  })
  assert.equal((await detail(instance, original)).passkeys.length, 1)
  ctx.step('"Verweis lösen" removes only the link')

  // Deleting the passkey asks and names the relying party.
  await instance.click('passwords-all')
  await instance.click(`passwords-entry-${original}`)
  await openExtra(ctx, instance)
  await inEditorExtra(ctx, instance, async () => {
    await instance.click(`passwords-passkey-delete-${passkey.id}`)
    await instance.waitForDisplayed('passwords-passkey-delete-confirm')
    const asked = await instance.exec<string>(
      `return document.querySelector('[role="alertdialog"]').textContent`,
    )
    assert.ok(asked.includes('Example'), asked)
    await instance.click('passwords-passkey-delete-confirm')
    await ctx.waitFor(
      'the passkey to go',
      async () => (await detail(instance, original)).passkeys.length === 0,
    )
  })
  ctx.step('deleting the passkey asks with the relying party named')
})
