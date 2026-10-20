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
import { parseColorScheme } from '~/lib/settings/colorScheme'
import { parseLanguage } from '~/lib/settings/language'
import type { useWindowManagerStore } from '~/stores/windowManager'

type WmStore = ReturnType<typeof useWindowManagerStore>

/** Preference keys the settings components read (specs 002, 008, 009). */
const DEFAULT_MODEL_KEY = 'chat.default_model_id'
const STT_MODEL_KEY = 'voice.stt_model_id'
const AUTONOMY_KEY = 'chat.autonomy_mode'
const DENY_RULES_KEY = 'cli_delegate.deny_rules'

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
  const { getPrefAsync, setPrefAsync, clearPrefAsync } = usePreferences()
  const models = useModels()
  const huggingFace = useHuggingFace()
  const providers = useProviders()
  const sttModels = useSttModels()
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
  /** Settings apply to the vault; only the default and speech models to this device (spec 023,
   * FR-024). */
  const VAULT: PrefScope = { kind: 'vault' }

  on('settings.get', async () => {
    const device = await currentDeviceInfoAsync()
    const scope: PrefScope = { kind: 'device', uuid: device.vaultDeviceUuid }
    const [defaultModel, stt, autonomy, denyRules, sessionRestore] =
      await Promise.all([
        getPrefAsync(scope, DEFAULT_MODEL_KEY),
        getPrefAsync(scope, STT_MODEL_KEY),
        getPrefAsync(VAULT, AUTONOMY_KEY),
        getPrefAsync(VAULT, DENY_RULES_KEY),
        wm.getSessionRestore(),
      ])
    return {
      deviceAlias: device.alias,
      language: language.language.value,
      colorScheme: colorScheme.scheme.value,
      appearance: appearance.appearance.value,
      ...(defaultModel ? { defaultModel } : {}),
      sttModel: stt,
      sessionRestore: sessionRestore.enabled,
      autonomyMode: autonomy,
      delegateDenyRules: denyRules ? (JSON.parse(denyRules) as unknown) : [],
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
  on('settings.models.setDefault', async ({ input }) => {
    await setPrefAsync(
      await deviceScope(),
      DEFAULT_MODEL_KEY,
      String(input.modelId),
    )
    return done
  })
  on('settings.models.clearDefault', async () => {
    await clearPrefAsync(await deviceScope(), DEFAULT_MODEL_KEY)
    return done
  })
  on('settings.models.setStt', async ({ input }) => {
    const installed = await sttModels.downloadFromCatalogAsync(
      String(input.catalogId),
    )
    await setPrefAsync(await deviceScope(), STT_MODEL_KEY, installed.id)
    await invoke('invalidate_stt_model_cache')
    return { modelId: installed.id }
  })
  on('settings.models.downloadCatalog', async ({ input }) => {
    await models.downloadFromCatalogAsync(String(input.entryId))
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
    await providers.deleteAsync(String(input.providerId))
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
  on('settings.autonomy.setMode', async ({ input }) => {
    await setPrefAsync(VAULT, AUTONOMY_KEY, String(input.mode))
    return done
  })
  on('settings.delegate.setDenyRules', async ({ input }) => {
    await setPrefAsync(VAULT, DENY_RULES_KEY, JSON.stringify(input.rules))
    return done
  })
  on('settings.agents.files.set', async ({ input }) => {
    await invoke('files_agent_permission_set', {
      agentId: input.agentId,
      kind: input.kind,
      target: input.target,
      status: input.status,
    })
    return done
  })
  on('settings.agents.files.remove', async ({ input }) => {
    await invoke('files_agent_permission_remove', {
      agentId: input.agentId,
      kind: input.kind,
      target: input.target,
    })
    return done
  })
}
