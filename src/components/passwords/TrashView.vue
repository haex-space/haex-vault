<script setup lang="ts">
/**
 * The trash (spec 034, US4, FR-015, FR-016): the folders and entries that were deleted, each with
 * the folder path it came from, with "Restore" and "Delete for good" (after a confirmation naming
 * what goes), and "Empty trash" (after a confirmation that names the number). An entry in a trashed
 * folder is part of that folder's row. Only the user can do any of this.
 */
import { toast } from 'vue-sonner'
import type { ItemHeader } from '@bindings/ItemHeader'
import type { Target } from '@bindings/Target'
import { displayTitle } from '~/lib/passwords/format'
import { buildMenu, type MenuCommand } from '~/lib/passwords/menus'
import { groupPath, trashGroupIds, TRASH_GROUP_ID } from '~/lib/passwords/tree'

const { t } = useI18n()
const { errString } = useErrorString()
const router = useTabRouter()
const store = usePasswordsStore()
const { restoreAsync, deletePermanentlyAsync } = usePasswords()

const trashIds = computed(() => trashGroupIds(store.groups))

/** Folders deleted directly (their parent is the trash) with the number of entries inside. */
const folders = computed(() =>
  store.groups
    .filter((group) => group.parentId === TRASH_GROUP_ID)
    .map((group) => {
      const inside = new Set([group.id])
      let grew = true
      while (grew) {
        grew = false
        for (const other of store.groups) {
          if (
            other.parentId &&
            inside.has(other.parentId) &&
            !inside.has(other.id)
          ) {
            inside.add(other.id)
            grew = true
          }
        }
      }
      return {
        group,
        path: pathOf(group.trashedFromParentId),
        entries: store.headers.filter((h) => h.groupId && inside.has(h.groupId))
          .length,
      }
    }),
)

/** Entries deleted directly: those whose folder is the trash itself. */
const entries = computed(() =>
  store.headers
    .filter((header) => header.groupId === TRASH_GROUP_ID)
    .map((header) => ({ header, path: pathOf(header.trashedFromGroupId) })),
)

const trashedItemIds = computed(() =>
  store.headers
    .filter((h) => h.groupId && trashIds.value.has(h.groupId))
    .map((h) => h.id),
)
const totalEntries = computed(
  () =>
    store.headers.filter((h) => h.groupId && trashIds.value.has(h.groupId))
      .length,
)
const totalFolders = computed(
  () =>
    store.groups.filter(
      (g) => g.id !== TRASH_GROUP_ID && trashIds.value.has(g.id),
    ).length,
)

function pathOf(groupId: string | null): string {
  const names = groupPath(store.groups, groupId)
  return names.length > 0
    ? names.map((n) => n ?? t('passwords.trash.title')).join(' / ')
    : t('passwords.trash.topLevel')
}

const emptyDialog = ref(false)
const deleting = ref<Target | null>(null)
const deletingLabel = ref('')
const inlineReferences = ref(true)

/** The entries a delete for good removes: the entry, or every entry inside the folder (spec 036). */
const deletingItemIds = computed(() => {
  const target = deleting.value
  if (!target) return []
  if (target.kind === 'item') return [target.id]
  const inside = new Set([target.id])
  let grew = true
  while (grew) {
    grew = false
    for (const group of store.groups) {
      if (
        group.parentId &&
        inside.has(group.parentId) &&
        !inside.has(group.id)
      ) {
        inside.add(group.id)
        grew = true
      }
    }
  }
  return store.headers
    .filter((header) => header.groupId !== null && inside.has(header.groupId))
    .map((header) => header.id)
})

function target(kind: 'item' | 'group', id: string): Target {
  return { kind, id }
}

async function restoreOneAsync(item: Target) {
  try {
    await restoreAsync([item])
    await store.quietReloadAsync()
    toast.success(t('passwords.trash.restored'))
  } catch (cause) {
    toast.error(errString(cause))
  }
}

function askDelete(item: Target, label: string) {
  deleting.value = item
  deletingLabel.value = label
  inlineReferences.value = true
}

async function confirmDeleteAsync() {
  const item = deleting.value
  deleting.value = null
  if (!item) return
  try {
    await deletePermanentlyAsync([item], inlineReferences.value)
    await store.quietReloadAsync()
  } catch (cause) {
    toast.error(errString(cause))
  }
}

/** The right-click menu of a row in the trash (spec 036, FR-018): the two buttons of the row. */
const trashMenu = buildMenu({ kind: 'entry', inTrash: true })

function runTrashCommand(command: MenuCommand, item: Target, label: string) {
  if (command === 'restore') void restoreOneAsync(item)
  else if (command === 'deleteForGood') askDelete(item, label)
}

function titleOf(header: ItemHeader): string {
  return displayTitle(header.title) ?? t('passwords.untitled')
}
</script>

