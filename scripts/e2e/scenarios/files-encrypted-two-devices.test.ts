import assert from 'node:assert/strict'
import { unwrap } from '../lib/flows.ts'
import { deviceFiles, onDevice } from '../lib/platform/device-files.ts'
import { startRustfs } from '../lib/rustfs.ts'
import { scenario } from '../lib/scenario.ts'
import {
  list,
  names,
  openFiles,
  type Page,
  type Source,
  startImport,
  transfersDone,
} from '../lib/storage-files.ts'
import { connectStorage } from '../lib/storage-setup.ts'

// Spec 048, US3, quickstart §4 (T038, SC-007): an encrypted folder in a storage of RustFS on two
// devices of one vault. The laptop creates and fills it; the phone opens it without any input,
// adds a file, and the laptop sees it after a refresh. Both write `notiz.txt` without seeing the
// other's: both stay, one as "notiz (Konflikt …).txt". A second vault on the same bucket sees a
// neutral folder it cannot write into. The devices come from the group API of spec 033; in an
// Android run the phone is on the emulator and the others on Linux, so each device imports its
// file from where it can read it (`onDevice`). Needs Docker for RustFS; skipped without it.

const BUCKET = 'holzi-encrypted-two-e2e'
const SYNC_TIMEOUT_MS = 60_000
/** Loads the folder at `path` again, as the browser's refresh does. */
const refresh = async (page: Page, source: Source, path: string) =>
  unwrap(
    'files_encrypted_refresh',
    await page.invoke('files_encrypted_refresh', { source, path }),
  )

