// The vault files of an instance in the app's storage (spec 043, scenario vault-file-import): read
// out to where a person would pick them, and removed as if the vault had gone to another device. On
// Linux the files lie below the instance's data root, on Android in the app's data directory,
// reached through `run-as` (debug builds only).
import { createHash } from 'node:crypto'
import { existsSync, readFileSync, rmSync } from 'node:fs'
import { join } from 'node:path'
import { createAdb } from './platform/adb.ts'
import { PACKAGE } from './platform/android.ts'
import { DATABASE_FILE, VAULT_DIR } from './platform/linux.ts'

const ANDROID_PREFIX = 'android:'
/** How often a vault file is read through adb before the scenario gives up. */
const READ_TRIES = 3
/** The diagnostic log of a vault, relative to the vault's folder. */
const LOG_FILE = 'logs/haex.log'
/** The snapshot shared by the Android settings screen. */
const SHARED_LOG_COPY = 'cache/diagnostic-log/haex-diagnostic-log.txt'

const sha256 = (bytes: Buffer) =>
  createHash('sha256').update(bytes).digest('hex')

export interface VaultFiles {
  /** The bytes of the vault's database (`vaults/<vault>/vault.db`); the app must have ended. */
  read: (vault: string) => Buffer
  /** Removes the folder of the vault with everything in it. */
  remove: (vault: string) => void
  /** The text of the vault's diagnostic log (`logs/haex.log`, spec 098), or null while it does not exist. */
  log: (vault: string) => string | null
  /** The app-cache snapshot made for Android's share sheet, or null when it does not exist. */
  sharedLogCopy: () => string | null
}

export function vaultFiles(root: string): VaultFiles {
  if (root.startsWith(ANDROID_PREFIX)) {
    const adb = createAdb(root.slice(ANDROID_PREFIX.length))
    // Relative to the app's data directory, where `run-as` starts.
    return {
      // `exec-out` once handed back other bytes than the file's in CI (the app then called the
      // copy malformed): a read counts only with the SHA-256 the device computes for the file.
      read: (vault) => {
        const file = `vaults/${vault}/${DATABASE_FILE}`
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
      remove: (vault) => void adb.runAs(PACKAGE, `rm -rf vaults/${vault}`),
      log: (vault) => {
        const file = `vaults/${vault}/${LOG_FILE}`
        const exists = adb
          .runAs(PACKAGE, `[ -f '${file}' ] && echo yes || echo no`)
          .toString()
          .trim()
        if (exists !== 'yes') return null
        return adb.runAs(PACKAGE, `cat ${file}`).toString()
      },
      sharedLogCopy: () => {
        const exists = adb
          .runAs(PACKAGE, `[ -f '${SHARED_LOG_COPY}' ] && echo yes || echo no`)
          .toString()
          .trim()
        if (exists !== 'yes') return null
        return adb.runAs(PACKAGE, `cat '${SHARED_LOG_COPY}'`).toString()
      },
    }
  }
  const dir = join(root, ...VAULT_DIR)
  return {
    read: (vault) => readFileSync(join(dir, vault, DATABASE_FILE)),
    remove: (vault) =>
      rmSync(join(dir, vault), { recursive: true, force: true }),
    log: (vault) => {
      const file = join(dir, vault, LOG_FILE)
      return existsSync(file) ? readFileSync(file, 'utf8') : null
    },
    sharedLogCopy: () => null,
  }
}
