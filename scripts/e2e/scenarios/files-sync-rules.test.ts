import { deviceFiles } from '../lib/extension-files.ts'
import { createAndUnlock, unwrap } from '../lib/flows.ts'
import { scenario } from '../lib/scenario.ts'
import { KEY, runAction } from '../lib/settings.ts'

// Spec 061, US1 to US4, quickstart §1 to §4 (T034, T043, T051, T060): a one-way, a two-way and an
// asking rule between folders of the device. The rules are created through the command, since the roots of the folder
// picker differ per platform. The overview shows them; "Jetzt abgleichen" copies the files and
// leaves a file of the target alone; the two-way rule brings the new files of each side to the
// other, and a file changed on both sides ends with the newer version under its name and the older
// one as a conflict copy on both sides. A deletion of the asking rule waits in the overview until it
// is confirmed; a place that is gone makes its rule wait and deletes nothing. An exclusion written
// in the editor keeps a file out, the preview counts it, the log lists the runs and opens one, and
// the log depth set in the settings drops the older runs. The create dialog offers both folder
// pickers. Moves and the other rows of the plan are covered by the Rust runs
// against real folders (`file_sync::run`).

const sleep = (ms: number) =>
  new Promise<void>((resolve) => setTimeout(resolve, ms))

const draft = (
  name: string,
  a: string,
  b: string,
  direction: string,
  deleteMode = direction === 'both' ? 'mirror' : 'keep',
) => ({
  name,
  placeA: { source: { kind: 'device' }, path: a },
  placeB: { source: { kind: 'device' }, path: b },
  direction,
  deleteMode,
  intervalSecs: 900,
  enabled: true,
  allowDecrypted: false,
})

