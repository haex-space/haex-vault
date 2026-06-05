import { describe, expect, it } from 'vitest'
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'

// D3 — once a site has been migrated to createOnceListener, any reintroduction
// of a raw `await listen(` would re-open the TOCTOU race. Strip comments
// before checking so doc references to `listen()` don't trip the guard.
const FIXED_SITES = [
  'src/stores/sync/syncEvents.ts',
  'src/composables/handlers/useCoreExternalRequestHandlers.ts',
  'src/composables/useExternalAuth.ts',
  'src/stores/file-sync.ts',
  'src/stores/peer-storage.ts',
  'src/composables/usePermissionPrompt.ts',
]

describe('D3: no raw `await listen(` in TOCTOU-fixed sites', () => {
  for (const relPath of FIXED_SITES) {
    it(relPath, () => {
      const absPath = resolve(__dirname, '../../..', relPath)
      const content = readFileSync(absPath, 'utf8')
      const stripped = content
        .replace(/\/\*[\s\S]*?\*\//g, '')
        .replace(/\/\/.*$/gm, '')
      expect(
        stripped,
        `raw \`await listen(\` regressed in ${relPath} — wrap with createOnceListener`,
      ).not.toMatch(/await\s+listen\s*[<(]/)
    })
  }
})
