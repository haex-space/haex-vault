// Replay harness shared by the frontend check scripts (`node scripts/check-chat-state.ts`,
// `node scripts/check-vault-lifecycle.ts`). It boots the real chat page's `<script setup>` block,
// the real composables (`useChat`, `useModels`, `useCatalog`, `useProviders`, `useInstance`,
// `usePreferences`, `useDevice`, `useAutoResizeTextarea`, `useErrorString`) and the real
// `useModelsStore` (a genuine Pinia instance, fresh per call) with Tauri replaced at its actual IPC
// boundary (`invoke`/`listen` from `@tauri-apps/api/core`/`event`). No browser or GPU is required.
//
// The sandbox is a hand-rolled CommonJS loader (using the `typescript` package already a
// dependency here) that transpiles the page, the store and every composable either imports, then
// executes each in its own `new Function('exports', 'require', ...)` sandbox. Real composable
// imports used to be stripped and replaced with hand-written fakes matching their public method
// names; that made any logic later moved into a composable invisible to these tests. Now only
// three things are faked: Tauri's own `invoke`/`listen` (there is no backend here),
// `onMounted`/`onBeforeUnmount` (no real Vue component instance exists to register them against —
// every composable's calls are collected into one array `mount()`/`unmount()` drain), and
// `dompurify` (needs a real DOM).
//
// Extracted from `scripts/check-chat-state.ts` for spec 013 (specs/013-vault-lifecycle-isolation,
// tasks T005), as that file's own header required before another case could be added.
import { readFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { dirname, resolve as resolvePath } from 'node:path'
import { fileURLToPath } from 'node:url'
import ts from 'typescript'
import { createVaultDataHub } from '../../src/lib/sync/vaultData.ts'
import { nextTick, reactive } from 'vue'

/** Same shape the page's own `useI18n()` gets — see the bare-globals list below. */
const useI18nDouble = () => ({ t: (key: string) => key })

const __dirname = dirname(fileURLToPath(import.meta.url))
const repoRoot = resolvePath(__dirname, '../..')
const nodeRequire = createRequire(import.meta.url)

const pageSource = readFileSync(
  resolvePath(repoRoot, 'src/components/apps/ChatApp.vue'),
  'utf8',
)
const setupBlock = pageSource.match(
  /<script setup lang="ts">([\s\S]*?)<\/script>/,
)
if (!setupBlock) {
  throw new Error(
    'No <script setup lang="ts"> block found in the chat page. These tests ' +
      'replay that block verbatim, so a renamed or reformatted tag silently ' +
      'removes all coverage — fix the pattern above rather than this message.',
  )
}

const CJS_OPTIONS = {
  compilerOptions: {
    target: ts.ScriptTarget.ES2022,
    module: ts.ModuleKind.CommonJS,
    esModuleInterop: true,
  },
}

const transpiledFileCache = new Map<string, string>()

function transpileFile(absPath: string): string {
  let cached = transpiledFileCache.get(absPath)
  if (cached === undefined) {
    cached = ts.transpileModule(
      readFileSync(absPath, 'utf8'),
      CJS_OPTIONS,
    ).outputText
    transpiledFileCache.set(absPath, cached)
  }
  return cached
}

const pageCode = ts.transpileModule(setupBlock[1], CJS_OPTIONS).outputText

/** A real composable's absolute path, for `runComposable` below or a case that needs the raw path
 * (e.g. `check-vault-lifecycle.ts` running `useErrorString` on its own, with no chat page around
 * it). */
export function composablePath(name: string): string {
  return resolvePath(repoRoot, 'src/composables', `${name}.ts`)
}

/**
 * Executes one real composable file's transpiled CommonJS output in its own
 * sandbox and returns its exports. `req` is threaded through so a composable
 * that itself imports another `~/composables/*` module (none do today, but
 * a future page split may add one) resolves the same way as the page does.
 *
 * Returns whatever `Function`'s own call signature returns (loosely typed
 * by design, like the rest of this sandbox — see `noImplicitAny` above).
 */
export function runComposable(
  absPath: string,
  req: (specifier: string) => unknown,
  autoImports: Record<string, unknown> = {},
) {
  const autoImportNames = Object.keys(autoImports)
  return new Function(
    'exports',
    'require',
    // `useModelsStore` (and `useErrorString`, which it depends on) call the
    // real ambient `useI18n()` — no `import` in their source, same as the
    // page's own macros — so it's injected here too, not just for the page.
    'useI18n',
    ...autoImportNames,
    `${transpileFile(absPath)}\nreturn exports;`,
  )({}, req, useI18nDouble, ...autoImportNames.map((name) => autoImports[name]))
}

type InvokeHandler = (args?: unknown) => unknown

/**
 * Minimal Tauri IPC double. `invoke` dispatches by command name to a
 * per-`createChatState` handler map (defaults below, overridable per test
 * via `overrides`/`preferenceOverrides`, which replace a composable's
 * *method* — the same shape as today's real `use*()` return value — not
 * the `invoke` layer itself). `listen` just records subscribers and never
 * fires them: every one of the 19 cases below drives page state by calling
 * its returned handlers directly (e.g. `state.handleToken(...)`), so this
 * only needs to resolve without hanging or throwing.
 */
function createTauriDouble(
  invokeHandlers: Record<string, InvokeHandler>,
  invokeLog?: string[],
) {
  async function invoke(cmd: string, args?: unknown) {
    invokeLog?.push(cmd)
    const handler = invokeHandlers[cmd]
    if (!handler) {
      throw new Error(
        `check-chat-state harness: no invoke handler for '${cmd}'`,
      )
    }
    return handler(args)
  }
  async function listen(
    _event: string,
    _cb: (e: { payload: unknown }) => void,
  ) {
    return () => {}
  }
  return { invoke, listen }
}

/** Matches the page's default `createChatState()` mount flow — see the
 * one test that awaits `mount()` to completion. */
const DEFAULT_INVOKE_HANDLERS: Record<string, InvokeHandler> = {
  list_threads: () => [
    {
      id: 'a',
      title: 'New conversation',
      lastModelId: null,
      createdAt: Date.now(),
      updatedAt: Date.now(),
    },
  ],
  list_messages: () => [],
  rename_thread: (raw) => {
    const { args } = raw as { args: { threadId: string; title: string } }
    return {
      id: args.threadId,
      title: args.title,
      lastModelId: null,
      createdAt: Date.now(),
      updatedAt: Date.now(),
    }
  },
  delete_thread: () => {},
  send_message: () => ({
    threadId: 'a',
    userMessageId: 'u',
    assistantMessageId: 'answer',
  }),
  active_model_info: () => ({ modelId: 'model' }),
  model_load_status: () => ({ status: 'idle', vaultGeneration: 0 }),
  get_pref: () => null,
  list_installed_models: () => [],
  list_catalog: () => [],
  list_providers: () => [],
  close_instance: () => {},
  current_device_info: () => ({ vaultDeviceUuid: 'device' }),
  // Inert by default: `active_model_info` above already returns a truthy
  // model, so `models.ts`'s `autoLoadFirstAvailableModel()` short-circuits
  // before ever calling this — only reached by a test that overrides
  // `active_model_info` to simulate nothing being active yet.
  resolve_default_model: () => ({ modelId: null, source: 'none' }),
}

const RETURN_STATEMENT = `
    return { send, selectThread, handleToken, handleRetry, handleTurnComplete,
      historyDuration, startEditing, saveThreadTitle, cancelEditing,
      requestDelete, confirmDelete,
      handleToolPermissionRequest: chatTranscript.handleToolPermissionRequest,
      handleChoiceRequest: chatTranscript.handleChoiceRequest,
      threads, messagesByThread, activeThreadId,
      input, busy, activeModel, modelLoadPending, draftTitle,
      editingThreadId, editTitleError, deleteCandidate, deleteError,
      composerInputDisabled, sendDisabled, pendingPrompts, streamingMessageId,
      streamingThreadId, lastError,
      updatePermissionMode, permissionMode, permissionModeSaving, syncFromLocation,
      addAttachments, attachments, activePeerId };
`

/** Resolves a store module name to its source file in this checkout. */
function storePath(name: string): string {
  return resolvePath(repoRoot, 'src/stores', `${name}.ts`)
}

/** Page-level doubles a case can watch: the instances store and `navigateTo` (spec 013). */
export interface PageGlobals {
  instancesStore?: object
  navigateTo?: (to: string) => unknown
  tabRouter?: RecordingTabRouter
  /** Every `useAction(id)(input)` / `wm.runAction` call the page makes (spec 020). */
  actionLog?: { id: string; input: Record<string, unknown> }[]
  /** The tab-bound action handlers the page registers (spec 020). */
  actionHandlers?: Map<
    string,
    (context: { input: Record<string, unknown> }) => unknown
  >
}

/** A tab router double for the chat page (spec 020-tab-navigation): a real linear history of paths,
 * reactive like the window manager's, with every call recorded in `log` (`push /x`, `replace /y`, `back`). */
export type RecordingTabRouter = ReturnType<typeof createRecordingTabRouter>

export function createRecordingTabRouter() {
  const entries = ['/']
  let index = 0
  const state = reactive({
    route: {
      path: '/',
      query: {} as Record<string, string>,
      params: {} as Record<string, string>,
      matched: [] as { path: string }[],
    },
    canGoBack: false,
    canGoForward: false,
    log: [] as string[],
    entries,
  })
  function sync() {
    state.route.path = entries[index] ?? '/'
    state.canGoBack = index > 0
    state.canGoForward = index < entries.length - 1
  }
  return Object.assign(state, {
    push(to: string) {
      state.log.push(`push ${to}`)
      if (to === entries[index]) return
      entries.splice(index + 1)
      entries.push(to)
      index = entries.length - 1
      sync()
    },
    replace(to: string) {
      state.log.push(`replace ${to}`)
      entries[index] = to
      sync()
    },
    setQuery() {},
    back() {
      state.log.push('back')
      if (index > 0) index -= 1
      sync()
    },
    forward() {
      state.log.push('forward')
      if (index < entries.length - 1) index += 1
      sync()
    },
    skipCurrent() {
      state.log.push('skip')
      if (entries.length <= 1) return false
      entries.splice(index, 1)
      index = Math.max(0, index - 1)
      sync()
      return true
    },
  })
}

/** Boots the real chat page dependencies inside an isolated test sandbox. */
export function createChatState(
  overrides: Record<string, unknown> = {},
  preferenceOverrides: Record<string, unknown> = {},
  dependencyOverrides: Record<string, (...args: unknown[]) => unknown> = {},
  invokeLog?: string[],
  pageGlobals: PageGlobals = {},
) {
  const tauri = createTauriDouble({ ...DEFAULT_INVOKE_HANDLERS }, invokeLog)

  const mountHooks: Array<() => unknown> = []
  const unmountHooks: Array<() => unknown> = []
  const vueDouble = {
    ...(nodeRequire('vue') as object),
    onMounted: (hook: () => unknown) => mountHooks.push(hook),
    onBeforeUnmount: (hook: () => unknown) => unmountHooks.push(hook),
  }

  /**
   * Resolves imports for the sandbox while sharing overridable chat and
   * preference instances with the model store. This hoisted declaration is
   * called only after those instances have been initialized below.
   */
  function req(specifier: string) {
    if (specifier === 'vue') return vueDouble
    if (specifier === 'pinia') return nodeRequire('pinia')
    if (specifier === '@tauri-apps/api/core') return { invoke: tauri.invoke }
    if (specifier === '@tauri-apps/api/event') return { listen: tauri.listen }
    if (specifier === 'dompurify') {
      const identity = (html: string) => html
      return { default: { sanitize: identity }, sanitize: identity }
    }
    if (specifier === 'marked') return nodeRequire('marked')
    if (specifier === '~/composables/useChat') return { useChat: () => chat }
    if (specifier === '~/composables/usePreferences') {
      // `usePreferences()` itself is overridden below to share the one
      // `preferences` instance every consumer in this sandbox sees, but its
      // other named exports (`AUTONOMY_MODES`/`isAutonomyMode`) are plain,
      // side-effect-free values — running the real module for those costs
      // nothing and means the page's replayed autonomy-mode validation
      // exercises the actual shared helper instead of a hand-duplicated one.
      const real = runComposable(composablePath('usePreferences'), req)
      return { ...real, usePreferences: () => preferences }
    }
    if (specifier.startsWith('~/composables/')) {
      const name = specifier.slice('~/composables/'.length)
      const real = runComposable(composablePath(name), req, {
        onVaultTablesChanged,
        ...(name === 'useErrorString'
          ? { hfErrorKey: req('~/composables/useHuggingFace').hfErrorKey }
          : {}),
      })
      const override = dependencyOverrides[name]
      return override ? { ...real, [name]: override } : real
    }
    // Pure TS shared with the check scripts (the notice logic); runs as it is.
    if (specifier === '~/lib/chat/toolNotice')
      return runComposable(
        resolvePath(repoRoot, 'src/lib/chat/toolNotice.ts'),
        req,
      )
    // Marketplace error keys are a pure helper imported by useErrorString.
    if (specifier === '~/lib/marketplace/errors')
      return runComposable(
        resolvePath(repoRoot, 'src/lib/marketplace/errors.ts'),
        req,
      )
    // Used only inside an action handler (`chat.messages.list`), which these tests never call.
    if (specifier === '~/lib/actions/agentTools')
      return { agentSafeMessages: () => ({ messages: [] }) }
    // Only referenced as <template> tag names — never executed by the
    // replayed script-setup body — so a dead stub is enough.
    if (specifier.startsWith('~/components/')) return {}
    throw new Error(
      `check-chat-state harness: unexpected import '${specifier}'`,
    )
  }

  // The real subscriber hub behind `onVaultTablesChanged`, so a test can fire a change.
  const vaultData = createVaultDataHub((error) => {
    throw error
  })
  const onVaultTablesChanged = (
    tables: readonly string[],
    handler: () => void | Promise<void>,
  ) => vaultData.subscribe({ tables, handler })

  const chat = {
    ...runComposable(composablePath('useChat'), req).useChat(),
    ...overrides,
  }
  const preferences = {
    ...runComposable(composablePath('usePreferences'), req).usePreferences(),
    ...preferenceOverrides,
  }

  // A fresh Pinia per test, matching every other piece of state here.
  const pinia = nodeRequire('pinia')
  pinia.setActivePinia(pinia.createPinia())
  const modelStore = runComposable(storePath('models'), req, {
    onVaultTablesChanged,
    useChat: () => chat,
    useModels: () => req('~/composables/useModels').useModels(),
    useCatalog: () => req('~/composables/useCatalog').useCatalog(),
    useProviders: () => req('~/composables/useProviders').useProviders(),
    useErrorString: () => req('~/composables/useErrorString').useErrorString(),
    usePreferences: () => preferences,
    useDevice: () => req('~/composables/useDevice').useDevice(),
    // A desktop whose capability table has not arrived: every facility counts as present.
    useDeviceCapabilities: () => ({
      capabilities: { value: null },
      isAndroid: { value: false },
      readyAsync: async () => null,
    }),
    parseModelIntegrityFailure: req('~/composables/useModels')
      .parseModelIntegrityFailure,
  }).useModelsStore()

  // These have no `import` statement in the page at all — real Nuxt
  // auto-imports/macros with no module backing here — so they must be
  // injected as bare names in scope rather than resolved through `require`.
  const state = new Function(
    'exports',
    'require',
    'definePageMeta',
    'useRoute',
    'useI18n',
    'useChat',
    'useInstance',
    'usePreferences',
    'useDevice',
    'useErrorString',
    'useChatTranscript',
    'useThreadSidebar',
    'useComposer',
    'useComposerAttachments',
    'useChatPermissionMode',
    'registerChatSubscriptions',
    'useInstancesStore',
    'useWindowManagerStore',
    'useWmTab',
    'useTabRouter',
    'useChatNavigation',
    'useAction',
    'useChatTab',
    'useToolNotice',
    'onVaultTablesChanged',
    'useModelsStore',
    'storeToRefs',
    'useAgentModelState',
    'useConversations',
    'navigateTo',
    'document',
    `${pageCode}\n${RETURN_STATEMENT}`,
  )(
    {},
    req,
    () => {},
    () => ({ params: { instance: 'vault' } }),
    useI18nDouble,
    () => chat,
    req('~/composables/useInstance').useInstance,
    () => preferences,
    req('~/composables/useDevice').useDevice,
    req('~/composables/useErrorString').useErrorString,
    req('~/composables/useChatTranscript').useChatTranscript,
    req('~/composables/useThreadSidebar').useThreadSidebar,
    req('~/composables/useComposer').useComposer,
    req('~/composables/useComposerAttachments').useComposerAttachments,
    req('~/composables/useChatPermissionMode').useChatPermissionMode,
    req('~/composables/useChatSubscriptions').registerChatSubscriptions,
    () => pageGlobals.instancesStore ?? {},
    () => ({
      flushAsync: async () => {},
      openApp: () => {},
      runAction: async (id: string, input: Record<string, unknown> = {}) => {
        pageGlobals.actionLog?.push({ id, input })
        return { ok: true, result: null }
      },
    }),
    () => ({
      tabId: '',
      windowId: '',
      requestAttention: () => {},
      clearAttention: () => {},
      setTitle: () => {},
      registerCloseGuard: () => () => {},
      closeSelf: () => {},
      appId: 'system.chat',
      openApp: () => {},
      registerActionHandler: (
        id: string,
        handler: (context: { input: Record<string, unknown> }) => unknown,
      ) => {
        pageGlobals.actionHandlers?.set(id, handler)
        return () => pageGlobals.actionHandlers?.delete(id)
      },
    }),
    () => pageGlobals.tabRouter ?? createRecordingTabRouter(),
    req('~/composables/useChatNavigation').useChatNavigation,
    (id: string) =>
      async (input: Record<string, unknown> = {}) => {
        pageGlobals.actionLog?.push({ id, input })
        return { ok: true, result: null }
      },
    req('~/composables/useChatTab').useChatTab,
    req('~/composables/useToolNotice').useToolNotice,
    onVaultTablesChanged,
    () => modelStore,
    pinia.storeToRefs,
    runComposable(composablePath('useAgentModelState'), req, {
      useModelsStore: () => modelStore,
      storeToRefs: pinia.storeToRefs,
      useAction:
        (id: string) =>
        async (input: Record<string, unknown> = {}) => {
          pageGlobals.actionLog?.push({ id, input })
          return { ok: true, result: null }
        },
    }).useAgentModelState,
    // Spec 072: no contact conversations in these cases; the list stays empty.
    () => {
      const { ref } = nodeRequire('vue') as typeof import('vue')
      return {
        conversations: ref([]),
        contacts: ref([]),
        error: ref(null),
        refresh: async () => {},
        listAsync: async () => [],
        openDirectAsync: async () => ({ threadId: '' }),
        muteAsync: async () => {},
        deleteAsync: async () => {},
      }
    },
    pageGlobals.navigateTo ?? (() => {}),
    { querySelector: () => null },
  )
  // Existing send-flow tests model a ready chat session unless they override it.
  state.activeModel.value = {
    modelId: 'model',
    name: 'Model',
    tokenizerRepo: 'tokenizer',
    contextWindow: null,
  }

  return {
    ...state,
    /** Announces a change of `tables` as the backend's `vault-data-changed` event does. */
    emitVaultChange: (tables: string[]) => vaultData.dispatch(tables),
    modelStore,
    mount: () => Promise.all(mountHooks.map((hook) => hook())),
    unmount: () => Promise.all(unmountHooks.map((hook) => hook())),
  }
}

/** Lets pending promise chains and Vue watchers run to completion. */
export async function flush() {
  for (let i = 0; i < 12; i++) {
    await Promise.resolve()
    await nextTick()
  }
}
