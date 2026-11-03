import { computed, ref } from 'vue'
import type {
  InstalledModel,
  ModelCapabilities,
  useModels,
} from '~/composables/useModels'
import type { CatalogEntryWithFit, useCatalog } from '~/composables/useCatalog'
import type {
  Provider,
  ProviderModel,
  useProviders,
} from '~/composables/useProviders'

export type ModelGroup = {
  providerId: string
  providerName: string
  models: { id: string; name: string; disabled?: boolean }[]
}

/** The subset of vue-i18n's `t` this module needs. */
export type Translate = (key: string, named?: Record<string, unknown>) => string

/**
 * The two `cli_delegate` vendors (spec 007-cli-delegate). Always shown in
 * the picker, connected or not — unlike `api_key` providers, which only
 * appear once a row (and models) exist, a delegate vendor is a fixed,
 * known option the user can discover before ever connecting one
 * (spec.md FR-004, Acceptance Scenario 2).
 */
const DELEGATE_VENDORS = ['claude', 'codex'] as const

export interface ModelInventoryDeps {
  models: ReturnType<typeof useModels>
  catalog: ReturnType<typeof useCatalog>
  providers: ReturnType<typeof useProviders>
  t: Translate
  errString: (error: unknown) => string
  /** Reports a non-fatal failure through the owning store's error state. */
  setError: (message: string) => void
  /** Whether this device runs CLI delegates (spec 043 FR-016); without them the picker shows none. */
  cliDelegates?: () => boolean
  /** The active local model: the only local one the picker offers. */
  activeLocalModelId: () => string | null
}

/**
 * Installed/catalog/provider model lists and the picker's grouping over
 * them, extracted from `useModelsStore` (spaex 500-LoC boundary). Takes its
 * collaborators as parameters instead of calling Nuxt auto-imports, so the
 * store stays the only place that wires composables together.
 */