scenario(
  'files-encrypted-two-devices',
  { timeoutMs: 480_000, needs: { container: true } },
  async (ctx) => {
    const rustfs = await startRustfs([BUCKET])
    ctx.onTeardown(() => rustfs.stop())
    ctx.step('RustFS')

    const group = await ctx.group({
      users: { anna: ['laptop', 'phone'], ben: ['desktop'] },
    })
    const laptop = group.device('anna/laptop').page
    const phone = group.device('anna/phone').page
    const other = group.device('ben/desktop').page

    // Host files; `onDevice` hands each device a path it can read.
    const hostFiles = deviceFiles('holzi-encrypted-two-host-', {
      E2E_PLATFORM: 'linux',
    })
    ctx.onTeardown(() => hostFiles.remove())
    hostFiles.write('brief.txt', 'Brief vom Laptop')
    hostFiles.write('antwort.txt', 'Antwort vom Telefon')
    const brief = hostFiles.path('brief.txt')
    const antwort = hostFiles.path('antwort.txt')
    const laptopFiles = deviceFiles('holzi-encrypted-two-laptop-', {
      E2E_PLATFORM: 'linux',
    })
    const phoneFiles = deviceFiles('holzi-encrypted-two-phone-', {
      E2E_PLATFORM: 'linux',
    })
    ctx.onTeardown(() => laptopFiles.remove())
    ctx.onTeardown(() => phoneFiles.remove())
    laptopFiles.write('notiz.txt', 'vom Laptop')
    phoneFiles.write('notiz.txt', 'vom Telefon, etwas länger')

    const storageId = await connectStorage(laptop, rustfs, BUCKET, 'Fotos')
    const source: Source = { kind: 'storage', storageId }
    await ctx.waitFor(
      'the storage and its credentials to reach the phone',
      async () => {
        const there = unwrap<{
          connections: Array<{ credentials: string }>
          storages: Array<{ id: string }>
        }>('storage_list', await phone.invoke('storage_list'))
        return (
          there.storages.some((s) => s.id === storageId) &&
          there.connections[0]?.credentials === 'present'
        )
      },
      { timeoutMs: SYNC_TIMEOUT_MS, fixed: true },
    )
    await openFiles(laptop, storageId)
    await openFiles(phone, storageId)
    ctx.step('the storage on both devices')

    unwrap(
      'files_create_encrypted_folder',
      await laptop.invoke('files_create_encrypted_folder', {
        source,
        path: '/',
        name: 'Unterlagen',
      }),
    )
    await startImport(laptop, [onDevice(brief, laptop)], source, '/Unterlagen')
    await transfersDone(ctx, laptop, 'brief.txt to go in on the laptop')
    ctx.step('the laptop created and filled the folder')

    // The phone opens it without any input once the vault's content key reached it.
    await ctx.waitFor(
      'the phone to read the folder',
      async () =>
        (await list(phone, source, '/')).some(
          (entry) =>
            entry.name === 'Unterlagen' &&
            entry.encrypted?.state === 'readable',
        ),
      { timeoutMs: SYNC_TIMEOUT_MS, fixed: true },
    )
    assert.deepEqual(await names(phone, source, '/Unterlagen'), ['brief.txt'])
    const text = unwrap<{ text: string }>(
      'files_read_text',
      await phone.invoke('files_read_text', {
        source,
        path: '/Unterlagen/brief.txt',
      }),
    )
    assert.equal(text.text, 'Brief vom Laptop')
    ctx.step('the phone opens the folder and the file without input')

    await startImport(phone, [onDevice(antwort, phone)], source, '/Unterlagen')
    await transfersDone(ctx, phone, 'antwort.txt to go in on the phone')
    await refresh(laptop, source, '/Unterlagen')
    assert.deepEqual(await names(laptop, source, '/Unterlagen'), [
      'antwort.txt',
      'brief.txt',
    ])
    ctx.step('a file the phone adds shows on the laptop after a refresh')

    // Both write notiz.txt, neither seeing the other's: both stay, one as a conflict copy.
    const fromLaptop = laptopFiles.path('notiz.txt')
    const fromPhone = phoneFiles.path('notiz.txt')
    await Promise.all([
      startImport(
        laptop,
        [onDevice(fromLaptop, laptop)],
        source,
        '/Unterlagen',
      ),
      startImport(phone, [onDevice(fromPhone, phone)], source, '/Unterlagen'),
    ])
    await transfersDone(ctx, laptop, 'notiz.txt to go in on the laptop')
    await transfersDone(ctx, phone, 'notiz.txt to go in on the phone')
    for (const [name, page] of [
      ['laptop', laptop],
      ['phone', phone],
    ] as const) {
      await refresh(page, source, '/Unterlagen')
      const shown = await names(page, source, '/Unterlagen')
      const notes = shown.filter((entry) => entry.startsWith('notiz'))
      assert.equal(notes.length, 2, `${name}: ${shown.join(', ')}`)
      assert.ok(notes.includes('notiz.txt'), `${name}: ${shown.join(', ')}`)
      assert.ok(
        notes.some((entry) => /^notiz \(Konflikt .+\)\.txt$/.test(entry)),
        `${name}: ${shown.join(', ')}`,
      )
    }
    ctx.step('both versions of notiz.txt stay, one as a conflict copy')

    // Another vault on the same bucket: a neutral folder it can neither open nor write into.
    const otherId = await connectStorage(other, rustfs, BUCKET, 'Fremd')
    const otherSource: Source = { kind: 'storage', storageId: otherId }
    const seen = await list(other, otherSource, '/')
    assert.equal(seen.length, 1, JSON.stringify(seen))
    assert.equal(seen[0]?.encrypted?.state, 'otherVault')
    assert.doesNotMatch(seen[0]?.name ?? '', /Unterlagen/)
    const refused = await other.invoke('files_create_folder', {
      source: otherSource,
      path: seen[0]?.path,
      name: 'Neu',
    })
    assert.ok(
      'error' in refused,
      `a write into another vault is refused: ${JSON.stringify(refused)}`,
    )
    assert.match(JSON.stringify(refused.error), /otherVault/)
    await openFiles(other, otherId)
    await ctx.waitFor(
      'the neutral name in the browser',
      () =>
        other.exec<boolean>(
          `const list = document.querySelector('[data-testid="files-entries"]')
           return !!list && /anderen Vault|another vault/.test(list.textContent)`,
        ),
      { timeoutMs: 15_000 },
    )
    ctx.step('another vault sees a neutral folder and cannot write')
  },
)
