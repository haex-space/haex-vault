import { deviceFiles } from '../lib/extension-files.ts'
import { createAndUnlock, unwrap } from '../lib/flows.ts'
import { scenario } from '../lib/scenario.ts'
import { runAction } from '../lib/settings.ts'

// Spec 061, US1 and US2, quickstart §1 and §2 (T034, T043): a one-way and a two-way rule between
// folders of the device. The rules are created through the command, since the roots of the folder
// picker differ per platform. The overview shows them; "Jetzt abgleichen" copies the files and
// leaves a file of the target alone; the two-way rule brings the new files of each side to the
// other, and a file changed on both sides ends with the newer version under its name and the older
// one as a conflict copy on both sides. The create dialog offers both folder pickers. Deletions,
// moves and the other rows of the plan are covered by the Rust runs against real folders
// (`file_sync::run`).

const sleep = (ms: number) =>
  new Promise<void>((resolve) => setTimeout(resolve, ms))

const draft = (name: string, a: string, b: string, direction: string) => ({
  name,
  placeA: { source: { kind: 'device' }, path: a },
  placeB: { source: { kind: 'device' }, path: b },
  direction,
  deleteMode: direction === 'both' ? 'mirror' : 'keep',
  intervalSecs: 900,
  enabled: true,
  allowDecrypted: false,
})

scenario('files-sync-rules', { timeoutMs: 180_000 }, async (ctx) => {
  const source = deviceFiles('holzi-sync-a-')
  const target = deviceFiles('holzi-sync-b-')
  const left = deviceFiles('holzi-sync-left-')
  const right = deviceFiles('holzi-sync-right-')
  try {
    const instance = await ctx.startInstance()
    await createAndUnlock(instance, { name: 'e2e-files-sync-rules' })

    // After the start: on Android it clears the app's data, where the folders live.
    source.write('notiz.txt', 'Hallo')
    source.write('Fotos/liste.txt', 'Eins')
    target.write('platzhalter.txt', 'bleibt')
    left.write('links.txt', 'L')
    left.write('x.txt', 'eins')
    right.write('Ordner/rechts.txt', 'R')

    unwrap(
      'sync_rules_create',
      await instance.invoke('sync_rules_create', {
        draft: draft('Sicherung', source.folder, target.folder, 'aToB'),
      }),
    )
    const twoWay = unwrap<{ rule: { id: string } }>(
      'sync_rules_create',
      await instance.invoke('sync_rules_create', {
        draft: draft('Abgleich', left.folder, right.folder, 'both'),
      }),
    )

    await runAction(instance, 'wm.app.open', {
      appId: 'system.files',
      at: `/device?p=${encodeURIComponent(source.folder)}`,
    })
    await instance.click('files-sync-rules-open')
    await instance.waitForDisplayed('files-sync-rule-Sicherung')

    await instance.click('files-sync-rule-run-Sicherung')
    await ctx.waitFor(
      'the files of the source at the target, beside its own',
      () =>
        target.files().sort().join(',') ===
        'Fotos/liste.txt,notiz.txt,platzhalter.txt',
      { timeoutMs: 30_000 },
    )
    if (target.read('notiz.txt') !== 'Hallo') {
      throw new Error('the copy differs from the source')
    }

    // Two-way: each side gets the new files of the other.
    const both = 'Ordner/rechts.txt,links.txt,x.txt'
    await instance.click('files-sync-rule-run-Abgleich')
    await ctx.waitFor(
      'the new files of each side on the other',
      () =>
        left.files().sort().join(',') === both &&
        right.files().sort().join(',') === both,
      { timeoutMs: 30_000 },
    )

    // Changed on both sides: the newer keeps the name, the older is a conflict copy on both.
    // Pause the watched rule while staging both edits. adb writes and permission updates on
    // Android can take longer than the watcher's quiet window, which would otherwise sync the
    // first edit before the second side has changed.
    unwrap(
      'sync_rules_set_enabled',
      await instance.invoke('sync_rules_set_enabled', {
        id: twoWay.rule.id,
        enabled: false,
      }),
    )
    left.write('x.txt', 'links geändert')
    await sleep(1500)
    right.write('x.txt', 'rechts geändert!')
    const settled = (side: typeof left) =>
      side.files().length === 4 &&
      side.files().some((name) => name.startsWith('x (Konflikt ')) &&
      side.read('x.txt') === 'rechts geändert!'
    unwrap(
      'sync_rules_set_enabled',
      await instance.invoke('sync_rules_set_enabled', {
        id: twoWay.rule.id,
        enabled: true,
      }),
    )
    await ctx
      .waitFor(
        'the conflict copy on both sides',
        () => settled(left) && settled(right),
        { timeoutMs: 30_000 },
      )
      .catch(async (error: Error) => {
        const result = await instance.invoke('sync_rules_list')
        error.message += `; left: ${JSON.stringify(left.files().map((name) => [name, left.read(name)]))}, right: ${JSON.stringify(right.files().map((name) => [name, right.read(name)]))}, rules: ${JSON.stringify(result)}`
        throw error
      })
    const aside = left.files().find((name) => name.startsWith('x (Konflikt '))
    if (!aside || left.read(aside) !== 'links geändert') {
      throw new Error('the older version is not the conflict copy')
    }

    // The dialog offers both pickers.
    await instance.click('files-sync-rules-new')
    await instance.waitForDisplayed('files-picker-a-root')
    await instance.waitForDisplayed('files-picker-b-root')
  } finally {
    source.remove()
    target.remove()
    left.remove()
    right.remove()
  }
})
