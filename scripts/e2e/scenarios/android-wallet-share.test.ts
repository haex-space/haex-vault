import assert from 'node:assert/strict'
import { scenario } from '../lib/scenario.ts'
import { createAndUnlock } from '../lib/flows.ts'
import { deviceFiles } from '../lib/extension-files.ts'
import { overview } from '../lib/passwords.ts'
import { createAdb } from '../lib/platform/adb.ts'
import { PACKAGE } from '../lib/platform/android.ts'
import { storeCard, trainPass } from '../lib/pkpass.ts'

const TRAIN_SECRET = 'SECRET-MARKER-E2E-SHARE-TRAIN'
const SHOP_SECRET = 'SECRET-MARKER-E2E-SHARE-SHOP'
const PASS_MIME = 'application/vnd.apple.pkpass'

// The app takes only `content:` addresses of other apps, so the files are offered by the provider of
// the system's shell app: the shell (adb) writes into its folder and grants the read with the intent.
const SHELL_PROVIDER = 'content://com.android.shell/bugreports'
const SHELL_FOLDER = '/data/user_de/0/com.android.shell/files/bugreports'

/** Puts the device file `from` into the shell's provider folder and returns its `content:` address. */
function offer(from: string, name: string): string {
  const adb = createAdb(process.env.ANDROID_SERIAL ?? '')
  adb.shell('mkdir', '-p', SHELL_FOLDER)
  adb.shell('cp', `'${from}'`, `'${SHELL_FOLDER}/${name}'`)
  return `${SHELL_PROVIDER}/${name}`
}

/**
 * Why no copy appeared, as text for the failure: a phone scenario's material has no logcat. The
 * system's refusal ("Permission Denial: opening provider …") is in it, not in anything the test sees.
 */
function diagnosis(): string {
  const adb = createAdb(process.env.ANDROID_SERIAL ?? '')
  const run = (label: string, task: () => string) => {
    try {
      return `--- ${label}\n${task()}`
    } catch (error) {
      return `--- ${label} failed: ${(error as Error).message.slice(0, 300)}`
    }
  }
  return [
    run('shell folder', () => adb.shell('ls', '-laZ', SHELL_FOLDER)),
    run('app cache', () =>
      adb.runAs(PACKAGE, 'ls -la cache cache/shared-files 2>&1').toString(),
    ),
    run('logcat', () =>
      adb
        .shell('logcat', '-d', '-t', '2000')
        .split('\n')
        .filter((line) =>
          /holzi|SharedFiles|AndroidRuntime|FATAL|SecurityException|Permission Denial|ContentProviderHelper|ActivityTaskManager.*SEND/i.test(
            line,
          ),
        )
        .slice(-40)
        .join('\n'),
    ),
  ].join('\n')
}

/** How many copies of shared files the app holds in its cache now. */
function copies(): number {
  const adb = createAdb(process.env.ANDROID_SERIAL ?? '')
  const out = adb
    .runAs(PACKAGE, 'ls cache/shared-files 2>/dev/null | wc -l')
    .toString()
    .trim()
  return Number.parseInt(out, 10)
}

// Spec 092, US8 (FR-037, FR-035): a pass file shared to the app waits until a vault is open and
// then shows the import dialog in Wallet; a share to the running app shows it at once. The copy the
// app made of a shared file is gone once the file has been read, whether the pass is saved or not,
// and no code value shows as text.
scenario(
  'android-wallet-share',
  { needs: { phone: true }, timeoutMs: 420_000 },
  async (ctx) => {
    const files = deviceFiles('e2e-wallet-share-')
    try {
      const instance = await ctx.startInstance()
      // After the start: it clears the app's data, and with it its download folder.
      files.write('train.pkpass', trainPass('SH-1', TRAIN_SECRET))
      files.write('shop.pkpass', storeCard('SH-2', SHOP_SECRET))
      ctx.step('files written')

      // No vault is open: the share waits.
      instance.phone!.shareContent(
        offer(files.path('train.pkpass'), 'train.pkpass'),
        PASS_MIME,
      )
      try {
        await ctx.waitFor(
          'the app to copy the share',
          async () => copies() === 1,
          {
            timeoutMs: 20_000,
          },
        )
      } catch (error) {
        throw new Error(`${(error as Error).message}\n${diagnosis()}`)
      }
      ctx.step('the share waits with a copy')

      // The vault opens: the dialog shows the pass; skipping writes nothing and the copy is gone.
      await createAndUnlock(instance, { name: 'phone-vault' })
      await instance.waitForDisplayed('wallet-import-form', 30_000)
      assert.equal(
        await instance.exec<boolean>(
          `return document.body.innerText.includes('SECRET-MARKER-E2E-SHARE')`,
        ),
        false,
        'no code value on the screen',
      )
      await ctx.waitFor(
        'the copy to go once it was read',
        async () => copies() === 0,
      )
      await instance.click('wallet-import-skip')
      assert.equal(
        (await overview(instance)).headers.filter((h) =>
          h.title?.includes('Berlin'),
        ).length,
        0,
        'skipping saves nothing',
      )
      ctx.step('skipped: nothing saved, copy gone')

      // A share to the running app shows the dialog at once; saving writes the pass.
      instance.phone!.shareContent(
        offer(files.path('shop.pkpass'), 'shop.pkpass'),
        PASS_MIME,
      )
      await instance.waitForDisplayed('wallet-import-form', 30_000)
      await instance.click('wallet-import-save')
      await instance.waitForDisplayed('wallet-import-done')
      await instance.click('wallet-import-close')
      const card = (await overview(instance)).headers.find((h) =>
        h.title?.includes('Corner Shop'),
      )
      assert.ok(card, 'the shared card is in the vault')
      assert.equal(copies(), 0, 'no copy is left')
      ctx.step('a share to the running app imported')
    } finally {
      files.remove()
    }
  },
)
