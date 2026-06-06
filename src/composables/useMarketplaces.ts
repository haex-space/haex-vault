import { ref } from 'vue'
import { eq, asc } from 'drizzle-orm'
import { fetch as tauriFetch } from '@tauri-apps/plugin-http'
import { createMarketplaceClient } from '@haex-space/marketplace-sdk'
import type { ExtensionListItem, CategoryWithCount, ListExtensionsParams, DownloadResponse } from '@haex-space/marketplace-sdk'
import { fetchWithDidAuth } from '@/utils/auth/didAuth'
import { haexMarketplaces } from '@/database/schemas/marketplaces'
import type { SelectHaexMarketplaces } from '@/database/schemas/marketplaces'
import { requireDb } from '~/stores/vault'

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
          sourceErrors.value[row.id] = { name: row.name, message: err?.message ?? 'Unknown error' }
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
    const settled = await Promise.allSettled(rows.map(row => buildClient(row).listCategories()))

    const merged: CategoryWithCount[] = []
    const seen = new Set<string>()
    for (const result of settled) {
      if (result.status === 'fulfilled') {
        for (const cat of result.value.categories) {
          if (!seen.has(cat.slug)) {
            seen.add(cat.slug)
            merged.push(cat)
          }
        }
      }
    }
    categories.value = merged
    return merged
  }

  const getDownloadUrl = async (slug: string, sourceMarketplaceId?: string, version?: string): Promise<DownloadResponse> => {
    const rows = await loadEnabledRowsAsync()
    const row = sourceMarketplaceId
      ? rows.find(r => r.id === sourceMarketplaceId)
      : rows.find(r => r.isDefault) ?? rows[0]

    if (!row) throw new Error('No enabled marketplace found')
    return buildClient(row).getDownloadUrl(slug, version)
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
    fetchCategories,
    getDownloadUrl,
    clearError,
  }
}
