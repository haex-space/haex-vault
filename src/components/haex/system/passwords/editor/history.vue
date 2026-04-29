<template>
  <div class="flex h-full min-h-0">
    <!-- Timeline list -->
    <div class="w-14 sm:w-56 shrink-0 overflow-y-auto border-r border-default px-3 py-4">
      <div
        v-if="sortedSnapshots.length"
        class="space-y-1"
      >
        <div
          v-for="(snapshot, index) in sortedSnapshots"
          :key="snapshot.id"
          class="flex gap-2 cursor-pointer"
          @click="selectedSnapshot = snapshot"
        >
          <!-- Dot + line -->
          <div class="flex flex-col items-center w-7 shrink-0">
            <div
              :class="[
                'size-7 rounded-full flex items-center justify-center transition-colors shrink-0',
                selectedSnapshot?.id === snapshot.id
                  ? 'bg-primary text-white'
                  : 'bg-elevated text-muted hover:bg-elevated/80',
              ]"
            >
              <UIcon
                name="i-lucide-clock"
                class="size-3.5"
              />
            </div>
            <div
              v-if="index < sortedSnapshots.length - 1"
              class="w-px flex-1 bg-default mt-1 min-h-4"
            />
          </div>

          <!-- Label (hidden on narrow) -->
          <div class="hidden sm:block flex-1 pb-4">
            <div
              class="rounded-md px-2 py-1.5 transition-colors"
              :class="[
                selectedSnapshot?.id === snapshot.id
                  ? 'bg-primary/10'
                  : 'hover:bg-elevated/50',
              ]"
            >
              <p class="text-sm font-medium leading-tight">
                {{ formatRelative(snapshot.modifiedAt || snapshot.createdAt) }}
              </p>
              <p class="text-xs text-muted mt-0.5">
                {{ snapshotLabel(snapshot) }}
              </p>
            </div>
          </div>
        </div>
      </div>

      <div
        v-else
        class="py-8 text-center text-muted text-sm"
      >
        {{ t('noHistory') }}
      </div>
    </div>

    <!-- Detail panel -->
    <div class="flex-1 overflow-y-auto px-4 py-4">
      <div
        v-if="selectedSnapshot && parsedData"
        class="max-w-lg space-y-3"
      >
        <p class="text-xs text-muted">
          {{ t('savedAt') }}: {{ formatAbsolute(selectedSnapshot.modifiedAt || selectedSnapshot.createdAt) }}
        </p>

        <!-- Core fields -->
        <div
          v-if="parsedData.title"
          class="space-y-1"
        >
          <p class="text-xs font-medium text-muted">
            {{ t('fields.title') }}
          </p>
          <UiInput
            :model-value="parsedData.title"
            :read-only="true"
          />
        </div>

        <div
          v-if="parsedData.username"
          class="space-y-1"
        >
          <p class="text-xs font-medium text-muted">
            {{ t('fields.username') }}
          </p>
          <UiInput
            :model-value="parsedData.username"
            :read-only="true"
            with-copy-button
          />
        </div>

        <div
          v-if="parsedData.password"
          class="space-y-1"
        >
          <p class="text-xs font-medium text-muted">
            {{ t('fields.password') }}
          </p>
          <UiInputPassword
            :model-value="parsedData.password"
            :read-only="true"
            with-copy-button
          />
        </div>

        <div
          v-if="parsedData.url"
          class="space-y-1"
        >
          <p class="text-xs font-medium text-muted">
            {{ t('fields.url') }}
          </p>
          <UiInput
            :model-value="parsedData.url"
            :read-only="true"
            with-copy-button
          />
        </div>

        <div
          v-if="parsedData.note"
          class="space-y-1"
        >
          <p class="text-xs font-medium text-muted">
            {{ t('fields.note') }}
          </p>
          <UiTextarea
            :model-value="parsedData.note"
            :read-only="true"
            :rows="3"
          />
        </div>

        <div
          v-if="parsedData.otpSecret"
          class="space-y-1"
        >
          <p class="text-xs font-medium text-muted">
            {{ t('fields.otpSecret') }}
          </p>
          <UiInput
            :model-value="parsedData.otpSecret"
            :read-only="true"
          />
        </div>

        <div
          v-if="parsedData.tagNames?.length"
          class="space-y-1"
        >
          <p class="text-xs font-medium text-muted">
            {{ t('fields.tags') }}
          </p>
          <div class="flex flex-wrap gap-1">
            <UBadge
              v-for="tag in parsedData.tagNames"
              :key="tag"
              :label="tag"
              color="neutral"
              variant="soft"
            />
          </div>
        </div>

        <!-- Custom fields -->
        <div
          v-if="parsedData.keyValues?.length"
          class="space-y-2"
        >
          <p class="text-xs font-medium text-muted">
            {{ t('customFields') }}
          </p>
          <div class="border border-default rounded-lg divide-y divide-default">
            <div
              v-for="(kv, i) in parsedData.keyValues"
              :key="i"
              class="grid grid-cols-2 gap-2 px-3 py-2"
            >
              <p class="text-sm font-medium truncate">
                {{ kv.key }}
              </p>
              <p class="text-sm text-muted truncate">
                {{ kv.value }}
              </p>
            </div>
          </div>
        </div>

        <!-- Attachments -->
        <div
          v-if="parsedData.attachments?.length"
          class="space-y-2"
        >
          <p class="text-xs font-medium text-muted">
            {{ t('attachments') }}
          </p>
          <div class="flex flex-col gap-1">
            <div
              v-for="att in parsedData.attachments"
              :key="att.binaryHash"
              class="flex items-center gap-2 px-3 py-2 rounded-md border border-default"
            >
              <UIcon
                name="i-lucide-paperclip"
                class="size-4 text-muted shrink-0"
              />
              <span class="text-sm flex-1 truncate">{{ att.fileName }}</span>
            </div>
          </div>
        </div>
      </div>

      <!-- Nothing selected -->
      <div
        v-else-if="sortedSnapshots.length"
        class="h-full flex flex-col items-center justify-center gap-2 text-muted"
      >
        <UIcon
          name="i-lucide-arrow-left"
          class="size-6 opacity-40"
        />
        <p class="text-sm">
          {{ t('selectSnapshot') }}
        </p>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { useTimeAgo } from '@vueuse/core'
