import { invoke } from '@tauri-apps/api/core'
import {
  useAppearance,
  type AppearancePatch,
} from '~/composables/useAppearance'
import { useColorScheme } from '~/composables/useColorScheme'
import { useDevice } from '~/composables/useDevice'
import { useHuggingFace } from '~/composables/useHuggingFace'
import { useLanguage } from '~/composables/useLanguage'
import { useModels } from '~/composables/useModels'
import { usePreferences, type PrefScope } from '~/composables/usePreferences'
import { useProviders, type DelegateVendor } from '~/composables/useProviders'
import { useSync } from '~/composables/useSync'
import { useSttModels } from '~/composables/useSttModels'
import { useWorkspaceBackground } from '~/composables/useWorkspaceBackground'
import { OPENAI_COMPATIBLE } from '~/lib/models/servers'
import { parseTokens } from '~/lib/models/contextSetting'
import { parseColorScheme } from '~/lib/settings/colorScheme'
import { parseLanguage } from '~/lib/settings/language'
import { useModelDownloadsStore } from '~/stores/modelDownloads'
import type { useWindowManagerStore } from '~/stores/windowManager'
import type { AgentProfileDeleted } from '~/types/bindings/AgentProfileDeleted'
import type { BridgeOverview } from '~/types/bindings/BridgeOverview'
import type { McpStatus } from '~/types/bindings/McpStatus'

type WmStore = ReturnType<typeof useWindowManagerStore>

/** Preference keys the settings components read (specs 002, 008). */
const STT_MODEL_KEY = 'voice.stt_model_id'

/**
 * Global handlers of the settings actions (spec 020-tab-navigation, T052,
 * `lib/actions/settingsActions.ts`). They hold the write logic the settings
 * components used to run themselves, so a setting changes the same way from
 * the UI, a shortcut or an agent, with or without the settings window open.
 */
