import assert from 'node:assert/strict'
import { unwrap } from '../lib/flows.ts'
import { acceptCard, ownCardText } from '../lib/peers.ts'
import { deviceFiles } from '../lib/platform/device-files.ts'
import type { InvokeResult } from '../lib/platform/host.ts'
import { scenario } from '../lib/scenario.ts'
import { runAction } from '../lib/settings.ts'
import { names, type Page } from '../lib/storage-files.ts'

// Spec 056 (quickstart §1–§4, T075): Anna's phone opens the files of her laptop as another own device
// and copies a file from there; Anna shares a folder of the laptop with Ben, who finds it by asking
// her devices, reads it and cannot write; once Anna takes his access away, his next request is
// refused. The devices meet through the scene's iroh relay. In an Android run the phone is the
// emulator and the laptop and Ben's desktop run on Linux, so the laptop's files are host files.

const SYNC_TIMEOUT_MS = 90_000

type PeerDevice = { id: string; name: string | null; state: string }
type ReceivedShare = { storageId: string; name: string; level: string }
type ContactShares = {
  contactId: string
  contactName: string
  vaultPubkey: string
  shares: ReceivedShare[]
}
type Sources = { devices: PeerDevice[]; contacts: ContactShares[] }
type Share = { id: string }

/** The command answered, and with a failure. */
const refused = (result: InvokeResult) => !('ended' in result) && !result.ok

const sources = async (page: Page) =>
  unwrap<Sources>('files_sources', await page.invoke('files_sources'))

const storage = (storageId: string) => ({ kind: 'storage', storageId }) as const

const readText = async (
  page: Page,
  source: { kind: 'device' } | { kind: 'storage'; storageId: string },
  path: string,
) =>
  unwrap<{ text: string }>(
    'files_read_text',
    await page.invoke('files_read_text', { source, path }),
  ).text

const TRANSFERS = `document.querySelector('#__nuxt').__vue_app__.config.globalProperties.$pinia._s.get('filesTransfers')`

scenario('file-shares', { timeoutMs: 600_000 }, async (ctx) => {
  const group = await ctx.group({
    users: { anna: ['laptop', 'phone'], ben: ['desktop'] },
    irohRelay: true,
  })
  const laptop = group.device('anna/laptop').page
  const phone = group.device('anna/phone').page
  const ben = group.device('ben/desktop').page

  // The laptop always runs on Linux: its files are host files.
  const laptopFiles = deviceFiles('holzi-file-shares-laptop-', {
    E2E_PLATFORM: 'linux',
  })
  ctx.onTeardown(() => laptopFiles.remove())
  laptopFiles.write('notiz.txt', 'vom Laptop')
  const phoneFiles = deviceFiles('holzi-file-shares-phone-')
  ctx.onTeardown(() => phoneFiles.remove())
  ctx.step('files on the laptop')

  // §1: the laptop is another own device of the phone.
  let laptopId = ''
  await ctx.waitFor(
    'the laptop to be reachable from the phone',
    async () => {
      const found = (await sources(phone)).devices.find(
        (device) => device.state === 'reachable',
      )
      laptopId = found?.id ?? ''
      return found !== undefined
    },
    { timeoutMs: SYNC_TIMEOUT_MS },
  )
  const onLaptop = storage(laptopId)
  assert.ok(
    (await names(phone, onLaptop, laptopFiles.folder)).includes('notiz.txt'),
  )
  assert.equal(
    await readText(phone, onLaptop, `${laptopFiles.folder}/notiz.txt`),
    'vom Laptop',
  )
  ctx.step('the phone reads the laptop')

  // §1: copy from the laptop onto the phone, through the file browser's transfers.
  await runAction(phone, 'wm.app.open', {
    appId: 'system.files',
    at: `/device?p=${encodeURIComponent(phoneFiles.folder)}`,
  })
  // The store of the transfers exists once the file browser is mounted.
  await ctx.waitFor(
    'the file browser to hold its transfers',
    async () => await phone.exec<boolean>(`return !!${TRANSFERS}`),
  )
  await phone.exec(
    `return ${TRANSFERS}.startAsync('copy', arguments[0], arguments[1], arguments[2], 'e2e').then(() => true)`,
    [
      onLaptop,
      [`${laptopFiles.folder}/notiz.txt`],
      { source: { kind: 'device' }, path: phoneFiles.folder },
    ],
  )
  await ctx.waitFor(
    'the copy to reach the phone',
    async () => {
      try {
        return phoneFiles.read('notiz.txt') === 'vom Laptop'
      } catch {
        return false
      }
    },
    { timeoutMs: SYNC_TIMEOUT_MS },
  )
  ctx.step('copied onto the phone')

  // §3: cards both ways, then a share of a laptop folder for Ben.
  await acceptCard(ben, await ownCardText(laptop, 'Anna'))
  await acceptCard(laptop, await ownCardText(ben, 'Ben'))
  const shared = deviceFiles('holzi-file-shares-shared-', {
    E2E_PLATFORM: 'linux',
  })
  ctx.onTeardown(() => shared.remove())
  shared.write('rezept.txt', 'Hefezopf')
  const benOnLaptop = (await sources(laptop)).contacts.find(
    (contact) => contact.contactName === 'Ben',
  )
  assert.ok(benOnLaptop, 'Ben is a contact with a card on the laptop')
  const share = unwrap<Share>(
    'file_share_save',
    await laptop.invoke('file_share_save', {
      args: {
        source: { kind: 'device' },
        path: shared.folder,
        name: 'Rezepte',
        grants: [{ vaultPubkey: benOnLaptop.vaultPubkey, level: 'read' }],
      },
    }),
  )
  ctx.step('shared with Ben')

  const annaOnBen = (await sources(ben)).contacts.find(
    (contact) => contact.contactName === 'Anna',
  )
  assert.ok(annaOnBen, 'Anna is a contact with a card on Ben')
  let shareId = ''
  await ctx.waitFor(
    "Ben to find Anna's share",
    async () => {
      const answer = unwrap<ContactShares>(
        'files_contact_query',
        await ben.invoke('files_contact_query', {
          contactId: annaOnBen.contactId,
        }),
      )
      shareId = answer.shares.find((s) => s.name === 'Rezepte')?.storageId ?? ''
      return shareId !== ''
    },
    { timeoutMs: SYNC_TIMEOUT_MS },
  )
  const rezepte = storage(shareId)
  assert.deepEqual(await names(ben, rezepte, '/'), ['rezept.txt'])
  assert.equal(await readText(ben, rezepte, '/rezept.txt'), 'Hefezopf')
  const write = await ben.invoke('files_create_folder', {
    source: rezepte,
    path: '/',
    name: 'Neu',
  })
  assert.ok(refused(write), 'a read share takes nothing')
  ctx.step('Ben reads the share and cannot write')

  // §4: taking the access away refuses Ben's next request.
  unwrap(
    'file_share_revoke',
    await laptop.invoke('file_share_revoke', {
      shareId: share.id,
      vaultPubkey: benOnLaptop.vaultPubkey,
    }),
  )
  await ctx.waitFor(
    "Ben's next request to be refused",
    async () =>
      refused(await ben.invoke('files_list', { source: rezepte, path: '/' })),
    { timeoutMs: SYNC_TIMEOUT_MS },
  )
  ctx.step('revoked')
})
