<template>
  <UContextMenu :items="menuItems">
    <div
      ref="rowRef"
      :class="[
        'group',
        isDragging && 'opacity-40',
        isCut && 'opacity-50 grayscale',
        isMultiSelected && 'ring-2 ring-primary rounded-lg',
      ]"
      draggable="true"
      @dragstart="onDragStart"
      @dragend="onDragEnd"
    >
      <UiListItem
        :highlight="selected"
        class="cursor-pointer"
        @click="onRowClick"
        @dblclick="onRowDblClick"
      >
        <div class="flex items-center gap-3 min-h-14">
          <button
            v-if="isSelectionMode"
            type="button"
            :class="[
              'shrink-0 size-6 rounded border flex items-center justify-center transition-colors',
              isMultiSelected
                ? 'bg-primary border-primary text-inverted'
                : 'border-default hover:border-primary',
            ]"
            :aria-label="isMultiSelected ? t('deselect') : t('select')"
            @click.stop="onCheckboxClick"
          >
            <UIcon
              v-if="isMultiSelected"
              name="i-lucide-check"
              class="size-4"
            />
          </button>
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
            >
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

        <template
          v-if="isExpired"
          #actions
        >
          <UIcon
            name="i-lucide-alert-triangle"
            class="size-4 text-warning"
          />
        </template>
      </UiListItem>
    </div>
  </UContextMenu>
</template>

<script setup lang="ts">
import type { ContextMenuItem } from '@nuxt/ui'
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
const toast = useToast()
const { getIconDescriptor } = useIconComponents()
const iconCacheStore = usePasswordsIconCacheStore()
const passwordsStore = usePasswordsStore()
const groupsStore = usePasswordsGroupsStore()
const isInTrash = computed(() => {
  const groupId = groupsStore.itemGroupMap.get(props.item.id)
  return groupId ? groupsStore.isGroupInTrash(groupId) : false
})

const selection = usePasswordsSelectionStore()
const { isSelectionMode } = storeToRefs(selection)

const isMultiSelected = computed(() => selection.isSelected(props.item.id))
const isCut = computed(() => selection.isCut(props.item.id))

const isWideLayout = inject<Ref<boolean>>('passwords:isWideLayout', ref(false))

const orderedIds = inject<Ref<string[]>>(
  'passwordsList:orderedIds',
  ref<string[]>([]),
)

const rowRef = useTemplateRef<HTMLElement>('rowRef')
const { shouldSuppressClick } = useLongPressSelection(rowRef, () => {
  if (!isSelectionMode.value) {
    selection.enterSelectionWith(props.item.id)
  } else {
    selection.toggle(props.item.id)
  }
})

const onRowClick = (event: MouseEvent) => {
  if (shouldSuppressClick()) return
  if (event.shiftKey) {
    event.preventDefault()
    selection.selectRange(props.item.id, orderedIds.value)
    return
  }
  if (event.ctrlKey || event.metaKey) {
    event.preventDefault()
    selection.toggle(props.item.id)
    return
  }
  if (isSelectionMode.value) {
    // In selection mode a plain click continues to toggle — matches
    // file-manager conventions and avoids accidental drill-down.
    selection.toggle(props.item.id)
    return
  }
  // On wide layout (sidebar visible), single-click selects the item instead
  // of opening it. This enables keyboard shortcuts (Ctrl+C/B) to copy
  // credentials without touching the mouse again.
  if (isWideLayout.value) {
    selection.setDesktopFocus(props.item.id)
    return
  }
  emit('click')
}

const onRowDblClick = () => {
  if (!isWideLayout.value) return
  if (shouldSuppressClick()) return
  if (isSelectionMode.value) return
  emit('click')
}

const onCheckboxClick = (event: MouseEvent) => {
  if (event.shiftKey) {
    selection.selectRange(props.item.id, orderedIds.value)
    return
  }
  selection.toggle(props.item.id)
}

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

const isDragging = ref(false)

const onDragStart = (event: DragEvent) => {
  if (!event.dataTransfer) return
  event.dataTransfer.effectAllowed = 'move'
  event.dataTransfer.setData('application/x-haex-item', props.item.id)
  isDragging.value = true
}

const onDragEnd = () => {
  isDragging.value = false
}

const copyToClipboard = async (value: string | null | undefined, key: string) => {
  if (!value) return
  try {
    await navigator.clipboard.writeText(value)
    toast.add({ title: t(`toast.${key}`), color: 'success', duration: 1500 })
  } catch (error) {
    console.error('[ListItem] clipboard write failed', error)
    toast.add({
      title: t('toast.copyFailed'),
      color: 'error',
      icon: 'i-lucide-alert-triangle',
    })
  }
}

const menuItems = computed<ContextMenuItem[][]>(() => {
  if (isInTrash.value) {
    return [
      [
        {
          label: t('menu.restore'),
          icon: 'i-lucide-undo-2',
          onSelect: async () => {
            try {
              await groupsStore.restoreItemAsync(props.item.id)
              toast.add({ title: t('toast.restored'), color: 'success' })
            } catch (error) {
              console.error('[ListItem] restore failed', error)
            }
          },
        },
      ],
      [
        {
          label: t('menu.deletePermanently'),
          icon: 'i-lucide-trash-2',
          color: 'error' as const,
          onSelect: async () => {
            try {
              await passwordsStore.deleteItemAsync(props.item.id)
              toast.add({ title: t('toast.deleted'), color: 'success' })
            } catch (error) {
              console.error('[ListItem] delete failed', error)
            }
          },
        },
      ],
    ]
  }
  return [
    [
      {
        label: t('menu.open'),
        icon: 'i-lucide-pencil',
        onSelect: () => emit('click'),
      },
    ],
    [
      {
        label: t('menu.copyUsername'),
        icon: 'i-lucide-user',
        disabled: !props.item.username,
        onSelect: () => copyToClipboard(props.item.username, 'usernameCopied'),
      },
      {
        label: t('menu.copyPassword'),
        icon: 'i-lucide-key-round',
        disabled: !props.item.password,
        onSelect: () => copyToClipboard(props.item.password, 'passwordCopied'),
      },
    ],
    [
      {
        label: t('menu.delete'),
        icon: 'i-lucide-trash',
        color: 'error' as const,
        onSelect: async () => {
          try {
            await passwordsStore.deleteItemAsync(props.item.id)
            toast.add({ title: t('toast.movedToTrash'), color: 'success' })
          } catch (error) {
            console.error('[ListItem] delete failed', error)
          }
        },
      },
    ],
  ]
})
</script>

<i18n lang="yaml">
de:
  untitled: (ohne Titel)
  select: Auswählen
  deselect: Abwählen
  menu:
    open: Öffnen
    copyUsername: Benutzername kopieren
    copyPassword: Passwort kopieren
    delete: In Papierkorb
    restore: Wiederherstellen
    deletePermanently: Endgültig löschen
  toast:
    usernameCopied: Benutzername kopiert
    passwordCopied: Passwort kopiert
    movedToTrash: In Papierkorb verschoben
    restored: Wiederhergestellt
    deleted: Eintrag gelöscht
    copyFailed: Kopieren fehlgeschlagen
en:
  untitled: (untitled)
  select: Select
  deselect: Deselect
  menu:
    open: Open
    copyUsername: Copy username
    copyPassword: Copy password
    delete: Move to trash
    restore: Restore
    deletePermanently: Delete permanently
  toast:
    usernameCopied: Username copied
    passwordCopied: Password copied
    movedToTrash: Moved to trash
    restored: Restored
    deleted: Entry deleted
    copyFailed: Copy failed
</i18n>