export function useModelInventory(deps: ModelInventoryDeps) {
  const { models, catalog, providers, t, errString, setError } = deps
  const delegatesHere = () => deps.cliDelegates?.() ?? true

  const installedModels = ref<InstalledModel[]>([])
  const catalogEntries = ref<CatalogEntryWithFit[]>([])
  const providerList = ref<Provider[]>([])
  const providerModels = ref<Record<string, ProviderModel[]>>({})

  // A connected `cli_delegate` provider's one cached model
  // (`refreshProviders` below) already lands in `providerModels`, the
  // same way an `api_key` provider's models do — so this check needs no
  // separate delegate-specific case.
  const noModelsInstalled = computed(
    () =>
      installedModels.value.length === 0 &&
      Object.values(providerModels.value).every((list) => list.length === 0),
  )

  /** Groups selectable models by provider for the picker's <optgroup>. */
  const modelGroups = computed<ModelGroup[]>(() => {
    // Of the installed local models only the active one is in memory, so only it is a choice;
    // the settings activate another one.
    const activeLocal = installedModels.value.filter(
      (m) => m.id === deps.activeLocalModelId(),
    )
    const localGroup: ModelGroup | null =
      activeLocal.length > 0
        ? {
            providerId: 'local',
            providerName: t('chat.model.local'),
            models: activeLocal.map((m) => ({
              id: m.id,
              name: m.name,
            })),
          }
        : null

    // A connected `cli_delegate` provider gets real cached model rows
    // (`<providerId>:<remoteId>`) via the same `list_models`/
    // `replace_provider_models` refresh path `api_key` providers already
    // use (`providers/mod.rs::compose_model_row`,
    // `adapters/cli_delegate/mod.rs::list_models`) — so it flows through
    // `remoteGroups` unchanged, no separate synthesis needed for the
    // connected case. Claude's rows come straight from Anthropic's own
    // `/v1/models` API (e.g. `<uuid>:claude-opus-5`), Codex still gets one
    // synthetic `<uuid>:codex` row.
    const remoteGroups = providerList.value
      .filter(
        (p) =>
          p.kind === 'api_key' ||
          (p.kind === 'cli_delegate' && delegatesHere()),
      )
      .map<ModelGroup>((p) => ({
        providerId: p.id,
        providerName: p.name,
        models: (providerModels.value[p.id] ?? []).map((m) => ({
          id: m.id,
          name: m.name,
        })),
      }))
      .filter((g) => g.models.length > 0)

    // Unlike `api_key`, a `cli_delegate` vendor the user hasn't connected
    // yet has no `providers` row at all — nothing for `remoteGroups`
    // above to find. Shown anyway, disabled, so it's discoverable
    // (spec.md FR-004, Acceptance Scenario 2); connecting happens from
    // Settings (tasks.md T027), not from this picker.
    const notConnectedDelegateGroups: ModelGroup[] = DELEGATE_VENDORS.filter(
      (vendor) =>
        delegatesHere() &&
        !providerList.value.some(
          (p) =>
            p.kind === 'cli_delegate' &&
            p.adapter === vendor &&
            p.hasCredentials,
        ),
    ).map((vendor) => {
      const label = t(`chat.model.delegate.${vendor}`)
      return {
        providerId: `delegate-${vendor}`,
        providerName: label,
        models: [
          {
            id: `delegate-${vendor}:not-connected`,
            name: t('chat.model.delegateNotConnected'),
            disabled: true,
          },
        ],
      }
    })

    return localGroup
      ? [localGroup, ...remoteGroups, ...notConnectedDelegateGroups]
      : [...remoteGroups, ...notConnectedDelegateGroups]
  })

  /** Looks up a picker entry's friendly name across every group. */
  function findModelName(id: string): string {
    for (const group of modelGroups.value) {
      const found = group.models.find((m) => m.id === id)
      if (found) return found.name
    }
    // An installed local model that is not active is no choice in the picker, but has a name.
    return installedModels.value.find((m) => m.id === id)?.name ?? id
  }

  /**
   * The cached capabilities of one picker model, straight from the lists the
   * store already fetched — no IPC and no second cache. `undefined` means no
   * row matches (nothing selected, lists still loading, the disabled
   * "not connected" placeholder); `null` means a row exists whose
   * capabilities are not determined.
   */
  function capabilitiesFor(
    modelId: string,
  ): ModelCapabilities | null | undefined {
    const installed = installedModels.value.find((m) => m.id === modelId)
    if (installed) return installed.capabilities ?? null
    for (const list of Object.values(providerModels.value)) {
      const found = list.find((m) => m.id === modelId)
      if (found) return found.capabilities ?? null
    }
    return undefined
  }

  /** Refreshes the installed models and their catalog metadata together. */
  async function refreshInstalledAndCatalog() {
    installedModels.value = await models.listInstalledAsync()
    catalogEntries.value = await catalog.listAsync()
  }

  /**
   * Refreshes the provider list and re-fetches api_key/cli_delegate model
   * caches. Each provider's fetch is isolated: one provider being
   * unreachable (expired delegate token, network blip) must not blank out
   * every other provider's already-known models or abort the caller's
   * broader `initialize()` sequence.
   */
  async function refreshProviders() {
    providerList.value = await providers.listAsync()
    const relevant = providerList.value.filter(
      (p) => p.kind === 'api_key' || p.kind === 'cli_delegate',
    )
    const next: Record<string, ProviderModel[]> = {}
    const results = await Promise.allSettled(
      relevant.map(async (p) => {
        next[p.id] = await providers.listModelsAsync(p.id)
      }),
    )
    // A provider whose fetch failed this round keeps its last-known models
    // instead of being blanked out — `next` only ever holds currently
    // relevant providers, so one no longer connected still drops out below.
    for (const p of relevant) {
      if (!(p.id in next)) next[p.id] = providerModels.value[p.id] ?? []
    }
    const failure = results.find((r) => r.status === 'rejected') as
      PromiseRejectedResult | undefined
    if (failure) setError(errString(failure.reason))
    providerModels.value = next
  }

  return {
    installedModels,
    catalogEntries,
    providerList,
    providerModels,
    noModelsInstalled,
    modelGroups,
    findModelName,
    capabilitiesFor,
    refreshInstalledAndCatalog,
    refreshProviders,
  }
}
