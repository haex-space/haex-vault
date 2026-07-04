<template>
  <!--
    K1 — "Aktive Freigaben" section on the owner-side cloud-storage detail
    view. Shows every haex_s3_backends row that was derived from this
    backend via a Space share (origin_type='shared_from_space' +
    parent_backend_id=this.id) together with the space it landed in.

    The section is hidden entirely while loading returns no rows (design
    doc §7.4, parallel to J3's SpaceSharedCloudStorages behaviour).
  -->
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
      <div
        class="flex items-center gap-2 px-2.5 py-2.5 text-xs font-semibold text-muted uppercase tracking-wide cursor-pointer hover:text-foreground transition-colors"
      >
        <UIcon
          name="i-lucide-chevron-right"
          class="w-3 h-3 shrink-0 transition-transform duration-200 [[data-state=open]>&]:rotate-90"
        />
        <UIcon
          name="i-lucide-share-2"
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
                name="i-lucide-users"
                class="w-4 h-4 text-primary shrink-0"
              />
              <div class="min-w-0 flex-1">
                <p class="text-sm font-medium truncate">
                  {{ entry.spaceName }}
                </p>
                <p class="text-xs text-muted truncate font-mono">
                  {{ entry.sharePrefix?.trim() ? entry.sharePrefix : t('wholeBucket') }}
                </p>
                <p
                  v-if="entry.createdAt"
                  class="text-[10px] text-muted mt-0.5"
                >
                  {{ formatRelative(entry.createdAt) }}
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
                :space-id="entry.spaceId"
                :label="entry.spaceName"
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
  haexSpaces,
} from '~/database/schemas'
import {
  SHARE_ACCESS_READ_ONLY,
  SHARE_ACCESS_READ_WRITE,
} from '@/lib/storage/shareAccessFlags'
import SpaceItemDeleteButton from '../spaces/SpaceItemDeleteButton.vue'

interface ShareEntry {
  id: string
  name: string
  sharePrefix: string | null
  shareAccessFlags: number | null
  createdAt: string | null
  spaceId: string
  spaceName: string
}

const props = defineProps<{
  parentBackendId: string
}>()

const { t } = useI18n()
const { getDb } = useVaultDb()

const isLoading = ref(false)
const entries = ref<ShareEntry[]>([])

const HAEX_S3_BACKENDS_TABLE = 'haex_s3_backends'

const loadAsync = async () => {
  const db = getDb()
  if (!db) return

  isLoading.value = true
  try {
    // Pull owner-side "shared_from_space" derivations of THIS parent backend.
    const rows = await db
      .select()
      .from(haexS3Backends)
      .where(
        and(
          eq(haexS3Backends.parentBackendId, props.parentBackendId),
          eq(haexS3Backends.originType, 'shared_from_space'),
        ),
      )

    if (rows.length === 0) {
      entries.value = []
      return
    }

    // Resolve (backendId → spaceId) via haex_shared_space_sync. rowPks is a
    // JSON array; for haex_s3_backends the primary key is single-column so
    // rowPks[0] is the backend id.
    const assignments = await db
      .select()
      .from(haexSharedSpaceSync)
      .where(eq(haexSharedSpaceSync.tableName, HAEX_S3_BACKENDS_TABLE))

    const spaceByBackend = new Map<string, string>()
    for (const a of assignments) {
      const pks = Array.isArray(a.rowPks) ? a.rowPks : []
      const first = pks[0]
      if (typeof first === 'string') spaceByBackend.set(first, a.spaceId)
    }

    // Fetch names for the referenced spaces.
    const spaceIds = new Set(spaceByBackend.values())
    const spaceNameById = new Map<string, string>()
    if (spaceIds.size > 0) {
      const spaces = await db.select().from(haexSpaces)
      for (const s of spaces) {
        if (spaceIds.has(s.id)) spaceNameById.set(s.id, s.name)
      }
    }

    // Drop any share row whose space mapping is missing (should not happen
    // under normal CRDT flow, but avoids rendering an orphan revoke button).
    entries.value = rows.flatMap((r): ShareEntry[] => {
      const spaceId = spaceByBackend.get(r.id)
      if (!spaceId) return []
      return [{
        id: r.id,
        name: r.name,
        sharePrefix: r.sharePrefix,
        shareAccessFlags: r.shareAccessFlags,
        createdAt: r.createdAt,
        spaceId,
        spaceName: spaceNameById.get(spaceId) ?? t('unknownSpace'),
      }]
    })
  } finally {
    isLoading.value = false
  }
}

