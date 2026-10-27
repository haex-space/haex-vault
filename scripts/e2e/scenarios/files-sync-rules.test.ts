import { deviceFiles } from '../lib/extension-files.ts'
import { createAndUnlock, unwrap } from '../lib/flows.ts'
import { scenario } from '../lib/scenario.ts'
import { runAction } from '../lib/settings.ts'

// Spec 061, US1, quickstart §1 (T034): a one-way rule between two folders of the device. The rule is
// created through the command, since the roots of the folder picker differ per platform; the
// overview shows it, "Jetzt abgleichen" copies the files and leaves a file of the target alone, and
// the create dialog offers both folder pickers. Deletions and the other rows of the plan are covered
// by the Rust runs against real folders (`file_sync::run`).

scenario('files-sync-rules', { timeoutMs: 180_000 }, async (ctx) => {
  const source = deviceFiles('holzi-sync-a-')
  const target = deviceFiles('holzi-sync-b-')
  try {
    const instance = await ctx.startInstance()
    await createAndUnlock(instance, { name: 'e2e-files-sync-rules' })

    // After the start: on Android it clears the app's data, where the folders live.
    source.write('notiz.txt', 'Hallo')
    source.write('Fotos/liste.txt', 'Eins')
    target.write('platzhalter.txt', 'bleibt')

    unwrap(
      'sync_rules_create',
      await instance.invoke('sync_rules_create', {
        draft: {
          name: 'Sicherung',
          placeA: { source: { kind: 'device' }, path: source.folder },
          placeB: { source: { kind: 'device' }, path: target.folder },
          direction: 'aToB',
          deleteMode: 'keep',
          intervalSecs: 900,
          enabled: true,
          allowDecrypted: false,
        },
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

    // The dialog offers both pickers.
    await instance.click('files-sync-rules-new')
    await instance.waitForDisplayed('files-picker-a-root')
    await instance.waitForDisplayed('files-picker-b-root')
  } finally {
    source.remove()
    target.remove()
  }
})
