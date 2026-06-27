import { describe, expect, it } from 'vitest'
import { readFileSync, readdirSync, statSync } from 'node:fs'
import { join, resolve } from 'node:path'

/**
 * Guards against the PR #520 class of regression: a `.vue` template referenced
 * a component by its short name (e.g. `<ImportWizardShell>`) while Nuxt only
 * auto-registers the path-prefixed name (`HaexSystemPasswordsImportWizardShell`).
 * Vue silently rendered the short reference as a custom HTML element, so all
 * three password import dialogs were dead with no console error.
 *
 * The test scans every `.vue` file under `src/components/`, extracts the
 * PascalCase tags from its `<template>` block, and asserts that each tag is
 * EITHER auto-registered (present in `.nuxt/components.d.ts`) OR explicitly
 * imported in the file's `<script setup>` block.
 */
const ROOT = resolve(__dirname, '../../..')
const COMPONENTS_DIR = join(ROOT, 'src/components')
const NUXT_COMPONENTS_DTS = join(ROOT, '.nuxt/components.d.ts')

// Vue / Nuxt built-ins that never appear in components.d.ts.
const BUILTINS = new Set([
  'Transition',
  'TransitionGroup',
  'KeepAlive',
  'Suspense',
  'Teleport',
  'Component',
  'Fragment',
  'ClientOnly',
  'DevOnly',
  'NuxtErrorBoundary',
  'NuxtLink',
  'NuxtPage',
  'NuxtLayout',
  'NuxtImg',
  'NuxtPicture',
  'NuxtLoadingIndicator',
  'NuxtRouteAnnouncer',
  'NuxtIcon',
  'NuxtClientFallback',
  'I18nT',
  'I18nN',
  'I18nD',
])

function loadRegisteredComponents(): Set<string> {
  const src = readFileSync(NUXT_COMPONENTS_DTS, 'utf8')
  const names = new Set<string>()
  for (const m of src.matchAll(/export const (\w+):/g)) {
    let name = m[1]!
    if (name.startsWith('Lazy')) name = name.slice(4)
    names.add(name)
  }
  return names
}

function walkVueFiles(dir: string, out: string[]): void {
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry)
    if (statSync(full).isDirectory()) walkVueFiles(full, out)
    else if (entry.endsWith('.vue')) out.push(full)
  }
}

function extractBlock(src: string, tag: 'template' | 'script'): string {
  // Find FIRST opening tag of this kind. For <script>, also accept attrs
  // (lang, setup, generic, …). Matches up to the matching closing tag.
  const re = new RegExp(`<${tag}\\b[^>]*>([\\s\\S]*?)<\\/${tag}>`, 'i')
  const m = src.match(re)
  return m ? m[1]! : ''
}

function extractScriptLocals(scriptSrc: string): Set<string> {
  // Strip comments first so commented-out code doesn't pollute the locals set.
  const stripped = scriptSrc
    .replace(/\/\*[\s\S]*?\*\//g, '')
    .replace(/\/\/.*$/gm, '')

  const locals = new Set<string>()

  // import X from '…'
  for (const m of stripped.matchAll(/import\s+(\w+)(?=\s*[,;]|\s+from\b)/g)) {
    locals.add(m[1]!)
  }
  // import { a, b as c } from '…'  (also handles default + named: `import X, { Y }`)
  for (const m of stripped.matchAll(/import\s+(?:\w+\s*,\s*)?\{([^}]+)\}/g)) {
    for (const raw of m[1]!.split(',')) {
      const seg = raw.trim().split(/\s+as\s+/)
      const name = (seg[seg.length - 1] ?? '').trim().replace(/^type\s+/, '')
      if (name) locals.add(name)
    }
  }
  // import * as X from '…'
  for (const m of stripped.matchAll(/import\s+\*\s+as\s+(\w+)\s+from\b/g)) {
    locals.add(m[1]!)
  }
  // Top-level const/let/var declarations — covers `const X = defineAsyncComponent(...)`
  // and shallow `const X = ref(...)`. Permissive on purpose; the goal is to
  // suppress false positives, not police variable usage.
  for (const m of stripped.matchAll(/(?:^|\n)\s*(?:const|let|var)\s+(\w+)\s*=/g)) {
    locals.add(m[1]!)
  }

  return locals
}

function extractPascalTags(templateSrc: string): Set<string> {
  // Drop HTML comments and Vue-style attribute strings that contain `<` so
  // they can't trigger false matches.
  const stripped = templateSrc.replace(/<!--[\s\S]*?-->/g, '')
  const tags = new Set<string>()
  for (const m of stripped.matchAll(/<([A-Z][A-Za-z0-9]*)(?=[\s/>])/g)) {
    tags.add(m[1]!)
  }
  return tags
}

describe('Vue template component references resolve via auto-import or script-setup import', () => {
  const registered = loadRegisteredComponents()
  const files: string[] = []
  walkVueFiles(COMPONENTS_DIR, files)

  const failures: Array<{ file: string; tag: string }> = []

  for (const file of files) {
    const src = readFileSync(file, 'utf8')
    const templateSrc = extractBlock(src, 'template')
    if (!templateSrc) continue

    const tags = extractPascalTags(templateSrc)
    if (tags.size === 0) continue

    const scriptSrc = extractBlock(src, 'script')
    const scriptLocals = scriptSrc ? extractScriptLocals(scriptSrc) : new Set<string>()

    for (const tag of tags) {
      if (BUILTINS.has(tag)) continue
      if (scriptLocals.has(tag)) continue
      if (registered.has(tag)) continue
      failures.push({
        file: file.replace(`${ROOT}/`, ''),
        tag,
      })
    }
  }

  it('no unresolved PascalCase tags', () => {
    if (failures.length === 0) return
    const lines = failures
      .map((f) => `  ${f.file}: <${f.tag}>`)
      .join('\n')
    expect.fail(
      `${failures.length} unresolved PascalCase template tag(s):\n${lines}\n\n`
      + `Each tag is neither auto-registered (.nuxt/components.d.ts) nor declared in <script setup>. `
      + `Vue will render these as unknown HTML elements with no error — the silent-broken-dialog class of bug.`,
    )
  })
})
