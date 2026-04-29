import { useDebounceFn } from '@vueuse/core'
import { inArray } from 'drizzle-orm'
import { haexPasswordsBinaries } from '~/database/schemas'
import { requireDb } from '~/stores/vault'

export const usePasswordsIconCacheStore = defineStore(
  'passwordsIconCacheStore',
  () => {
    const cache = ref<Map<string, string>>(new Map())
    const pendingHashes = ref<Set<string>>(new Set())
    const isLoading = ref(false)

    const getIconDataUrl = (hash: string): string | null => {
      return cache.value.get(hash) ?? null
    }

    const isCached = (hash: string): boolean => {
      return cache.value.has(hash)
    }

    const loadPendingIconsAsync = async (): Promise<void> => {
      if (pendingHashes.value.size === 0 || isLoading.value) return

      isLoading.value = true
      const hashesToLoad = Array.from(pendingHashes.value)
      pendingHashes.value.clear()

      try {
        const db = requireDb()
        const results = await db
          .select({
            hash: haexPasswordsBinaries.hash,
            data: haexPasswordsBinaries.data,
          })
          .from(haexPasswordsBinaries)
          .where(inArray(haexPasswordsBinaries.hash, hashesToLoad))

        for (const row of results) {
          if (row.data) {
            cache.value.set(row.hash, `data:image/png;base64,${row.data}`)
          }
        }

        // Mark misses with an empty string so requestIcon doesn't re-enqueue them.
        for (const hash of hashesToLoad) {
          if (!cache.value.has(hash)) cache.value.set(hash, '')
        }
      } catch (error) {
        const cause =
          error instanceof Error && 'cause' in error ? error.cause : null
        console.error('[IconCache] Failed to load icons batch:', cause || error)

        for (const hash of hashesToLoad) {
          if (!cache.value.has(hash)) pendingHashes.value.add(hash)
        }
      } finally {
        isLoading.value = false
        if (pendingHashes.value.size > 0) debouncedLoadPendingIcons()
      }
    }

    const debouncedLoadPendingIcons = useDebounceFn(loadPendingIconsAsync, 50)

    const requestIcon = (hash: string): void => {
      if (cache.value.has(hash) || pendingHashes.value.has(hash)) return
      pendingHashes.value.add(hash)
      debouncedLoadPendingIcons()
    }

    const preloadAllIconsAsync = async (): Promise<void> => {
      isLoading.value = true
      try {
        const db = requireDb()
        const results = await db
          .select({
            hash: haexPasswordsBinaries.hash,
            data: haexPasswordsBinaries.data,
          })
          .from(haexPasswordsBinaries)

        for (const row of results) {
          if (row.data) {
            cache.value.set(row.hash, `data:image/png;base64,${row.data}`)
          }
        }
        pendingHashes.value.clear()
      } catch (error) {
        const cause =
          error instanceof Error && 'cause' in error ? error.cause : null
        console.error('[IconCache] Failed to preload icons:', cause || error)
      } finally {
        isLoading.value = false
      }
    }

    const invalidate = (hash: string): void => {
      cache.value.delete(hash)
    }

    const clear = (): void => {
      cache.value.clear()
      pendingHashes.value.clear()
    }

    return {
      cache,
      isLoading,
      getIconDataUrl,
      isCached,
      requestIcon,
      preloadAllIconsAsync,
      invalidate,
      clear,
    }
  },
)
