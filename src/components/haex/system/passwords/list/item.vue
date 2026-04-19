<template>
  <UiListItem
    :highlight="selected"
    class="cursor-pointer"
    @click="emit('click')"
  >
    <div class="flex items-center gap-3 min-h-14">
      <!-- Icon -->
      <div
        class="shrink-0 size-10 rounded-md flex items-center justify-center bg-elevated overflow-hidden"
        :style="iconBackgroundStyle"
      >
        <UIcon
          v-if="iconDescriptor.kind === 'iconify'"
          :name="iconDescriptor.name"
          class="size-6"
          :class="iconColorClass"
        />
        <img
          v-else-if="binaryIconSrc"
          :src="binaryIconSrc"
          :alt="item.title ?? 'icon'"
          class="size-8 object-contain"
        />
        <UIcon
          v-else
          name="i-lucide-key"
          class="size-6 text-muted"
        />
      </div>

      <!-- Content -->
      <div class="flex-1 min-w-0">
        <p class="font-medium truncate">
          {{ item.title || t('untitled') }}
        </p>

        <div
          v-if="item.username || item.url"
          class="mt-0.5 flex items-center gap-3 text-xs text-muted"
        >
          <span
            v-if="item.username"
            class="flex items-center gap-1 min-w-0"
          >
            <UIcon
              name="i-lucide-user"
              class="hidden @md:inline size-3 shrink-0"
            />
            <span class="truncate">{{ item.username }}</span>
          </span>
          <span
            v-if="item.url"
            class="flex items-center gap-1 min-w-0"
          >
            <UIcon
              name="i-lucide-globe"
              class="hidden @md:inline size-3 shrink-0"
            />
            <span class="truncate">{{ displayUrl }}</span>
          </span>
        </div>

        <div
          v-if="tags.length"
          class="mt-1.5 flex flex-wrap gap-1"
        >
          <UBadge
            v-for="tag in tags"
            :key="tag.id"
            :label="tag.name"
            color="neutral"
            variant="soft"
          />
        </div>
      </div>
    </div>

    <template #actions>
      <div class="flex items-center gap-1 text-muted">
        <UIcon
          v-if="isExpired"
          name="i-lucide-alert-triangle"
          class="size-4 text-warning"
        />
        <UIcon
          name="i-lucide-chevron-right"
          class="size-4"
        />
      </div>
    </template>
  </UiListItem>
</template>

<script setup lang="ts">
import type {
  SelectHaexPasswordsItemDetails,
  SelectHaexPasswordsTags,
} from '~/database/schemas'

const props = defineProps<{
  item: SelectHaexPasswordsItemDetails
  tags: SelectHaexPasswordsTags[]
  selected: boolean
}>()

const emit = defineEmits<{ click: [] }>()

const { t } = useI18n()
const { getIconDescriptor } = useIconComponents()
const iconCacheStore = usePasswordsIconCacheStore()

const iconDescriptor = computed(() => getIconDescriptor(props.item.icon))

// Binary icons are loaded from the DB via the cache store. Trigger lookup on first render.
const binaryIconSrc = computed(() => {
  if (iconDescriptor.value.kind !== 'binary') return null
  const src = iconCacheStore.getIconDataUrl(iconDescriptor.value.hash)
  // src === null → request. src === '' → DB miss, don't retry.
  if (src === null) {
    iconCacheStore.requestIcon(iconDescriptor.value.hash)
    return null
  }
  return src || null
})

const iconBackgroundStyle = computed(() => {
  if (!props.item.color) return undefined
  return { backgroundColor: props.item.color }
})

const iconColorClass = computed(() =>
  props.item.color ? '' : 'text-primary',
)

const displayUrl = computed(() => {
  if (!props.item.url) return ''
  try {
    return new URL(props.item.url).hostname
  } catch {
    return props.item.url
  }
})

const isExpired = computed(() => {
  if (!props.item.expiresAt) return false
  const ts = Date.parse(props.item.expiresAt)
  if (Number.isNaN(ts)) return false
  return ts < Date.now()
})
</script>

<i18n lang="yaml">
de:
  untitled: (ohne Titel)
en:
  untitled: (untitled)
</i18n>