/**
 * Optimistically drop the deleted row, then re-query to reconcile with
 * anything the CRDT sync may have resurrected. Mirrors J3's behaviour.
 */
const onDeleted = async (backendId: string) => {
  entries.value = entries.value.filter((e) => e.id !== backendId)
  await loadAsync()
}

// Access-level chip helpers — mirror `storage.vue` (I1). Kept local so
// the section can render without pulling in the wider storage settings.
type AccessLevelBadgeColor = 'success' | 'warning' | 'neutral'

const accessLevelBadgeColor = (
  flags: number | null | undefined,
): AccessLevelBadgeColor => {
  if (flags == null) return 'neutral'
  if (flags === SHARE_ACCESS_READ_ONLY) return 'success'
  if (flags === SHARE_ACCESS_READ_WRITE) return 'warning'
  return 'neutral'
}

const accessLevelBadgeLabel = (flags: number | null | undefined): string => {
  if (flags == null) return t('accessLevel.custom')
  if (flags === SHARE_ACCESS_READ_ONLY) return t('accessLevel.readOnly')
  if (flags === SHARE_ACCESS_READ_WRITE) return t('accessLevel.readWrite')
  return t('accessLevel.custom')
}

/**
 * Relative time helper — mirrors `useSyncRulesState.formatRelative`. Kept
 * local to avoid pulling the sync-rules composable into a storage-only
 * component. `createdAt` is a SQLite CURRENT_TIMESTAMP string
 * (`YYYY-MM-DD HH:MM:SS`, UTC); parse it explicitly so we don't rely on
 * host locale interpretation.
 */
const formatRelative = (timestamp: string): string => {
  const parsed = parseSqliteTimestamp(timestamp)
  if (parsed == null) return timestamp
  const diff = Date.now() - parsed
  if (diff < 60_000) return t('time.justNow')
  if (diff < 3_600_000)
    return t('time.minutesAgo', { n: Math.floor(diff / 60_000) })
  if (diff < 86_400_000)
    return t('time.hoursAgo', { n: Math.floor(diff / 3_600_000) })
  if (diff < 30 * 86_400_000)
    return t('time.daysAgo', { n: Math.floor(diff / 86_400_000) })
  return new Date(parsed).toLocaleDateString()
}

const parseSqliteTimestamp = (value: string): number | null => {
  // "YYYY-MM-DD HH:MM:SS" → treat as UTC.
  const iso = value.includes('T') ? value : value.replace(' ', 'T') + 'Z'
  const t = Date.parse(iso)
  return Number.isNaN(t) ? null : t
}

onMounted(loadAsync)

watch(() => props.parentBackendId, loadAsync)
</script>

<i18n lang="yaml">
de:
  title: Aktive Freigaben
  loading: Wird geladen …
  wholeBucket: Ganzer Bucket
  unknownSpace: Unbekannter Space
  accessLevel:
    readOnly: nur lesen
    readWrite: lesen + schreiben
    custom: benutzerdefiniert
  time:
    justNow: gerade eben
    minutesAgo: "vor {n} min"
    hoursAgo: "vor {n} h"
    daysAgo: "vor {n} Tagen"
en:
  title: Active Shares
  loading: Loading …
  wholeBucket: Whole bucket
  unknownSpace: Unknown space
  accessLevel:
    readOnly: read-only
    readWrite: read + write
    custom: custom
  time:
    justNow: just now
    minutesAgo: "{n}m ago"
    hoursAgo: "{n}h ago"
    daysAgo: "{n}d ago"
</i18n>
