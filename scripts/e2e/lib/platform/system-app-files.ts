import { existsSync, readdirSync, readFileSync } from 'node:fs'
import { join } from 'node:path'

/** The host-side files the encrypted-folder system-app flow leaves behind. */
export function encryptedSystemAppFiles(root: string) {
  const opened = join(root, 'cache', 'com.haex.holzi', 'files-opened-encrypted')
  const logs = join(root, 'data', 'com.haex.holzi', 'logs')
  return {
    hasOpenedCopy: () => existsSync(opened) && readdirSync(opened).length > 0,
    hasNoOpenedCopy: () =>
      !existsSync(opened) || readdirSync(opened).length === 0,
    logTexts: () => {
      const files = existsSync(logs) ? readdirSync(logs) : []
      return files.map((file) => readFileSync(join(logs, file), 'utf8'))
    },
  }
}
