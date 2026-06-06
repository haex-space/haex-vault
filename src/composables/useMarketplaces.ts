import { ref } from 'vue'
import { eq, asc } from 'drizzle-orm'
import { fetch as tauriFetch } from '@tauri-apps/plugin-http'
import { createMarketplaceClient } from '@haex-space/marketplace-sdk'
import type { ExtensionListItem, CategoryWithCount, ListExtensionsParams, DownloadResponse, ExtensionDetail } from '@haex-space/marketplace-sdk'
import { fetchWithDidAuth } from '@/utils/auth/didAuth'
import { haexMarketplaces } from '@/database/schemas/marketplaces'
import type { SelectHaexMarketplaces } from '@/database/schemas/marketplaces'
import { requireDb } from '~/stores/vault'
import { createLogger } from '@/stores/logging'

const log = createLogger('MARKETPLACE')

const DEFAULT_MARKETPLACE_NAME = 'Haex Marketplace'
const DEFAULT_MARKETPLACE_URL = 'https://marketplace.haex.space'

/** Identity context needed when auth_type is 'did'. Caller loads it from the identity store. */
export interface DidIdentityContext {
  did: string
  privateKey: string
}

/**
 * Returns a fetch-compatible function that adds the correct auth header for this marketplace row.
 * For auth_type='did', pass the resolved identity context as the second argument.
 */
export function buildAuthedFetch(
  row: SelectHaexMarketplaces,
  didContext?: DidIdentityContext,
): (input: string, init?: RequestInit) => Promise<Response> {
  switch (row.authType) {
    case 'bearer':
      return (input, init) =>
        tauriFetch(input, {
          ...init,
          headers: { ...init?.headers, Authorization: `Bearer ${row.authToken}` },
        }) as unknown as Promise<Response>

    case 'basic': {
      const creds = btoa(`${row.authUsername}:${row.authPassword}`)
      return (input, init) =>
        tauriFetch(input, {
          ...init,
          headers: { ...init?.headers, Authorization: `Basic ${creds}` },
        }) as unknown as Promise<Response>
    }

    case 'did':
      if (!didContext) {
        throw new Error(
          `Marketplace ${row.name}: auth_type=did requires a didContext (load identity ${row.authIdentityId} first)`,
        )
      }
      return (input, init) =>
        fetchWithDidAuth(input, didContext.privateKey, didContext.did, 'marketplace:list', init)

    default: // 'none'
      return (input, init) => tauriFetch(input, init) as unknown as Promise<Response>
  }
}

export interface AggregatedExtension extends ExtensionListItem {
  sourceMarketplaceId: string
  sourceMarketplaceName: string
}

export interface SourceError {
  name: string
  message: string
}

/**
 * Ensures the built-in haex.space marketplace row exists. Called from
 * initVaultAsync at vault open — same lifecycle as ensureDefaultIdentityAsync
 * and ensureDefaultSpaceAsync. Inserts through drizzle so the row gets a
 * proper HLC timestamp and participates in CRDT sync.
 */
export async function ensureDefaultMarketplaceAsync(): Promise<void> {
  const db = requireDb()

  const existing = await db
    .select()
    .from(haexMarketplaces)
    .where(eq(haexMarketplaces.isDefault, true))
    .limit(1)

  if (existing.length > 0) return

  await db.insert(haexMarketplaces).values({
    name: DEFAULT_MARKETPLACE_NAME,
    baseUrl: DEFAULT_MARKETPLACE_URL,
    enabled: true,
    isDefault: true,
    sortOrder: 1,
    authType: 'none',
  })

  log.info('Default marketplace created')
}

export type MarketplaceAuthType = 'none' | 'bearer' | 'basic' | 'did'

export interface MarketplaceInput {
  name: string
  baseUrl: string
  enabled?: boolean
  sortOrder?: number
  authType?: MarketplaceAuthType
  authToken?: string | null
  authUsername?: string | null
  authPassword?: string | null
  authIdentityId?: string | null
}

/** Load every marketplace row (enabled and disabled), ordered by sortOrder. */
export async function loadAllMarketplacesAsync(): Promise<SelectHaexMarketplaces[]> {
  const db = requireDb()
  return db.select().from(haexMarketplaces).orderBy(asc(haexMarketplaces.sortOrder))
}

export async function createMarketplaceAsync(input: MarketplaceInput): Promise<void> {
  const db = requireDb()
  await db.insert(haexMarketplaces).values({
    name: input.name,
    baseUrl: input.baseUrl,
    enabled: input.enabled ?? true,
    isDefault: false,
    sortOrder: input.sortOrder ?? 100,
    authType: input.authType ?? 'none',
    authToken: input.authToken ?? null,
    authUsername: input.authUsername ?? null,
    authPassword: input.authPassword ?? null,
    authIdentityId: input.authIdentityId ?? null,
  })
  log.info(`Marketplace created: ${input.name} (${input.baseUrl})`)
}

export async function updateMarketplaceAsync(id: string, patch: Partial<MarketplaceInput>): Promise<void> {
  const db = requireDb()
  await db.update(haexMarketplaces)
    .set({ ...patch, updatedAt: new Date().toISOString() })
    .where(eq(haexMarketplaces.id, id))
  log.info(`Marketplace updated: ${id}`)
}