<template>
  <div class="min-h-0 flex-1 overflow-y-auto px-4 pb-6 @md:px-6">
    <div class="mx-auto flex w-full max-w-3xl flex-col gap-3">
      <div class="flex items-center gap-2 pt-1">
        <UiButton
          variant="ghost"
          size="icon"
          class="-ml-2 shrink-0"
          :aria-label="t('passwords.back')"
          @click="router.back()"
        >
          <Icon name="lucide:arrow-left" class="size-5" />
        </UiButton>
        <h1
          class="min-w-0 flex-1 truncate text-2xl font-bold"
          data-testid="passwords-trash-title"
        >
          {{ t('passwords.trash.title') }}
        </h1>
        <UiButton
          variant="outline"
          class="shrink-0"
          :disabled="totalEntries + totalFolders === 0"
          data-testid="passwords-empty-trash"
          @click="emptyDialog = true"
        >
          <Icon name="lucide:trash-2" class="size-4" />
          {{ t('passwords.trash.empty') }}
        </UiButton>
      </div>

      <p
        v-if="folders.length === 0 && entries.length === 0"
        class="py-10 text-center text-muted-foreground"
        data-testid="passwords-trash-empty"
      >
        {{ t('passwords.trash.isEmpty') }}
      </p>

      <SettingsGroup
        v-if="folders.length > 0"
        :label="t('passwords.trash.folders')"
      >
        <PasswordsEntryMenu
          v-for="folder in folders"
          :key="folder.group.id"
          :entries="trashMenu"
          @run="
            runTrashCommand(
              $event,
              target('group', folder.group.id),
              displayTitle(folder.group.name) ?? t('passwords.untitled'),
            )
          "
        >
          <SettingsRow
            :title="displayTitle(folder.group.name) ?? t('passwords.untitled')"
            :description="
              t('passwords.trash.cameFrom', { path: folder.path }) +
              ' · ' +
              t(
                'passwords.trash.entriesInside',
                { count: folder.entries },
                folder.entries,
              )
            "
            icon="lucide:folder"
            :data-testid="`passwords-trash-folder-${folder.group.id}`"
          >
            <UiButton
              variant="outline"
              size="sm"
              :data-testid="`passwords-trash-restore-${folder.group.id}`"
              @click="restoreOneAsync(target('group', folder.group.id))"
            >
              {{ t('passwords.trash.restore') }}
            </UiButton>
            <UiButton
              variant="ghost"
              size="sm"
              :data-testid="`passwords-trash-delete-${folder.group.id}`"
              @click="
                askDelete(
                  target('group', folder.group.id),
                  displayTitle(folder.group.name) ?? t('passwords.untitled'),
                )
              "
            >
              {{ t('passwords.trash.deleteForGood') }}
            </UiButton>
          </SettingsRow>
        </PasswordsEntryMenu>
      </SettingsGroup>

      <SettingsGroup
        v-if="entries.length > 0"
        :label="t('passwords.trash.entries')"
      >
        <PasswordsEntryMenu
          v-for="entry in entries"
          :key="entry.header.id"
          :entries="trashMenu"
          @run="
            runTrashCommand(
              $event,
              target('item', entry.header.id),
              titleOf(entry.header),
            )
          "
        >
          <SettingsRow
            :title="titleOf(entry.header)"
            :description="t('passwords.trash.cameFrom', { path: entry.path })"
            icon="lucide:key-round"
            :data-testid="`passwords-trash-entry-${entry.header.id}`"
          >
            <UiButton
              variant="ghost"
              size="sm"
              :data-testid="`passwords-trash-view-${entry.header.id}`"
              @click="router.push(`/entry/${entry.header.id}`)"
            >
              {{ t('passwords.trash.view') }}
            </UiButton>
            <UiButton
              variant="outline"
              size="sm"
              :data-testid="`passwords-trash-restore-${entry.header.id}`"
              @click="restoreOneAsync(target('item', entry.header.id))"
            >
              {{ t('passwords.trash.restore') }}
            </UiButton>
            <UiButton
              variant="ghost"
              size="sm"
              :data-testid="`passwords-trash-delete-${entry.header.id}`"
              @click="
                askDelete(
                  target('item', entry.header.id),
                  titleOf(entry.header),
                )
              "
            >
              {{ t('passwords.trash.deleteForGood') }}
            </UiButton>
          </SettingsRow>
        </PasswordsEntryMenu>
      </SettingsGroup>
    </div>

    <PasswordsEmptyTrashDialog
      v-model:open="emptyDialog"
      :entries="totalEntries"
      :folders="totalFolders"
      :item-ids="trashedItemIds"
    />

    <ShadcnAlertDialog
      :open="deleting !== null"
      @update:open="(value: boolean) => !value && (deleting = null)"
    >
      <ShadcnAlertDialogContent v-if="deleting">
        <ShadcnAlertDialogHeader>
          <ShadcnAlertDialogTitle>{{
            t('passwords.trash.deleteTitle')
          }}</ShadcnAlertDialogTitle>
          <ShadcnAlertDialogDescription>
            {{
              t(
                deleting.kind === 'group'
                  ? 'passwords.trash.deleteFolderBody'
                  : 'passwords.trash.deleteBody',
                { name: deletingLabel },
              )
            }}
          </ShadcnAlertDialogDescription>
        </ShadcnAlertDialogHeader>
        <PasswordsReferenceUsageNote
          v-model:inline="inlineReferences"
          :item-ids="deletingItemIds"
        />
        <PasswordsPasskeyDeleteNote :item-ids="deletingItemIds" />
        <ShadcnAlertDialogFooter>
          <ShadcnAlertDialogCancel>{{
            t('passwords.cancel')
          }}</ShadcnAlertDialogCancel>
          <UiButton
            variant="destructive"
            data-testid="passwords-trash-delete-confirm"
            @click="confirmDeleteAsync"
          >
            {{ t('passwords.trash.deleteForGood') }}
          </UiButton>
        </ShadcnAlertDialogFooter>
      </ShadcnAlertDialogContent>
    </ShadcnAlertDialog>
  </div>
</template>
