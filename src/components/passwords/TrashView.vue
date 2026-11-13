<script setup lang="ts">
/**
 * The trash (spec 034, US4, FR-015, FR-016): the folders and entries that were deleted, each with
 * the folder path it came from, with "Restore" and "Delete for good" (after a confirmation naming
 * what goes), and "Empty trash" (after a confirmation that names the number). An entry in a trashed
 * folder is part of that folder's row. Only the user can do any of this.
 *
 * One trash holds the entries of both apps (spec 092, Phase W): the password manager's trash shows
 * and empties the logins and their folders, Wallet's (`scope="pass"`) the cards and tickets, also
 * those inside a folder that was deleted (a pass can sit in a password-manager folder from before
 * Wallet). Neither can restore, delete or empty what the other shows, so deleting a folder never
 * takes a pass with it.
 */
import { toast } from 'vue-sonner'
import type { ItemHeader } from '@bindings/ItemHeader'
import type { Target } from '@bindings/Target'
import { displayTitle } from '~/lib/passwords/format'
import {
  deletePlan as deletePlanOf,
  emptyPlan as emptyPlanOf,
  inScope,
  scopeOf,
} from '~/lib/passwords/scope'
import { buildMenu, type MenuCommand } from '~/lib/passwords/menus'
import { groupPath, trashGroupIds, TRASH_GROUP_ID } from '~/lib/passwords/tree'

const props = withDefaults(
  defineProps<{
    /** Whose trash this is: the logins' (password manager) or the passes' (Wallet). */
    scope?: 'login' | 'pass'
  }>(),
  { scope: 'login' },
)

const { t } = useI18n()
const { errString } = useErrorString()
const router = useTabRouter()
const store = usePasswordsStore()
const { restoreAsync, deletePermanentlyAsync } = usePasswords()

const trashIds = computed(() => trashGroupIds(store.groups))

/** The entries this trash shows: passes in Wallet's, everything else in the password manager's. */
const scoped = computed(() => inScope(store.headers, props.scope))

/** Folders deleted directly (their parent is the trash) with the number of entries inside. */
const folders = computed(() =>
  store.groups
    .filter(
      (group) => props.scope === 'login' && group.parentId === TRASH_GROUP_ID,
    )
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
        entries: scoped.value.filter((h) => h.groupId && inside.has(h.groupId))
          .length,
        passes: store.headers.filter(
          (h) => scopeOf(h) === 'pass' && h.groupId && inside.has(h.groupId),
        ).length,
      }
    }),
)

/** The entries this trash lists as rows: those deleted directly and, in Wallet's, the passes inside
 * a deleted folder too, as Wallet shows no folders. In the password manager an entry of a deleted
 * folder is part of that folder's row. */
const entries = computed(() =>
  scoped.value
    .filter((header) =>
      props.scope === 'pass'
        ? header.groupId !== null && trashIds.value.has(header.groupId)
        : header.groupId === TRASH_GROUP_ID,
    )
    .map((header) => ({
      header,
      path:
        header.groupId === TRASH_GROUP_ID
          ? pathOf(header.trashedFromGroupId)
          : pathOf(header.groupId),
    })),
)

/** What "Empty trash" removes in this scope, and the counts the dialog names. */
const emptyPlan = computed(() =>
  emptyPlanOf(props.scope, TRASH_GROUP_ID, store.groups, store.headers),
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

/** What a delete for good of the chosen row removes: the entry, or the folder with every entry
 * inside (spec 036), or only the logins of a folder that holds a pass (Wallet's to delete). */
const deletingPlan = computed(() =>
  deleting.value
    ? deletePlanOf(props.scope, deleting.value, store.groups, store.headers)
    : { targets: [], itemIds: [] },
)
const deletingItemIds = computed(() => deletingPlan.value.itemIds)

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
  if (
    deletePlanOf(props.scope, item, store.groups, store.headers).targets
      .length === 0
  )
    return
  deleting.value = item
  deletingLabel.value = label
  inlineReferences.value = true
}

async function confirmDeleteAsync() {
  const targets = deletingPlan.value.targets
  deleting.value = null
  if (targets.length === 0) return
  try {
    await deletePermanentlyAsync(targets, inlineReferences.value)
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
          :disabled="emptyPlan.targets.length === 0"
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
              ) +
              (folder.passes > 0
                ? ' · ' +
                  t(
                    'passwords.trash.passesStay',
                    { count: folder.passes },
                    folder.passes,
                  )
                : '')
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
              :disabled="folder.passes > 0 && folder.entries === 0"
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
            :icon="scope === 'pass' ? 'lucide:wallet' : 'lucide:key-round'"
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
      :entries="emptyPlan.entries"
      :folders="emptyPlan.folders"
      :item-ids="emptyPlan.itemIds"
      :targets="emptyPlan.targets"
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
