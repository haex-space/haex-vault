/**
 * Guards against the "icon fetched from api.iconify.design at runtime" CSP
 * failure. Tauri's CSP blocks connect-src for the Iconify API, so every icon
 * the app references MUST live in the precomputed client bundle.
 *
 * The bundle is produced by `nuxt prepare` (postinstall) into
 * `.nuxt/nuxt-icon-client-bundle.mjs`. This test:
 *
 * 1. Reproduces @nuxt/icon's source scanner (regex + glob) over `src/`.
 * 2. Loads the generated client bundle.
 * 3. Asserts every referenced icon is present.
 *
 * If this fails, either:
 *   - Add the icon literal to a scanned file (the scanner sees `i-lucide-foo`
 *     or `lucide:foo` patterns in code / templates).
 *   - Or extend `icon.clientBundle.icons` in nuxt.config.ts when the icon name
 *     is built dynamically (e.g. `'i-' + variant + '-foo'`).
 */

import { describe, it, expect } from 'vitest'
import { readFileSync, readdirSync, statSync, existsSync } from 'node:fs'
import { resolve, join } from 'node:path'

const ROOT = resolve(__dirname, '../..')
const BUNDLE_PATH = join(ROOT, '.nuxt/nuxt-icon-client-bundle.mjs')

// Collections we use. Keep in sync with nuxt.config.ts `icon.serverBundle.collections`
// plus any collection appearing in the explicit `icon.clientBundle.icons` list.
const COLLECTIONS = [
  'heroicons',
  'mdi',
  'line-md',
  'solar',
  'gg',
  'emojione',
  'lucide',
  'hugeicons',
  'simple-icons',
]

// Mirrors @nuxt/icon's IconUsageScanner regex:
//   new RegExp("\\b(?:i-)?(" + collectionsRegex + ")[:-]([a-z0-9-]+)\\b", "g")
// Longest prefixes first to avoid `line-md` being eaten by a shorter alternative.
const ICON_REGEX = new RegExp(
  '\\b(?:i-)?('
    + [...COLLECTIONS].sort((a, b) => b.length - a.length).join('|')
    + ')[:-]([a-z0-9-]+)\\b',
  'g',
)

// Mirrors @nuxt/icon's default globInclude + nuxt.config.ts override.
const SCAN_EXTS = /\.(vue|jsx|tsx|ts|js|mjs|cjs|md|mdc|mdx|yml|yaml)$/
// Mirrors @nuxt/icon's default globExclude (paths) + our own.
const SCAN_EXCLUDE_DIRS = new Set([
  'node_modules',
  'dist',
  'build',
  'coverage',
  'test',
  'tests',
  '.nuxt',
  '.output',
  'src-tauri',
])

function* walk(dir: string): Generator<string> {
  for (const entry of readdirSync(dir)) {
    if (SCAN_EXCLUDE_DIRS.has(entry) || entry.startsWith('.')) continue
    const p = join(dir, entry)
    const st = statSync(p)
    if (st.isDirectory()) yield* walk(p)
    else if (SCAN_EXTS.test(entry)) yield p
  }
}

function collectUsedIcons(): Set<string> {
  const set = new Set<string>()
  for (const file of walk(resolve(ROOT, 'src'))) {
    const code = readFileSync(file, 'utf-8')
    for (const m of code.matchAll(ICON_REGEX)) {
      set.add(`${m[1]}:${m[2]}`)
    }
  }
  return set
}

function loadBundledIcons(): Map<string, Set<string>> {
  if (!existsSync(BUNDLE_PATH)) {
    throw new Error(
      `Icon client bundle not found at ${BUNDLE_PATH}. Run \`pnpm nuxt prepare\` first.`,
    )
  }
  const source = readFileSync(BUNDLE_PATH, 'utf-8')
  // The bundle stores collections as: const collections = JSON.parse("...")
  // We extract that string literal and JSON-parse it twice (the outer parse
  // unescapes the embedded JSON).
  const match = source.match(/JSON\.parse\("(.*?)"\)/s)
  if (!match) {
    throw new Error(`Could not extract collections payload from ${BUNDLE_PATH}`)
  }
  const collections = JSON.parse(JSON.parse('"' + match[1] + '"')) as Array<{
    prefix: string
    icons: Record<string, unknown>
  }>
  const out = new Map<string, Set<string>>()
  for (const c of collections) {
    out.set(c.prefix, new Set(Object.keys(c.icons)))
  }
  return out
}

describe('Icon client bundle', () => {
  it('contains every iconify icon referenced in src/ (CSP blocks runtime api.iconify.design fetches)', () => {
    const used = collectUsedIcons()
    expect(used.size, 'No icon references detected — regex broken?').toBeGreaterThan(0)

    const bundled = loadBundledIcons()
    const missing: string[] = []
    for (const id of used) {
      const [prefix, name] = id.split(':')
      if (!prefix || !name || !bundled.get(prefix)?.has(name)) missing.push(id)
    }

    expect(
      missing,
      [
        'Icons referenced in source but missing from the client bundle.',
        'These would be fetched from https://api.iconify.design at runtime',
        'and blocked by Tauri\'s CSP (connect-src violation).',
        '',
        'Fix: either ensure the icon literal appears in a scanned file, or',
        'add it to `icon.clientBundle.icons` in nuxt.config.ts.',
        '',
        'Missing:',
        ...missing.sort().map(m => `  - ${m}`),
      ].join('\n'),
    ).toEqual([])
  })
})
