import { deviceFiles } from '../lib/extension-files.ts'
import { createAndUnlock, unwrap } from '../lib/flows.ts'
import { scenario } from '../lib/scenario.ts'
import { runAction } from '../lib/settings.ts'

// Spec 074, quickstart: a rule with one file as its source. The rule is created through the command, since
// the roots of the folder picker differ per platform and a temporary folder is not among its places. The
// first run copies the file and leaves its neighbour alone; a changed file is copied again; a renamed
// source makes the rule's source invalid, with the copy untouched; the file back at its path ends that on
// the next run; a deleted copy is copied again. What a rule shows is read from the list the window reads.

const FILE = 'Haushalt.ods'

type View = { rule: { name: string }; status: { state: string } }

scenario('files-sync-file-rule', { timeoutMs: 180_000 }, async (ctx) => {
  const source = deviceFiles('holzi-file-rule-a-')
  const target = deviceFiles('holzi-file-rule-b-')
  try {
    const instance = await ctx.startInstance()
    await createAndUnlock(instance, { name: 'e2e-files-file-rule' })

    // After the start: on Android it clears the app's data, where the folders live.
    source.write(FILE, 'eins')
    source.write('anderes.txt', 'bleibt draußen')
    target.ensureFolder()

    unwrap(
      'sync_rules_create',
      await instance.invoke('sync_rules_create', {
        draft: {
          name: 'Haushalt',
          source: { location: { kind: 'device' }, path: source.path(FILE) },
          target: { location: { kind: 'device' }, path: target.folder },
          sourceKind: 'file',
          direction: 'sourceToTarget',
          deleteMode: 'keep',
          intervalSecs: 900,
          enabled: true,
          allowDecrypted: false,
        },
      }),
    )
    const stateOf = async () => {
      const views = unwrap<View[]>(
        'sync_rules_list',
        await instance.invoke('sync_rules_list'),
      )
      return views.find((view) => view.rule.name === 'Haushalt')?.status.state
    }

    await runAction(instance, 'wm.app.open', {
      appId: 'system.files',
      at: `/device?p=${encodeURIComponent(source.folder)}`,
    })
    await instance.click('files-sync-rules-open')
    await instance.waitForDisplayed('files-sync-rule-Haushalt')

    // The first run: the file, and nothing of its neighbours.
    await instance.click('files-sync-rule-run-Haushalt')
    await ctx.waitFor(
      'the file at the target, its neighbour not',
      () => target.files().join(',') === FILE,
      { timeoutMs: 30_000 },
    )
    if (target.read(FILE) !== 'eins') {
      throw new Error('the copy differs from the source')
    }

    // A changed file is copied again.
    source.write(FILE, 'zwei und mehr')
    await instance.click('files-sync-rule-run-Haushalt')
    await ctx.waitFor(
      'the changed file at the target',
      () => target.read(FILE) === 'zwei und mehr',
      { timeoutMs: 30_000 },
    )

    // A renamed source: the rule stays, shows an invalid source, and the copy is untouched.
    source.delete(FILE)
    source.write('Haushalt-2026.ods', 'zwei und mehr')
    await instance.click('files-sync-rule-run-Haushalt')
    await ctx.waitFor(
      'the rule shows an invalid source',
      async () => (await stateOf()) === 'invalidSource',
      { timeoutMs: 30_000 },
    )
    if (
      target.files().join(',') !== FILE ||
      target.read(FILE) !== 'zwei und mehr'
    ) {
      throw new Error('an invalid source changed the target')
    }

    // The file back at its path: the next run continues, without a step by the user.
    source.delete('Haushalt-2026.ods')
    source.write(FILE, 'wieder da, und anders')
    await instance.click('files-sync-rule-run-Haushalt')
    await ctx.waitFor(
      'the rule valid again, with the new version at the target',
      async () =>
        (await stateOf()) !== 'invalidSource' &&
        target.read(FILE) === 'wieder da, und anders',
      { timeoutMs: 30_000 },
    )

    // A deleted copy is copied again from the source file.
    target.delete(FILE)
    await instance.click('files-sync-rule-run-Haushalt')
    await ctx.waitFor(
      'the deleted copy back',
      () => target.files().join(',') === FILE,
      { timeoutMs: 30_000 },
    )
    if (source.read(FILE) !== 'wieder da, und anders') {
      throw new Error('the source file changed')
    }
  } finally {
    source.remove()
    target.remove()
  }
})
