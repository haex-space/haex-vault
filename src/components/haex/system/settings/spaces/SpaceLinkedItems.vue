<template>
  <div
    v-if="isLoading"
    class="flex justify-center py-3"
  >
    <UIcon
      name="i-lucide-loader-2"
      class="w-4 h-4 animate-spin text-muted"
    />
  </div>

  <div
    v-else-if="groups.length === 0"
    class="text-xs text-muted text-center py-3"
  >
    {{ t('empty') }}
  </div>

  <div
    v-else
    class="space-y-2"
  >
    <div
      v-for="group in groups"
      :key="group.key"
      class="rounded-md overflow-hidden bg-gray-100/50 dark:bg-gray-700/30"
    >
      <UCollapsible :unmount-on-hide="false">
        <!-- Group header (default slot = trigger) -->
        <div
          class="flex items-center gap-2 px-2.5 py-2.5 text-xs font-semibold text-muted uppercase tracking-wide cursor-pointer hover:text-foreground transition-colors"
        >
          <UIcon
            name="i-lucide-chevron-right"
            class="w-3 h-3 shrink-0 transition-transform duration-200 [[data-state=open]>&]:rotate-90"
          />
          <HaexIcon
            :name="group.icon"
            class="w-3.5 h-3.5 shrink-0"
          />
          <span class="truncate">{{ group.label }}</span>
          <UBadge
            variant="subtle"
            size="sm"
            color="neutral"
          >
            {{ group.items.length }}
          </UBadge>
          <div class="flex-1" />
          <UiButton
            icon="i-lucide-external-link"
            color="neutral"
            variant="ghost"
            :title="t('open')"
            @click.stop="emit('open-group', group)"
          />
        </div>

        <!-- Items -->
        <template #content>
          <UContextMenu
            v-for="(item, idx) in group.items"
            :key="idx"
            :items="[
              {
                label: t('remove'),
                icon: 'i-lucide-trash-2',
                color: 'error' as const,
                onSelect: () => onRemoveItem(item),
              },
            ]"
          >
            <div
              class="group flex items-center justify-between gap-2 px-2.5 py-1.5 hover:bg-gray-200/50 dark:hover:bg-gray-600/30 transition-colors"
              :class="idx % 2 === 1 ? 'bg-gray-100/30 dark:bg-gray-700/20' : ''"
            >
              <div class="flex items-center gap-2 min-w-0 flex-1">
                <UIcon
                  :name="item.icon"
                  class="w-3.5 h-3.5 shrink-0 text-muted"
                />
                <div class="min-w-0 flex-1">
                  <p class="text-sm truncate">{{ item.label }}</p>
                  <p
                    v-if="item.subtitle"
                    class="text-xs text-muted truncate"
                  >
                    {{ item.subtitle }}
                  </p>
                </div>
              </div>
              <UiButton
                v-if="canEdit"
                color="error"
                variant="ghost"
                icon="i-lucide-trash-2"
                class="opacity-0 group-hover:opacity-100 transition-opacity shrink-0"
                @click.stop="onRemoveItem(item)"
              />
            </div>
          </UContextMenu>
        </template>
      </UCollapsible>
    </div>
  </div>
</template>

<script setup lang="ts">
import type {
  SpaceLinkedItemGroup,
  SpaceLinkedItem,
} from '~/composables/useSpaceLinkedItems'

defineProps<{
  groups: SpaceLinkedItemGroup[]
  isLoading: boolean
  canEdit?: boolean
}>()

const emit = defineEmits<{
  remove: [item: SpaceLinkedItem]
  'open-group': [group: SpaceLinkedItemGroup]
}>()

const { t } = useI18n()
const { add } = useToast()

const onRemoveItem = async (item: SpaceLinkedItem) => {
  try {
    await item.remove()
    emit('remove', item)
    add({ title: t('removed'), color: 'neutral' })
  } catch (error) {
    add({
      title: t('error'),
      description: error instanceof Error ? error.message : String(error),
      color: 'error',
    })
  }
}
</script>

<i18n lang="yaml">
de:
  empty: Keine verknüpften Inhalte
  open: Öffnen
  remove: Aus Space entfernen
  removed: Verknüpfung entfernt
  error: Fehler
en:
  empty: No linked content
  open: Open
  remove: Remove from space
  removed: Link removed
  error: Error
</i18n>
