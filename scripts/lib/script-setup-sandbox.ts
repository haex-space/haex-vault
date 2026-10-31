// A small sandbox for pages whose `<script setup>` block can be replayed on its own, for the
// frontend checks of spec 013 (`node scripts/check-vault-lifecycle.ts`). The chat page has its own,
// much larger harness (`chat-state-harness.ts`); this one is for short pages and components, where
// booting every composable would only hide what the case is about.
//
// The block is transpiled with the `typescript` package already used by the other harness and run
// in its own `new Function` scope. Nuxt's auto-imports and compiler macros have no module behind
// them here, so they are injected as bare names. The ones that reach outside the page
// (`useInstance`, `useInstancesStore`, `useWindowManagerStore`, `useLanguage`, `usePickedFile`,
// `navigateTo`) throw unless the case provides them, so a page that starts using one of them without
// the case knowing fails loudly instead of doing nothing.
import { readFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { dirname, resolve as resolvePath } from 'node:path'
import { fileURLToPath } from 'node:url'
import ts from 'typescript'

const nodeRequire = createRequire(import.meta.url)
const repoRoot = resolvePath(dirname(fileURLToPath(import.meta.url)), '../..')

/** What a case can provide to the page. Anything left out is a loud failure or a plain double. */
export interface ScriptSetupGlobals {
  useInstance?: () => unknown
  useColorScheme?: () => unknown
  useInstancesStore?: () => unknown
  useWindowManagerStore?: () => unknown
  useFieldLabels?: () => unknown
  useErrorString?: () => unknown
  useLanguage?: () => unknown
  usePickedFile?: () => unknown
  useMcpAgentsStore?: () => unknown
  useTabRouter?: () => unknown
  useActionOrThrow?: (action: string) => unknown
  navigateTo?: (to: string) => unknown
  /** The route params the page reads, `{ instance: 'vault' }` by default. */
  params?: Record<string, string>
  /** `defineProps<Props>()`'s return value — an empty object by default, since most cases so far
   * replayed a page that reads route params instead. Made reactive by `loadScriptSetup` (and
   * returned back as `.props`) so a case can mutate a field after load and see a `watch` on it
   * react, the same way a real parent updating a prop would. */
  props?: Record<string, unknown>
  /** `defineEmits<{...}>()`'s return value — a no-op by default (events are silently swallowed).
   * A case that needs to observe emit order/arguments provides its own spy here. */
  emit?: (event: string, ...args: unknown[]) => void
  /** Explicit module dependencies; every import absent from this map still fails loudly. */
  imports?: Record<string, unknown>
  /** Bare `window`/`document` stand-ins for a page that registers real DOM listeners. Left
   * unprovided, using either throws the same loud "not provided" error as the Nuxt auto-imports. */
  window?: unknown
  document?: unknown
}

function notProvided(name: string) {
  return () => {
    throw new Error(
      `script-setup sandbox: the page used '${name}', which the case did not provide`,
    )
  }
}

/**
 * Replays the `<script setup lang="ts">` block of `vueFile` (a path relative to the repository
 * root) and returns the top-level bindings named in `bindings`, plus `mount`/`unmount` (drains
 * every `onMounted`/`onBeforeUnmount` hook the block registered, in registration order — no real
 * Vue component instance exists to run them automatically).
 */
export function loadScriptSetup<T>(
  vueFile: string,
  bindings: string[],
  globals: ScriptSetupGlobals = {},
): T & {
  mount: () => Promise<unknown[]>
  unmount: () => Promise<unknown[]>
  /** The reactive `props` object `defineProps()` returned — see `ScriptSetupGlobals.props`. */
  props: Record<string, unknown>
} {
  const source = readFileSync(resolvePath(repoRoot, vueFile), 'utf8')
  const block = source.match(/<script setup lang="ts">([\s\S]*?)<\/script>/)
  if (!block) {
    throw new Error(
      `No <script setup lang="ts"> block found in ${vueFile}. The cases replay that block ` +
        'verbatim, so a renamed tag silently removes all coverage: fix the pattern here.',
    )
  }
  const code = ts.transpileModule(block[1], {
    compilerOptions: {
      target: ts.ScriptTarget.ES2022,
      module: ts.ModuleKind.CommonJS,
      esModuleInterop: true,
    },
  }).outputText

  const vue = nodeRequire('vue') as {
    ref: unknown
    computed: unknown
    watch: unknown
    reactive: <V extends object>(v: V) => V
  }
  const mountHooks: Array<() => unknown> = []
  const unmountHooks: Array<() => unknown> = []
  // Reactive, not a plain object: a case that mutates a field after load (e.g. simulating a
  // parent dismissing the sheet by setting `open` back to false) needs the page's own `watch` on
  // that prop to actually fire, the way it would against a real parent's prop update.
  const reactiveProps = vue.reactive({ ...(globals.props ?? {}) })
  const scope: Record<string, unknown> = {
    ref: vue.ref,
    computed: vue.computed,
    watch: vue.watch,
    onMounted: (hook: () => unknown) => mountHooks.push(hook),
    onBeforeUnmount: (hook: () => unknown) => unmountHooks.push(hook),
    defineProps: () => reactiveProps,
    defineEmits: () => globals.emit ?? (() => {}),
    useI18n: () => ({ t: (key: string) => key }),
    useRoute: () => ({ params: globals.params ?? { instance: 'vault' } }),
    useInstance: globals.useInstance ?? notProvided('useInstance'),
    useColorScheme: globals.useColorScheme ?? notProvided('useColorScheme'),
    useInstancesStore:
      globals.useInstancesStore ?? notProvided('useInstancesStore'),
    useWindowManagerStore:
      globals.useWindowManagerStore ?? notProvided('useWindowManagerStore'),
    useFieldLabels: globals.useFieldLabels ?? notProvided('useFieldLabels'),
    useErrorString: globals.useErrorString ?? notProvided('useErrorString'),
    useLanguage: globals.useLanguage ?? notProvided('useLanguage'),
    usePickedFile: globals.usePickedFile ?? notProvided('usePickedFile'),
    useMcpAgentsStore:
      globals.useMcpAgentsStore ?? notProvided('useMcpAgentsStore'),
    useTabRouter: globals.useTabRouter ?? notProvided('useTabRouter'),
    useActionOrThrow:
      globals.useActionOrThrow ?? notProvided('useActionOrThrow'),
    navigateTo: globals.navigateTo ?? notProvided('navigateTo'),
    window: 'window' in globals ? globals.window : notProvided('window'),
    document:
      'document' in globals ? globals.document : notProvided('document'),
  }
  const names = Object.keys(scope)
  const result = new Function(
    'exports',
    'require',
    ...names,
    `${code}\nreturn { ${bindings.join(', ')} };`,
  )(
    {},
    (specifier: string) => {
      if (globals.imports && Object.hasOwn(globals.imports, specifier))
        return globals.imports[specifier]
      throw new Error(
        `script-setup sandbox: unexpected import '${specifier}' in ${vueFile}`,
      )
    },
    ...names.map((name) => scope[name]),
  ) as T
  return {
    ...result,
    props: reactiveProps,
    mount: () => Promise.all(mountHooks.map((hook) => hook())),
    unmount: () => Promise.all(unmountHooks.map((hook) => hook())),
  }
}
