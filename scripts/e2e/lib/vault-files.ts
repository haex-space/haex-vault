// The vault files of an instance in the app's storage (spec 043, scenario vault-file-import): read
// out to where a person would pick them, and removed as if the vault had gone to another device. On
// Linux the files lie below the instance's data root, on Android in the app's data directory,
// reached through `run-as` (debug builds only).
import { createHash } from 'node:crypto'
import { readFileSync, readdirSync, rmSync } from 'node:fs'
import { join } from 'node:path'
import { createAdb } from './platform/adb.ts'
import { PACKAGE } from './platform/android.ts'
import { VAULT_DIR } from './platform/linux.ts'

const ANDROID_PREFIX = 'android:'
/** How often a vault file is read through adb before the scenario gives up. */
const READ_TRIES = 3

const sha256 = (bytes: Buffer) =>
  createHash('sha256').update(bytes).digest('hex')

export interface VaultFiles {
  /** The bytes of `<vault>.db`; the app must have ended. */
  read: (vault: string) => Buffer
  /** Removes `<vault>.db` and every file beside it that belongs to it. */
  remove: (vault: string) => void
}

export function vaultFiles(root: string): VaultFiles {
  if (root.startsWith(ANDROID_PREFIX)) {
    const adb = createAdb(root.slice(ANDROID_PREFIX.length))
    // Relative to the app's data directory, where `run-as` starts.
    return {
      // `exec-out` once handed back other bytes than the file's in CI (the app then called the
      // copy malformed): a read counts only with the SHA-256 the device computes for the file.
      read: (vault) => {
        const file = `instances/${vault}.db`
        const want = adb
          .runAs(PACKAGE, `sha256sum ${file}`)
          .toString()
          .split(' ')[0]
        for (let i = 0; i < READ_TRIES; i++) {
          const bytes = adb.runAs(PACKAGE, `cat ${file}`)
          if (sha256(bytes) === want) return bytes
        }
        throw new Error(
          `${file}: ${READ_TRIES} reads through adb differ from the file on the device`,
        )
      },
      remove: (vault) =>
        void adb.runAs(
          PACKAGE,
          `rm -f instances/${vault}.db instances/${vault}.db?*`,
        ),
    }
  }
  const dir = join(root, ...VAULT_DIR)
  return {
    read: (vault) => readFileSync(join(dir, `${vault}.db`)),
    remove: (vault) => {
      for (const file of readdirSync(dir)) {
        if (
          file === `${vault}.db` ||
          file.startsWith(`${vault}.db.`) ||
          file.startsWith(`${vault}.db-`)
        )
          rmSync(join(dir, file), { force: true })
      }
    },
  }
}
