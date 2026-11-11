import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import type { ScenarioContext } from '../lib/scenario.ts'
import {
  createAndUnlock,
  openLauncher,
  unwrap,
  waitForWorkspace,
} from '../lib/flows.ts'
import { createEntry, entryTitles } from '../lib/passwords.ts'
import { PROCESS_END_LIMIT_MS } from '../lib/close-promises.ts'
import { deviceFiles } from '../lib/platform/device-files.ts'
import { vaultFiles } from '../lib/vault-files.ts'

const ENTRY = 'Bank'
const FILE = 'Annas Vault.db'
const IMPORTED = 'Annas-Vault'

type Instance = Awaited<ReturnType<ScenarioContext['startInstance']>>

/** Closes the vault, which ends the app (spec 013), and lets go of the driver. */
async function lock(instance: Instance): Promise<void> {
  await openLauncher(instance)
  await instance.press('lock-instance')
  await instance.waitForEnd(PROCESS_END_LIMIT_MS)
  await instance.stop()
}

// Spec 043 FR-002a (contract import-instance.md), on the desktop and on Android: a vault file from
// elsewhere opens on this device as a copy, under a name from the file name and with its entries; the
// same file a second time is refused because the vault is already here, and the chosen file stays as
// it was. The file is handed over as a path in a folder the app can read (the test seam of contract
// picked-file.md): a driver cannot work the system's file dialog.
scenario(
  'vault-file-import',
  { needs: { closeBehavior: 'exit' } },
  async (ctx) => {
    const { passphrase } = ctx.credentials()
    const first = await ctx.startInstance()
    await createAndUnlock(first, { name: 'anna', passphrase })
    await createEntry(first, { title: ENTRY })
    // The vault's public identity is published a moment after the first unlock; a file copied
    // before it has no identity, and holzi refuses it as no vault.
    await ctx.waitFor(
      'the vault to hold its public identity',
      async () => {
        const answer = await first.invoke('vault_public_identity')
        return !('ended' in answer) && answer.ok
      },
      { timeoutMs: 30_000 },
    )
    await lock(first)

    // The vault went to another device: its file is all that is left of it here.
    const files = deviceFiles('e2e-vault-file-')
    ctx.onTeardown(() => files.remove())
    const vault = vaultFiles(first.root)
    const original = vault.read('anna')
    files.write(FILE, original)
    vault.remove('anna')
    ctx.step('the vault file lies outside the app')

    const second = await ctx.startInstance({ reusesRoot: first.root })
    const imported = unwrap<{ info: { name: string } }>(
      'import_instance',
      await second.invoke('import_instance', {
        args: { file: files.path(FILE), passphrase },
      }),
    )
    assert.equal(imported.info.name, IMPORTED)
    await second.navigate(
      `tauri://localhost/workspace/${encodeURIComponent(IMPORTED)}`,
    )
    await waitForWorkspace(second)
    assert.deepEqual(await entryTitles(second), [ENTRY])
    ctx.step('the copy opens with its entries')
    await lock(second)

    const third = await ctx.startInstance({ reusesRoot: first.root })
    const again = await third.invoke('import_instance', {
      args: { file: files.path(FILE), passphrase },
    })
    assert.ok('ok' in again && !again.ok, 'the second copy was taken over')
    assert.deepEqual(again.error, {
      kind: 'AlreadyOnThisDevice',
      name: IMPORTED,
    })
    const names = unwrap<{ name: string }[]>(
      'list_instances',
      await third.invoke('list_instances'),
    ).map((instance) => instance.name)
    assert.deepEqual(names, [IMPORTED])
    assert.ok(files.readBytes(FILE).equals(original), 'the chosen file changed')
    ctx.step('the same file again is refused and stays as it was')
  },
)
