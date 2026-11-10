// Run with `node scripts/check-vue-templates.ts`. Compiles every tracked
// `.vue` file's template with the same compiler the build uses, without
// running a build.
//
// Why this exists: `pnpm typecheck` does not parse template expressions and
// `pnpm lint` does not either, so a template that the SFC compiler rejects
// passes both. One did — `HuggingFaceModelManagement.vue`'s tab handler broke
// `pnpm generate` on `main` and nobody noticed, because no CI job builds.
//
// The specific trap is a multi-statement inline handler. Vue collapses a
// template attribute's newlines before parsing it, so `@click="a = 1\nb = 2"`
// needs `;` separators — and Prettier's `semi: false` strips exactly those,
// leaving markup that will not compile. Put such handlers in a named function
// in `<script setup>`; that is the only form Prettier and Vue agree on.
//
// Plain `node scripts/<name>.ts`: Node strips the types itself from 22.19
// which is this repository's declared `engines` floor and the version CI
// pins, so no bundler, runner or extra dependency is involved.
import { execFileSync } from 'node:child_process'
import { readFileSync } from 'node:fs'
import { compileTemplate, parse } from 'vue/compiler-sfc'

const files = execFileSync('git', ['ls-files', '-z', '--', '*.vue'], {
  encoding: 'utf8',
})
  .split('\0')
  .filter(Boolean)

if (files.length === 0) {
  console.error('No tracked .vue files found — is this a Git checkout?')
  process.exit(1)
}

const failures: { filename: string; message: string }[] = []

for (const filename of files) {
  const source = readFileSync(filename, 'utf8')
  const { descriptor, errors: parseErrors } = parse(source, { filename })

  for (const error of parseErrors) {
    failures.push({ filename, message: describe(error) })
  }

  // A component may legitimately be render-function-only or template-less.
  if (!descriptor.template) continue

  const { errors } = compileTemplate({
    id: filename,
    filename,
    source: descriptor.template.content,
    scoped: descriptor.styles.some((style) => style.scoped),
  })
  for (const error of errors) {
    failures.push({ filename, message: describe(error) })
  }
}