scenario('files-sync-rules', { timeoutMs: 180_000 }, async (ctx) => {
  const source = deviceFiles('holzi-sync-a-')
  const target = deviceFiles('holzi-sync-b-')
  const left = deviceFiles('holzi-sync-left-')
  const right = deviceFiles('holzi-sync-right-')
  const askFrom = deviceFiles('holzi-sync-ask-a-')
  const askTo = deviceFiles('holzi-sync-ask-b-')
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
    askFrom.write('weg.txt', 'W')
    askFrom.write('bleibt.txt', 'B')

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

    unwrap(
      'sync_rules_create',
      await instance.invoke('sync_rules_create', {
        draft: draft('Nachfrage', askFrom.folder, askTo.folder, 'aToB', 'ask'),
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

    // Delete mode "ask": the deletion waits in the overview and happens once it is confirmed.
    await instance.click('files-sync-rule-run-Nachfrage')
    await ctx.waitFor(
      'the files of the asking rule at its target',
      () => askTo.files().sort().join(',') === 'bleibt.txt,weg.txt',
      { timeoutMs: 30_000 },
    )
    askFrom.delete('weg.txt')
    // The waiting deletions are in the details of the rule's row.
    await instance.click('files-sync-rule-open-Nachfrage')
    await instance.click('files-sync-rule-run-Nachfrage')
    await instance.click('files-sync-rule-open-Nachfrage')
    await instance.waitForDisplayed('files-sync-rule-pending-Nachfrage', 30_000)
    if (!askTo.files().includes('weg.txt')) {
      throw new Error('the asking rule deleted before it was confirmed')
    }
    await instance.click('files-sync-rule-pending-Nachfrage')
    await instance.click('files-sync-rule-pending-delete-weg.txt')
    // Into the trash: the system's, or the rule trash below the place.
    await ctx.waitFor(
      'the confirmed deletion done',
      () => !askTo.files().includes('weg.txt'),
      { timeoutMs: 30_000 },
    )
    if (!askTo.files().includes('bleibt.txt')) {
      throw new Error('the confirmation deleted another file')
    }

    // A place that is gone (FR-022): the mirroring rule waits and deletes nothing on the other side.
    const before = right.files().join(',')
    left.remove()
    await instance.click('files-sync-rule-run-Abgleich')
    await ctx.waitFor(
      'the two-way rule waiting for its place',
      async () => {
        const views = unwrap<
          { rule: { name: string }; status: { state: string } }[]
        >('sync_rules_list', await instance.invoke('sync_rules_list'))
        return views.some(
          (view) =>
            view.rule.name === 'Abgleich' && view.status.state === 'waiting',
        )
      },
      { timeoutMs: 30_000 },
    )
    if (right.files().join(',') !== before) {
      throw new Error('a place that is gone made the rule delete')
    }

    // Manage: an exclusion through the editor, the preview, the log and its depth.
    const closeModal = () =>
      instance.exec(
        `document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', keyCode: 27, bubbles: true }))`,
      )
    await instance.click('files-sync-rule-more-Sicherung')
    await instance.click('files-sync-rule-ignore-Sicherung')
    await instance.type('files-sync-rule-ignore-text', '*.tmp')
    await instance.click('files-sync-rule-ignore-save')
    await instance.waitForDisplayed('files-sync-rule-ignore-saved', 15_000)
    if (source.read('.haex-ignore') !== '*.tmp') {
      throw new Error('the editor did not write .haex-ignore in place A')
    }
    await closeModal()
    source.write('neu.txt', 'N')
    source.write('cache.tmp', 'T')
    await instance.click('files-sync-rule-more-Sicherung')
    await instance.click('files-sync-rule-preview-Sicherung')
    await instance.waitForDisplayed('files-sync-rule-plan-ignored', 15_000)
    await closeModal()
    await instance.click('files-sync-rule-run-Sicherung')
    await ctx.waitFor(
      'the new file at the target, the excluded one not',
      () => target.files().includes('neu.txt'),
      { timeoutMs: 30_000 },
    )
    if (target.files().includes('cache.tmp')) {
      throw new Error('an excluded file was copied')
    }
    const ruleId = (
      unwrap<{ rule: { id: string; name: string } }[]>(
        'sync_rules_list',
        await instance.invoke('sync_rules_list'),
      ).find((view) => view.rule.name === 'Sicherung') ??
      (() => {
        throw new Error('no rule Sicherung')
      })()
    ).rule.id
    const logged = async () =>
      unwrap<{ id: string }[]>(
        'sync_rules_runs',
        await instance.invoke('sync_rules_runs', { id: ruleId }),
      )
    await instance.click('files-sync-rule-more-Sicherung')
    await instance.click('files-sync-rule-log-Sicherung')
    await instance.click('files-sync-rule-log-open-0', 15_000)
    await closeModal()
    const copiedNew = await ctx.waitFor(
      'a logged run that copied neu.txt',
      async () => {
        for (const run of await logged()) {
          const items = unwrap<{ path: string; action: string }[]>(
            'sync_rules_run_items',
            await instance.invoke('sync_rules_run_items', {
              runId: run.id,
              offset: 0,
              limit: 100,
            }),
          )
          if (
            items.some(
              (item) => item.path === 'neu.txt' && item.action === 'copy',
            )
          )
            return true
        }
        return false
      },
      { timeoutMs: 15_000 },
    )
    if (!copiedNew) throw new Error('the log misses the copy')
    const runsBefore = (await logged()).length
    if (runsBefore < 3) throw new Error(`only ${runsBefore} runs logged`)
    // The dialog offers both pickers.
    await instance.click('files-sync-rules-new')
    await instance.waitForDisplayed('files-picker-a-root')
    await instance.waitForDisplayed('files-picker-b-root')
    await closeModal()

    // The log depth from the settings: the older runs go at once.
    await runAction(instance, 'wm.app.open', {
      appId: 'system.settings',
      at: '/general/sync-log',
    })
    await instance.type(
      'settings-sync-log-depth',
      // The field holds the default 10: clear it, type 2, save.
      `${KEY.backspace.repeat(3)}2${KEY.enter}`,
      15_000,
    )
    await ctx.waitFor(
      'the log kept to two runs',
      async () => (await logged()).length === 2,
      { timeoutMs: 15_000 },
    )
  } finally {
    source.remove()
    target.remove()
    left.remove()
    right.remove()
    askFrom.remove()
    askTo.remove()
  }
})