export function registerSettingsActionHandlers(wm: WmStore): void {
  const {
    currentDeviceInfoAsync,
    updateDeviceAliasAsync,
    listVaultDevicesAsync,
  } = useDevice()
  const { getPrefAsync, setPrefAsync } = usePreferences()
  const models = useModels()
  const huggingFace = useHuggingFace()
  const providers = useProviders()
  const sttModels = useSttModels()
  const downloads = useModelDownloadsStore()
  const colorScheme = useColorScheme()
  const appearance = useAppearance()
  const language = useLanguage()
  const background = useWorkspaceBackground()
  const sync = useSync()
  const done = { done: true }
  const on = wm.registerGlobalActionHandler

  async function deviceScope(): Promise<PrefScope> {
    const device = await currentDeviceInfoAsync()
    return { kind: 'device', uuid: device.vaultDeviceUuid }
  }

  on('settings.get', async () => {
    const device = await currentDeviceInfoAsync()
    const scope: PrefScope = { kind: 'device', uuid: device.vaultDeviceUuid }
    const [stt, sessionRestore] = await Promise.all([
      getPrefAsync(scope, STT_MODEL_KEY),
      wm.getSessionRestore(),
    ])
    return {
      deviceAlias: device.alias,
      language: language.language.value,
      colorScheme: colorScheme.scheme.value,
      appearance: appearance.appearance.value,
      sttModel: stt,
      sessionRestore: sessionRestore.enabled,
    }
  })
  on('settings.devices.list', async () => ({
    devices: (await listVaultDevicesAsync()).map((device) => ({
      vaultDeviceUuid: device.vaultDeviceUuid,
      ...(device.alias === null ? {} : { alias: device.alias }),
      isCurrent: device.isCurrent,
      role: device.role,
      online: device.online,
      ...(device.lastSeen === null ? {} : { lastSeen: device.lastSeen }),
      ...(device.problem === null ? {} : { problem: device.problem }),
      ...(device.schema === null ? {} : { schema: device.schema }),
    })),
  }))
  on('settings.devices.identity', async () => {
    const identity = await sync.vaultPublicIdentityAsync()
    return { npub: identity.npub, hex: identity.hex }
  })
  on('settings.sync.servers.set', async ({ input }) => {
    await sync.syncServersSetAsync({
      nostrRelays: input.nostrRelays as string[],
      irohRelays: input.irohRelays as string[],
      disabled: input.disabled as string[],
    })
    return done
  })
  on('settings.devices.remove', async ({ input }) => {
    await sync.deviceRemoveAsync(input.devicePubkey as string)
    return done
  })
  on('settings.devices.admit', async ({ input }) => {
    await sync.admissionDecideAsync(
      input.devicePubkey as string,
      input.admit as boolean,
    )
    return done
  })
  on('settings.devices.link', () => {
    wm.openApp('system.settings', '/federation/devices/link')
    return done
  })
  on('settings.models.list', async () => ({
    models: (await models.listInstalledAsync()).map((model) => ({
      modelId: model.id,
      name: model.name,
      sizeBytes: model.sizeBytes,
    })),
  }))
  on('settings.models.checkUpdates', async () => ({
    statuses: await huggingFace.checkUpdatesAsync(),
  }))

  on('settings.privacy.screenCapture.set', async ({ input }) => ({
    enabled: await invoke<boolean>('screen_capture_protection_set', {
      enabled: input.enabled === true,
    }),
  }))
  on('settings.files.deviceAccess.set', async ({ input }) => ({
    enabled: await invoke<boolean>('files_device_access_set', {
      enabled: input.enabled === true,
    }),
  }))
  // Spec 049: the External Bridge of this device; Rust validates the port.
  on('settings.browserBridge.set', async ({ input }) =>
    invoke<BridgeOverview>('browser_bridge_configure', {
      args: { enabled: input.enabled === true, port: Number(input.port) },
    }),
  )
  // Rust checks the grant against the client's declaration (FR-010, FR-038).
  on('settings.browserBridge.client.update', async ({ input }) =>
    invoke<BridgeOverview>('browser_bridge_client_update', {
      args: {
        clientId: String(input.clientId),
        access: input.access === 'readWrite' ? 'readWrite' : 'read',
        scopeTags: Array.isArray(input.scopeTags)
          ? input.scopeTags.map(String)
          : null,
        defaultTag:
          typeof input.defaultTag === 'string' ? input.defaultTag : null,
      },
    }),
  )
  on('settings.browserBridge.client.revoke', async ({ input }) =>
    invoke<BridgeOverview>('browser_bridge_client_revoke', {
      args: { clientId: String(input.clientId) },
    }),
  )
  // Spec 021: the MCP access of this device; Rust starts or stops it.
  on('settings.agents.mcp.enable', async ({ input }) =>
    invoke<McpStatus>('mcp_configure', {
      args: { enabled: input.enabled === true },
    }),
  )
  const agent = (input: Record<string, unknown>) => ({
    agentId: String(input.agentId),
  })
  on('settings.agents.mcp.revoke', async ({ input }) =>
    invoke('mcp_agent_revoke', { args: agent(input) }),
  )
  on('settings.agents.mcp.unblock', async ({ input }) =>
    invoke('mcp_agent_unblock', { args: agent(input) }),
  )
  on('settings.agents.mcp.log.clear', async ({ input }) =>
    invoke('mcp_agent_log_clear', { args: agent(input) }),
  )
  // Spec 083: agent profiles. The page reloads from `mcp-agents-changed`, which Rust emits.
  on('settings.agents.profiles.create', async ({ input }) =>
    invoke('agent_profile_create', { args: { name: String(input.name) } }),
  )
  on('settings.agents.profiles.rename', async ({ input }) =>
    invoke('agent_profile_rename', {
      args: { id: String(input.id), name: String(input.name) },
    }),
  )
  on('settings.agents.profiles.delete', async ({ input }) =>
    invoke<AgentProfileDeleted>('agent_profile_delete', {
      args: { id: String(input.id) },
    }),
  )
  on('settings.agents.profiles.rights.set', async ({ input }) =>
    invoke('agent_profile_rights_set', {
      args: { id: String(input.id), rights: input.rights },
    }),
  )
  on('settings.agents.profiles.assign', async ({ input }) =>
    invoke('agent_membership_set', {
      args: {
        subject: String(input.subject),
        profileId: String(input.profileId),
        member: input.member === true,
      },
    }),
  )
  on('settings.agents.profiles.activate', async ({ input }) =>
    invoke('agent_profile_active_set', {
      args: { profileId: String(input.profileId) },
    }),
  )
  // Spec 065: the agent browser.
  on('settings.agentBrowser.path.set', async ({ input }) =>
    invoke('agent_browser_path_set', {
      args: { path: (input as { path: string | null }).path ?? null },
    }),
  )
  on('settings.agentBrowser.profile.cookie.set', async ({ input }) => {
    const { cookie, value, expires } = input as {
      cookie: unknown
      value: string
      expires?: number | null
    }
    return invoke('agent_browser_cookie_set', {
      args: { cookie, value, expires: expires ?? null },
    })
  })
  on('settings.agentBrowser.profile.cookie.delete', async ({ input }) =>
    invoke<number>('agent_browser_cookie_delete', {
      args: { target: (input as { target: unknown }).target },
    }),
  )
  on('settings.agentBrowser.trackers.set', async ({ input }) =>
    invoke('agent_browser_trackers_set', {
      args: { settings: (input as { settings: unknown }).settings },
    }),
  )
  on('settings.agentBrowser.dns.set', async ({ input }) =>
    invoke('agent_browser_dns_set', {
      args: { settings: (input as { settings: unknown }).settings },
    }),
  )
  on('settings.agentBrowser.profile.clear', async () =>
    invoke('agent_browser_profile_clear'),
  )
  on('settings.browserBridge.client.unblock', async ({ input }) =>
    invoke<BridgeOverview>('browser_bridge_client_unblock', {
      args: { clientId: String(input.clientId) },
    }),
  )
  on('settings.sessionRestore.set', async ({ input }) =>
    wm.setSessionRestore(input.enabled === true),
  )
  on('settings.general.setLanguage', async ({ input }) => {
    const next = parseLanguage(input.language)
    if (!next) throw new Error(`unknown language ${String(input.language)}`)
    return { language: await language.setAsync(next) }
  })
  on('settings.appearance.setColorScheme', async ({ input }) => {
    const scheme = parseColorScheme(input.scheme)
    if (!scheme) throw new Error(`unknown color scheme ${String(input.scheme)}`)
    return { scheme: await colorScheme.setAsync(scheme) }
  })

  on('settings.appearance.removeBackground', async () => {
    await background.removeAsync()
    return done
  })

  /** What the appearance actions answer: the stored appearance and what had to be adjusted. */
  const appearanceResult = (stored: unknown) => ({
    appearance: stored,
    adjustments: [...appearance.adjustments.value],
  })
  on('settings.appearance.set', async ({ input }) =>
    appearanceResult(await appearance.setAsync(input as AppearancePatch)),
  )
  on('settings.appearance.reset', async () =>
    appearanceResult(await appearance.resetAsync()),
  )
  on('settings.appearance.export', async () => ({
    file: appearance.exportText(),
  }))
  on('settings.appearance.import', async ({ input }) =>
    appearanceResult(await appearance.importAsync(String(input.file ?? ''))),
  )

  on('settings.device.setAlias', async ({ input }) => {
    const alias = String(input.alias).trim()
    if (!alias) throw new Error('the device name must not be empty')
    await updateDeviceAliasAsync(alias)
    return done
  })
  on('settings.models.setStt', async ({ input }) => {
    const catalogId = String(input.catalogId)
    // The download reports progress to the shared store; forget it whoever started the action.
    const installed = await sttModels
      .downloadFromCatalogAsync(catalogId)
      .finally(() => downloads.clearDownload(catalogId))
    await setPrefAsync(await deviceScope(), STT_MODEL_KEY, installed.id)
    await invoke('invalidate_stt_model_cache')
    return { modelId: installed.id }
  })
  on('settings.models.downloadCatalog', async ({ input }) => {
    await models.downloadFromCatalogAsync(String(input.entryId))
    return done
  })
  on('settings.models.cancelDownload', async ({ input }) => {
    await models.cancelDownloadAsync(String(input.modelId))
    return done
  })
  on('settings.models.downloadFromHf', async ({ input }) =>
    models.downloadFromHfAsync({
      repoId: String(input.repoId),
      filename: String(input.filename),
      revision: typeof input.revision === 'string' ? input.revision : undefined,
      name:
        typeof input.name === 'string' ? input.name : String(input.filename),
      tokenizerRepo:
        typeof input.tokenizerRepo === 'string'
          ? input.tokenizerRepo
          : undefined,
      contextWindow:
        typeof input.contextWindow === 'number'
          ? input.contextWindow
          : undefined,
      forceTooBig: input.forceTooBig === true,
    }),
  )
  on('settings.models.installUpdate', async ({ input }) => {
    await huggingFace.installUpdateAsync(String(input.modelId))
    return done
  })
  on('settings.models.delete', async ({ input }) => {
    await models.deleteAsync(String(input.modelId))
    return done
  })
  // The models store calls `useI18n()` in its setup, which only works inside a component setup:
  // resolve it per call (the workspace page has created it by then), never at registration.
  on('settings.models.activate', async ({ input }) => {
    const store = useModelsStore()
    store.lastError = null
    store.integrityActionError = null
    const modelId = String(input.modelId)
    await store.activateLocalModel(modelId)
    const error = store.lastError ?? store.integrityActionError
    if (error) throw new Error(error)
    // A failed integrity check opens the decision dialog and sets no error: the model is not
    // active yet, and only the person decides there.
    if (store.integrityDialog?.modelId === modelId) {
      throw new Error(
        'The integrity check of the model failed; the person has to decide in the integrity dialog.',
      )
    }
    return done
  })
  on('settings.models.deactivate', async () => {
    const store = useModelsStore()
    store.lastError = null
    await store.deactivateLocalModel()
    if (store.lastError) throw new Error(store.lastError)
    return done
  })
  // The page keeps the length as text (`auto` or digits); the command takes `"auto"` or a number.
  const contextWire = (input: Record<string, unknown>) => {
    const context =
      input.context === 'auto'
        ? 'auto'
        : typeof input.context === 'string' && /^\d+$/.test(input.context)
          ? parseTokens(input.context)
          : null
    if (context === null)
      throw new Error(
        'Context must be "auto" or a positive 32-bit number of tokens.',
      )
    return {
      modelId: String(input.modelId),
      setting: { context, cache: String(input.cache) },
    }
  }
  const contextOf = (setting: { context: string | number; cache: string }) => ({
    context: String(setting.context),
    cache: setting.cache,
  })
  on('settings.models.context.get', async ({ input }) =>
    contextOf(
      await invoke('get_model_context', { modelId: String(input.modelId) }),
    ),
  )
  on('settings.models.context.set', async ({ input }) =>
    contextOf(await invoke('set_model_context', contextWire(input))),
  )
  on('settings.models.context.report', async ({ input }) =>
    invoke('model_context_report', contextWire(input)),
  )
  on('settings.delegate.refreshModels', async ({ input }) => {
    await providers.refreshModelsAsync(String(input.providerId))
    return done
  })
  on('settings.servers.add', async ({ input }) => {
    const added = await providers.addAsync({
      kind: 'api_key',
      adapter: OPENAI_COMPATIBLE,
      name: String(input.name),
      baseUrl: String(input.baseUrl),
      apiKey: typeof input.apiKey === 'string' ? input.apiKey : undefined,
    })
    return {
      providerId: added.provider.id,
      ...(added.modelCount === null ? {} : { modelCount: added.modelCount }),
      ...(added.refreshError === null
        ? {}
        : { refreshError: added.refreshError }),
    }
  })
  on('settings.servers.remove', async ({ input }) => {
    const providerId = String(input.providerId)
    const provider = (await providers.listAsync()).find(
      (candidate) => candidate.id === providerId,
    )
    if (provider?.adapter !== OPENAI_COMPATIBLE) {
      throw new Error('the provider is not an OpenAI-compatible model server')
    }
    await providers.deleteAsync(providerId)
    return done
  })
  on('settings.servers.setModelCapabilities', async ({ input }) => {
    await providers.setModelCapabilitiesAsync({
      modelId: String(input.modelId),
      acceptsImages:
        typeof input.acceptsImages === 'boolean'
          ? input.acceptsImages
          : undefined,
      effortLevels: Array.isArray(input.effortLevels)
        ? input.effortLevels.map(String)
        : undefined,
    })
    return done
  })
  on('settings.delegate.connectProvider', async ({ input }) => {
    const result = await providers.connectCliDelegateAsync({
      vendor: input.vendor as DelegateVendor,
      name: String(input.name),
    })
    return { status: result.status }
  })
  on('settings.delegate.submitCode', async ({ input }) => {
    await providers.submitCliDelegateCodeAsync({
      code: String(input.code),
      name: String(input.name),
    })
    return done
  })
}