// Spec 020-tab-navigation (research R20, SC-007): every state-changing control runs a catalog
// action (`useAction`/`useActionOrThrow`), so the UI, shortcuts and agents share one code path. A
// component calling one of these write APIs directly bypasses the catalog. Allowed only with an
// `action-exempt: <reason>` comment on the same line or up to three lines above.
const DIRECT_WRITES = [
  /\bwm\.(openApp|addTab|switchTab|closeTab|closeWindow|focusWindow|minimizeWindow|toggleMaximizeWindow|updateWindowGeometry|createWorkspace|switchWorkspace|deleteWorkspace|moveWindowToWorkspace|navigate|goTab)\(/,
  /\b(setPrefAsync|clearPrefAsync|updateDeviceAliasAsync)\(/,
  /\b(downloadFromHfAsync|downloadFromCatalogAsync|deleteAsync|installUpdateAsync|connectCliDelegateAsync|submitCliDelegateCodeAsync|refreshModelsAsync)\(/,
  /\b(loadModel|updateEffortLevel|downloadCatalogEntry|retryModelLoad|onIntegrityLoadUntrusted|onIntegrityRepairSource|onIntegrityChooseOther)\(/,
]

for (const filename of files) {
  if (!filename.startsWith('src/')) continue
  const lines = readFileSync(filename, 'utf8').split('\n')
  lines.forEach((line, index) => {
    if (!DIRECT_WRITES.some((pattern) => pattern.test(line))) return
    const context = lines.slice(Math.max(0, index - 3), index + 1).join('\n')
    if (context.includes('action-exempt:')) return
    failures.push({
      filename,
      message: `direct write outside the action catalog (line ${index + 1}): ${line.trim()} — trigger its action via useAction/useActionOrThrow, or mark it action-exempt: <reason>`,
    })
  })
}

// Spec 023-settings-app (research R9): palette colors do not follow the color scheme, so gray
// text on a dark background or a white surface would be unreadable in dark mode. Components use
// the theme's colors (`text-muted-foreground`, `bg-primary/10`, `text-destructive`, …).
const PALETTE_COLOR =
  /\b(?:text|bg|border|ring|divide|fill|stroke|outline|from|to|via)-(?:white|black|neutral|gray|slate|zinc|stone|red|green|amber|yellow|blue|emerald|orange|sky|indigo|rose)(?:-\d{2,3})?(?:\/\d+)?\b/

for (const filename of files) {
  if (!filename.startsWith('src/')) continue
  readFileSync(filename, 'utf8')
    .split('\n')
    .forEach((line, index) => {
      const match = PALETTE_COLOR.exec(line)
      if (!match) return
      failures.push({
        filename,
        message: `palette color ${match[0]} (line ${index + 1}) — use a theme color instead, it follows the color scheme`,
      })
    })
}

// Nuxt names a component after its folder (`components/onboarding/LinkScanner.vue` is
// `<OnboardingLinkScanner>`), and a tag that does not resolve compiles and type-checks fine: Vue only
// warns at run time and renders nothing. So the one wrong form that is easy to write is caught here: a
// tag that is the bare file name of a component in a folder, when no component has that name.
const pascal = (text: string) =>
  text
    .split(/[-_]/)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join('')
const componentNames = new Set<string>()
const folderForms = new Map<string, string[]>()
for (const filename of files) {
  const match = /^src\/components\/(.+)\.vue$/.exec(filename)
  if (!match) continue
  const parts = match[1].split('/')
  const base = parts[parts.length - 1]
  const prefix = parts.slice(0, -1).map(pascal).join('')
  // The name with the folders in front, and, as Nuxt does, without a folder name the file repeats.
  const names = [prefix + base]
  if (prefix && base.startsWith(prefix)) names.push(base)
  for (const name of names) componentNames.add(name)
  if (prefix)
    folderForms.set(base, [...(folderForms.get(base) ?? []), prefix + base])
}
for (const filename of files) {
  if (!filename.startsWith('src/')) continue
  const { descriptor } = parse(readFileSync(filename, 'utf8'), { filename })
  const template = descriptor.template?.content ?? ''
  const script = `${descriptor.scriptSetup?.content ?? ''}\n${descriptor.script?.content ?? ''}`
  for (const tag of new Set(
    [...template.matchAll(/<([A-Z][A-Za-z0-9]*)[\s/>]/g)].map((m) => m[1]),
  )) {
    const forms = folderForms.get(tag)
    if (!forms || componentNames.has(tag)) continue
    // A component imported in the script is not resolved by name.
    if (new RegExp(`\\bimport\\b[^;]*\\b${tag}\\b`).test(script)) continue
    failures.push({
      filename,
      message: `unresolved component <${tag}> — Nuxt names it after its folder: use <${forms.join('> or <')}>`,
    })
  }
}

function describe(error: unknown) {
  const detail = error as {
    message?: string
    loc?: { start?: { line?: number } }
  }
  const message = String(detail?.message ?? error)
  const line = detail?.loc?.start?.line
  return line ? `${message} (template line ${line})` : message
}

if (failures.length > 0) {
  console.error(
    `${failures.length} template error(s) in ${new Set(failures.map((f) => f.filename)).size} file(s):\n`,
  )
  for (const { filename, message } of failures) {
    console.error(`  ${filename}\n    ${message}`)
  }
  if (
    failures.some(
      (f) =>
        !f.message.startsWith('direct write') &&
        !f.message.startsWith('palette color') &&
        !f.message.startsWith('unresolved component'),
    )
  ) {
    console.error(
      '\nA multi-statement inline handler is the usual cause of a template' +
        '\nerror. Move it into a named function in `<script setup>` — see' +
        '\nAliasSetting.vue.',
    )
  }
  process.exit(1)
}

console.log(
  `${files.length} Vue templates compile; no direct writes outside the action catalog; no palette colors; no component tag without its folder prefix.`,
)