import { loadSnapshotsAsync } from '~/utils/passwords/snapshots'
import type {
  SelectHaexPasswordsItemSnapshots,
} from '~/database/schemas'
import type { SnapshotData } from '~/utils/passwords/snapshots'

const props = defineProps<{ itemId?: string }>()

const { t, locale } = useI18n()

const snapshots = ref<SelectHaexPasswordsItemSnapshots[]>([])
const selectedSnapshot = ref<SelectHaexPasswordsItemSnapshots | null>(null)

const sortedSnapshots = computed(() =>
  [...snapshots.value].sort((a, b) => {
    const da = new Date(a.modifiedAt ?? a.createdAt ?? 0).getTime()
    const db = new Date(b.modifiedAt ?? b.createdAt ?? 0).getTime()
    return db - da
  }),
)

watch(
  () => props.itemId,
  async (id) => {
    if (!id) { snapshots.value = []; return }
    try {
      snapshots.value = await loadSnapshotsAsync(id)
    } catch {
      snapshots.value = []
    }
  },
  { immediate: true },
)

watch(sortedSnapshots, (list) => {
  if (list.length && !selectedSnapshot.value) {
    selectedSnapshot.value = list[0] ?? null
  }
}, { immediate: true })

const parsedData = computed<SnapshotData | null>(() => {
  if (!selectedSnapshot.value?.snapshotData) return null
  try {
    return JSON.parse(selectedSnapshot.value.snapshotData) as SnapshotData
  } catch {
    return null
  }
})

function formatRelative(dateString: string | null | undefined): string {
  if (!dateString) return t('unknown')
  const timeAgo = useTimeAgo(new Date(dateString), {
    messages: locale.value === 'de'
      ? {
          justNow: 'gerade eben',
          past: 'vor {0}',
          future: 'in {0}',
          second: (n: number) => n === 1 ? 'einer Sekunde' : `${n} Sekunden`,
          minute: (n: number) => n === 1 ? 'einer Minute' : `${n} Minuten`,
          hour: (n: number) => n === 1 ? 'einer Stunde' : `${n} Stunden`,
          day: (n: number) => n === 1 ? 'einem Tag' : `${n} Tagen`,
          week: (n: number) => n === 1 ? 'einer Woche' : `${n} Wochen`,
          month: (n: number) => n === 1 ? 'einem Monat' : `${n} Monaten`,
          year: (n: number) => n === 1 ? 'einem Jahr' : `${n} Jahren`,
          invalid: '',
        }
      : undefined,
  })
  return timeAgo.value
}

function formatAbsolute(dateString: string | null | undefined): string {
  if (!dateString) return t('unknown')
  return new Date(dateString).toLocaleString(locale.value)
}

function snapshotLabel(snapshot: SelectHaexPasswordsItemSnapshots): string {
  try {
    const data = JSON.parse(snapshot.snapshotData) as SnapshotData
    return data.title || t('untitled')
  } catch {
    return t('untitled')
  }
}
</script>

<i18n lang="yaml">
de:
  noHistory: Noch keine gespeicherten Versionen.
  selectSnapshot: Wähle eine Version aus der Liste.
  savedAt: Gespeichert am
  unknown: Unbekannt
  untitled: (ohne Titel)
  customFields: Benutzerdefinierte Felder
  attachments: Anhänge
  fields:
    title: Titel
    username: Nutzername
    password: Passwort
    url: URL
    note: Notiz
    otpSecret: OTP Secret
    tags: Tags

en:
  noHistory: No saved versions yet.
  selectSnapshot: Select a version from the list.
  savedAt: Saved at
  unknown: Unknown
  untitled: (untitled)
  customFields: Custom Fields
  attachments: Attachments
  fields:
    title: Title
    username: Username
    password: Password
    url: URL
    note: Note
    otpSecret: OTP Secret
    tags: Tags
</i18n>
