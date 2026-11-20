// The texts of the interface in every language haex-vault speaks, for checks that must hold whatever
// language a run uses: a scenario compares a shown text with all of them.
import { readFileSync, readdirSync } from 'node:fs'

const LOCALES = new URL('../../../src/i18n/locales/', import.meta.url)

/** The text under the dotted `key` (`models.downloadConfirm.startAnyway`) in each language. */
export function translations(key: string): string[] {
  return readdirSync(LOCALES)
    .filter((file) => file.endsWith('.json'))
    .map((file) => {
      let node: unknown = JSON.parse(
        readFileSync(new URL(file, LOCALES), 'utf8'),
      )
      for (const part of key.split('.')) {
        node = (node as Record<string, unknown> | undefined)?.[part]
      }
      if (typeof node !== 'string') {
        throw new Error(`${file} has no text under ${key}`)
      }
      return node
    })
}