export async function setMarketplaceEnabledAsync(id: string, enabled: boolean): Promise<void> {
  await updateMarketplaceAsync(id, { enabled })
}

export async function deleteMarketplaceAsync(id: string): Promise<void> {
  const db = requireDb()
  const rows = await db.select().from(haexMarketplaces).where(eq(haexMarketplaces.id, id)).limit(1)
  const row = rows[0]
  if (!row) throw new Error(`Marketplace ${id} not found`)
  if (row.isDefault) throw new Error('The built-in default marketplace cannot be deleted')

  await db.delete(haexMarketplaces).where(eq(haexMarketplaces.id, id))
  log.info(`Marketplace deleted: ${row.name} (${id})`)
}

export function useMarketplaces() {
  const extensions = ref<AggregatedExtension[]>([])
  const extensionsTotal = ref(0)
  const categories = ref<CategoryWithCount[]>([])
  const isLoading = ref(false)
  const sourceErrors = ref<Record<string, SourceError>>({})

  const loadEnabledRowsAsync = async () => {
    const db = requireDb()
    return db.select().from(haexMarketplaces)
      .where(eq(haexMarketplaces.enabled, true))
      .orderBy(asc(haexMarketplaces.sortOrder))
  }

  const buildClient = (row: SelectHaexMarketplaces) =>
    createMarketplaceClient({
      baseUrl: row.baseUrl,
      fetch: buildAuthedFetch(row) as unknown as typeof globalThis.fetch,
    })

  const fetchExtensions = async (params?: ListExtensionsParams) => {
    isLoading.value = true
    sourceErrors.value = {}
    try {
      const rows = await loadEnabledRowsAsync()
      const settled = await Promise.allSettled(
        rows.map(row => {
          try {
            return buildClient(row).listExtensions(params)
          } catch (err) {
            return Promise.reject(err)
          }
        }),
      )

      const merged: AggregatedExtension[] = []
      const seen = new Set<string>()
      let totalAcrossSources = 0

      for (let i = 0; i < settled.length; i++) {
        const result = settled[i]!
        const row = rows[i]!
        if (result.status === 'fulfilled') {
          totalAcrossSources += result.value.pagination?.total ?? result.value.extensions.length
          for (const ext of result.value.extensions) {
            if (!seen.has(ext.extensionId)) {
              seen.add(ext.extensionId)
              merged.push({ ...ext, sourceMarketplaceId: row.id, sourceMarketplaceName: row.name })
            }
          }
        } else {
          const err = result.reason as Error
          const message = err?.message ?? 'Unknown error'
          sourceErrors.value[row.id] = { name: row.name, message }
          log.error(`fetchExtensions failed for "${row.name}" (${row.baseUrl}): ${message}`)
        }
      }

      extensions.value = merged
      extensionsTotal.value = totalAcrossSources
    } finally {
      isLoading.value = false
    }
  }

  const fetchCategories = async () => {
    const rows = await loadEnabledRowsAsync()
    const settled = await Promise.allSettled(
      rows.map(row => {
        try {
          return buildClient(row).listCategories()
        } catch (err) {
          return Promise.reject(err)
        }
      }),
    )

    const merged: CategoryWithCount[] = []
    const seen = new Set<string>()
    for (let i = 0; i < settled.length; i++) {
      const result = settled[i]!
      const row = rows[i]!
      if (result.status === 'fulfilled') {
        for (const cat of result.value.categories) {
          if (!seen.has(cat.slug)) {
            seen.add(cat.slug)
            merged.push(cat)
          }
        }
      } else {
        const err = result.reason as Error
        const message = err?.message ?? 'Unknown error'
        sourceErrors.value[row.id] = { name: row.name, message }
        log.error(`fetchCategories failed for "${row.name}" (${row.baseUrl}): ${message}`)
      }
    }
    categories.value = merged
    return merged
  }

  const resolveRowAsync = async (sourceMarketplaceId?: string) => {
    const rows = await loadEnabledRowsAsync()
    const row = sourceMarketplaceId
      ? rows.find(r => r.id === sourceMarketplaceId)
      : rows.find(r => r.isDefault) ?? rows[0]
    if (!row) throw new Error('No enabled marketplace found')
    return row
  }

  const getDownloadUrl = async (slug: string, sourceMarketplaceId?: string, version?: string): Promise<DownloadResponse> => {
    const row = await resolveRowAsync(sourceMarketplaceId)
    return buildClient(row).getDownloadUrl(slug, version)
  }

  const fetchExtension = async (slug: string, sourceMarketplaceId?: string): Promise<ExtensionDetail> => {
    const row = await resolveRowAsync(sourceMarketplaceId)
    return buildClient(row).getExtension(slug)
  }

  const clearError = () => {
    sourceErrors.value = {}
  }

  return {
    extensions,
    extensionsTotal,
    categories,
    isLoading,
    sourceErrors,
    fetchExtensions,
    fetchExtension,
    fetchCategories,
    getDownloadUrl,
    clearError,
  }
}
