import assert from 'node:assert/strict'

import { deviceFiles } from '../lib/extension-files.ts'
import { unwrap, type FlowInstance } from '../lib/flows.ts'
import { scenario } from '../lib/scenario.ts'
import { zipOf } from '../lib/zip.ts'

// Spec 053 US4, quickstart §3: the sets and the choice are the same on every device of a vault. A
// set imported and chosen on the laptop shows on the phone after the sync; two devices that choose at
// the same time end with one choice; removing the set on the laptop removes it on the phone and
// the choice falls back everywhere.

type Selection = {
  set: { kind: string; name?: string; id?: string }
  state: string
}
type Info = { ref: { kind: string; id?: string }; name: string; state: string }

const selectionOf = async (page: FlowInstance) =>
  unwrap<Selection>('selection', await page.invoke('file_icon_set_selection'))

const listOf = async (page: FlowInstance) =>
  unwrap<Info[]>('list', await page.invoke('file_icon_sets_list'))

const choose = async (page: FlowInstance, value: string) =>
  unwrap(
    'set_pref',
    await page.invoke('set_pref', {
      args: { scope: { kind: 'vault' }, key: 'files.iconSet', value },
    }),
  )

const valueOf = (selection: Selection) =>
  selection.set.kind === 'builtin'
    ? `builtin:${selection.set.name}`
    : `custom:${selection.set.id}`

scenario('files-icon-sets-two-devices', { timeoutMs: 420_000 }, async (ctx) => {
  const files = deviceFiles('holzi-icons-sync-')
  try {
    const group = await ctx.group({ users: { anna: ['laptop', 'phone'] } })
    const laptop = group.device('anna/laptop').page
    const phone = group.device('anna/phone').page

    files.write(
      'shared-set.zip',
      zipOf([
        {
          name: 'icon-theme.json',
          data: JSON.stringify({
            iconDefinitions: { file: { iconPath: './file.svg' } },
            file: 'file',
          }),
        },
        {
          name: 'file.svg',
          data: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><path d="M2 2h12v12H2z" fill="#0a0"/></svg>',
        },
      ]),
    )
    const added = unwrap<Info>(
      'import',
      await laptop.invoke('file_icon_set_import', {
        source: { kind: 'file', file: files.path('shared-set.zip') },
        replace: null,
      }),
    )
    const id = added.ref.id ?? ''
    await choose(laptop, `custom:${id}`)

    // The phone shows the set after the sync; until the row is there it shows the fallback.
    await ctx.waitFor(
      'the set to reach the phone',
      async () => {
        const selection = await selectionOf(phone)
        return (
          selection.state === 'ready' && valueOf(selection) === `custom:${id}`
        )
      },
      { timeoutMs: 60_000, fixed: true },
    )
    assert.ok(
      (await listOf(phone)).some(
        (info) => info.ref.id === id && info.state === 'ready',
      ),
      'the set is in the phone list, ready',
    )
    ctx.step('an imported and chosen set shows on the other device')

    // Both devices choose at the same time and end with the same choice.
    await Promise.all([
      choose(laptop, 'builtin:phosphor'),
      choose(phone, 'builtin:lucide'),
    ])
    await ctx.waitFor(
      'both devices to hold the same choice',
      async () => {
        const [a, b] = await Promise.all([
          selectionOf(laptop),
          selectionOf(phone),
        ])
        return valueOf(a) === valueOf(b)
      },
      { timeoutMs: 60_000, fixed: true },
    )
    ctx.step('a concurrent choice ends in one choice on both devices')

    // Removing the set on the laptop removes it on the phone; a choice on it falls back.
    await choose(laptop, `custom:${id}`)
    await ctx.waitFor(
      'the phone to choose the set again',
      async () => valueOf(await selectionOf(phone)) === `custom:${id}`,
      { timeoutMs: 60_000, fixed: true },
    )
    unwrap('remove', await laptop.invoke('file_icon_set_remove', { id }))
    await ctx.waitFor(
      'the removal to reach the phone',
      async () => {
        const [list, selection] = await Promise.all([
          listOf(phone),
          selectionOf(phone),
        ])
        return (
          !list.some((info) => info.ref.id === id) &&
          selection.set.kind === 'builtin' &&
          selection.state === 'ready'
        )
      },
      { timeoutMs: 60_000, fixed: true },
    )
    ctx.step(
      'removing a set removes it on the other device and the choice falls back',
    )
  } finally {
    files.remove()
  }
})
