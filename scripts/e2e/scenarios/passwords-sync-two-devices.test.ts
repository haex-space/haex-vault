import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import {
  createEntry,
  expectEntries,
  overview,
  restoreEntry,
  tagCount,
  trashEntry,
} from '../lib/passwords.ts'
import { unwrap } from '../lib/flows.ts'

// Spec 034, US8 (quickstart §10): two devices of one user keep the password manager's data in step. An
// entry made on one shows on the other; a tag made on both under one name stays one tag; an entry
// with tags and a custom field that is deleted for good on one is gone on the other. The devices
// come from the group API of spec 033 and find each other through the scenario's Nostr relay. The
// attachments of 25 MiB and the equal files are covered by `tests/passwords_sync.rs` (a scenario
// cannot write a file to a device).
scenario('passwords-sync-two-devices', { timeoutMs: 300_000 }, async (ctx) => {
  const group = await ctx.group({ users: { anna: ['laptop', 'phone'] } })
  const laptop = group.device('anna/laptop').page
  const phone = group.device('anna/phone').page

  const mail = await createEntry(laptop, {
    title: 'Mail',
    username: 'anna',
    tags: ['shared'],
    keyValues: [{ key: 'PIN', value: 'SECRET-MARKER-E2E-SYNC' }],
  })
  await expectEntries(ctx, phone, ['Mail'], 'the entry to show on the phone')
  ctx.step('created on one device, shown on the other')

  // A tag made on both devices under one name is one tag on both.
  await createEntry(laptop, { title: 'On laptop', tags: ['Work'] })
  await createEntry(phone, { title: 'On phone', tags: ['work'] })
  for (const [name, device] of [
    ['laptop', laptop],
    ['phone', phone],
  ] as const) {
    await ctx.waitFor(
      `the ${name} to hold three entries and one Work tag`,
      async () =>
        (await tagCount(device, 'work')) === 1 &&
        (
          unwrap<{ headers: unknown[] }>(
            'passwords_load_overview',
            await device.invoke('passwords_load_overview'),
          ) as { headers: unknown[] }
        ).headers.length === 3,
      { timeoutMs: 40_000, fixed: true },
    )
  }
  ctx.step('a tag made twice is one tag')

  // Trash on one device shows on the other, a restore too, and a delete for good removes the entry.
  await trashEntry(laptop, mail)
  await ctx.waitFor(
    'the trash to reach the phone',
    async () => {
      const overview = unwrap<{
        headers: Array<{ id: string; groupId: string | null }>
      }>(
        'passwords_load_overview',
        await phone.invoke('passwords_load_overview'),
      )
      return overview.headers.find((h) => h.id === mail)?.groupId === 'trash'
    },
    { timeoutMs: 40_000, fixed: true },
  )
  await restoreEntry(laptop, mail)
  await ctx.waitFor(
    'the restore to reach the phone',
    async () => {
      const overview = unwrap<{
        headers: Array<{ id: string; groupId: string | null }>
      }>(
        'passwords_load_overview',
        await phone.invoke('passwords_load_overview'),
      )
      return overview.headers.find((h) => h.id === mail)?.groupId !== 'trash'
    },
    { timeoutMs: 40_000, fixed: true },
  )
  await trashEntry(laptop, mail)
  unwrap(
    'passwords_delete_permanently',
    await laptop.invoke('passwords_delete_permanently', {
      args: { targets: [{ kind: 'item', id: mail }] },
    }),
  )
  await expectEntries(
    ctx,
    phone,
    ['On laptop', 'On phone'],
    'the delete for good to reach the phone',
  )
  assert.equal(await tagCount(phone, 'shared'), 1, 'the tag outlives its entry')
  ctx.step('trash, restore and delete for good arrive')

  // Spec 100 (FR-008, SC-003): a tag merged on one device is merged on the other, every entry keeps
  // its tag and none carries it twice.
  const jobOnly = await createEntry(laptop, {
    title: 'Job only',
    tags: ['Job'],
  })
  const jobAndWork = await createEntry(laptop, {
    title: 'Job and Work',
    tags: ['Job', 'Work'],
  })
  await expectEntries(
    ctx,
    phone,
    ['Job and Work', 'Job only', 'On laptop', 'On phone'],
    'the entries with the tag Job to reach the phone',
  )
  await ctx.waitFor(
    'the tag Job to reach the phone',
    async () => (await tagCount(phone, 'job')) === 1,
    { timeoutMs: 40_000, fixed: true },
  )
  const tagId = async (device: typeof laptop, name: string) => {
    // The stored spelling is the one of the device that made the tag first ("Work" or "work").
    const tag = (await overview(device)).tags.find(
      (t) => t.name.toLowerCase() === name.toLowerCase(),
    )
    assert.ok(tag, `the device holds no tag named "${name}"`)
    return tag.id
  }
  unwrap(
    'passwords_merge_tags',
    await laptop.invoke('passwords_merge_tags', {
      args: {
        sourceIds: [await tagId(laptop, 'Job')],
        targetIds: [await tagId(laptop, 'Work')],
      },
    }),
  )
  await ctx.waitFor(
    'the merge to reach the phone: no Job, every entry carries Work once',
    async () => {
      if ((await tagCount(phone, 'job')) !== 0) return false
      const { headers } = await overview(phone)
      return [jobOnly, jobAndWork].every((id) => {
        const names = headers.find((h) => h.id === id)?.tags.map((t) => t.name)
        return names?.length === 1 && names[0]?.toLowerCase() === 'work'
      })
    },
    { timeoutMs: 40_000, fixed: true },
  )
  ctx.step('a merged tag is merged on the other device')
})
