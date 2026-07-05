<template>
  <!-- Hide the section entirely when count is 0 (design doc §7.3). -->
  <div
    v-if="isLoading"
    class="rounded-md overflow-hidden bg-gray-100/50 dark:bg-gray-700/30 p-3 flex items-center gap-2"
  >
    <UIcon
      name="i-lucide-loader-2"
      class="w-4 h-4 animate-spin text-primary"
    />
    <span class="text-xs text-muted">{{ t('loading') }}</span>
  </div>

  <div
    v-else-if="entries.length > 0"
    class="rounded-md overflow-hidden bg-gray-100/50 dark:bg-gray-700/30"
  >
    <UCollapsible :unmount-on-hide="false">
      <!-- Collapsible trigger: section header -->
      <div
        class="flex items-center gap-2 px-2.5 py-2.5 text-xs font-semibold text-muted uppercase tracking-wide cursor-pointer hover:text-foreground transition-colors"
      >
        <UIcon
          name="i-lucide-chevron-right"
          class="w-3 h-3 shrink-0 transition-transform duration-200 [[data-state=open]>&]:rotate-90"
        />
        <UIcon
          name="i-lucide-cloud"
          class="w-3.5 h-3.5 shrink-0"
        />
        <span class="truncate">{{ t('title') }}</span>
        <UBadge
          variant="subtle"
          size="sm"
          color="neutral"
        >
          {{ entries.length }}
        </UBadge>
      </div>

      <template #content>
        <div class="p-2 space-y-1">
          <div
            v-for="(entry, idx) in entries"
            :key="entry.id"
            class="group flex items-center justify-between gap-3 px-3 py-2 rounded-md transition-colors"
            :class="idx % 2 === 1 ? 'bg-muted/5 dark:bg-muted/10' : ''"
          >
            <div class="flex items-center gap-3 min-w-0 flex-1">
              <UIcon
                name="i-lucide-database"
                class="w-4 h-4 text-primary shrink-0"
              />
              <div class="min-w-0 flex-1">
                <p class="text-sm font-medium truncate">
                  {{ entry.name }}
                </p>
                <p class="text-xs text-muted truncate font-mono">
                  {{ formatBucketPath(entry) }}
                </p>
              </div>
              <UBadge
                :color="accessLevelBadgeColor(entry.shareAccessFlags)"
                variant="subtle"
                size="sm"
                class="shrink-0"
              >
                {{ accessLevelBadgeLabel(entry.shareAccessFlags) }}
              </UBadge>
            </div>
            <div class="shrink-0 flex items-center">
              <SpaceItemDeleteButton
                item-type="shared_cloud_storage"
                :item-id="entry.id"
                :space-id="spaceId"
                :label="entry.name"
                @deleted="onDeleted(entry.id)"
              />
            </div>
          </div>
        </div>
      </template>
    </UCollapsible>
  </div>
</template>

<script setup lang="ts">
import { and, eq } from 'drizzle-orm'
import {
  haexS3Backends,
  haexSharedSpaceSync,
  type SelectHaexS3Backends,
} from '~/database/schemas'
import {
  accessLevelBadgeColor,
  accessLevelKind,
} from '@/lib/storage/shareAccessFlags'
import SpaceItemDeleteButton from './SpaceItemDeleteButton.vue'

const props = defineProps<{
  spaceId: string
}>()

const { t } = useI18n()
const { getDb } = useVaultDb()

const isLoading = ref(false)
const entries = ref<SelectHaexS3Backends[]>([])

const HAEX_S3_BACKENDS_TABLE = 'haex_s3_backends'

const loadAsync = async () => {
  const db = getDb()
  if (!db) return

  isLoading.value = true
  try {
    // Fetch shared-space-sync rows that map an s3 backend into this space.
    const assignments = await db
      .select()
      .from(haexSharedSpaceSync)
      .where(
        and(
          eq(haexSharedSpaceSync.spaceId, props.spaceId),
          eq(haexSharedSpaceSync.tableName, HAEX_S3_BACKENDS_TABLE),
        ),
      )

    // rowPks is a JSON array of primary-key values. For haex_s3_backends
    // (single-column PK) it's a one-element array containing the id.
    const backendIds = new Set<string>()
    for (const a of assignments) {
      const pks = Array.isArray(a.rowPks) ? a.rowPks : []
      const first = pks[0]
      if (typeof first === 'string') backendIds.add(first)
    }

    if (backendIds.size === 0) {
      entries.value = []
      return
    }

    // Pull the referenced backend rows, filtered to the owner-side view
    // (design doc §7.3: only show 'shared_from_space' rows in this section).
    const allBackends = await db
      .select()
      .from(haexS3Backends)
      .where(eq(haexS3Backends.originType, 'shared_from_space'))

    entries.value = allBackends.filter((b) => backendIds.has(b.id))
  } finally {
    isLoading.value = false
  }
}

/**
 * Optimistically drop the deleted row from the local list, then re-query
 * to reconcile with anything the CRDT sync may have resurrected (e.g. a
 * concurrent share from another owner device).
 */
const onDeleted = async (backendId: string) => {
  entries.value = entries.value.filter((e) => e.id !== backendId)
  await loadAsync()
}

/**
 * Render "bucket/prefix" — falls back to just the bucket if no prefix, or
 * to the backend name if the config is unreadable.
 */
const formatBucketPath = (entry: SelectHaexS3Backends): string => {
  const bucket =
    typeof entry.config?.bucket === 'string' ? entry.config.bucket : ''
  const prefix = entry.sharePrefix?.trim() ?? ''
  if (!bucket) return prefix || entry.name
  if (!prefix) return bucket
  const joined = prefix.startsWith('/') ? prefix : `/${prefix}`
  return `${bucket}${joined}`
}

const accessLevelBadgeLabel = (flags: number | null | undefined): string =>
  t(`accessLevel.${accessLevelKind(flags)}`)

onMounted(loadAsync)

watch(() => props.spaceId, loadAsync)
</script>

<i18n lang="yaml">
de:
  title: Geteilte Cloud-Speicher
  loading: Wird geladen …
  accessLevel:
    readOnly: nur lesen
    readWrite: lesen + schreiben
    custom: benutzerdefiniert
en:
  title: Shared cloud storages
  loading: Loading …
  accessLevel:
    readOnly: read-only
    readWrite: read + write
    custom: custom
</i18n>
