<script setup lang="ts">
/**
 * "Meine Dateifreigaben" (spec 056 US4, FR-020): every share of the vault with its source, the
 * device that serves it, its contacts with their level and its state. A share opens in the share
 * dialog to change its name and contacts; a contact's access is taken away here at once (FR-024),
 * and a change made on this device shows as waiting until the serving device has seen it (FR-026).
 */
import type { FileShare } from '@bindings/FileShare'
import type { FileShareGrant } from '@bindings/FileShareGrant'
import type { FilesContactShares } from '@bindings/FilesContactShares'
import { asFilesError } from '~/composables/useFiles'
import { useActionOrThrow } from '~/composables/useActionOrThrow'
import { onVaultTablesChanged } from '~/composables/useVaultData'

defineProps<{
  /** The contacts with a stored card (`Sources.contacts`), for the share dialog. */
  contacts: readonly FilesContactShares[]
}>()

const open = defineModel<boolean>('open', { required: true })

const { t } = useI18n()
const listShares = useActionOrThrow('files.shares.list')
const revokeGrant = useActionOrThrow('files.shares.revoke')

const shares = ref<FileShare[] | null>(null)
const error = ref<string | null>(null)
const busy = ref(false)
const editing = ref<FileShare | null>(null)
const editOpen = ref(false)

function failed(cause: unknown) {
  const code = asFilesError(cause)?.code
  error.value = code ? t(`files.error.${code}`) : t('files.shares.failed')
}

async function reloadAsync() {
  if (!open.value) return
  try {
    shares.value = (await listShares()) as FileShare[]
    error.value = null
  } catch (cause) {
    failed(cause)
  }
}

watch(open, (isOpen) => {
  if (isOpen) void reloadAsync()
})
onVaultTablesChanged(['haex_file_share*'], reloadAsync)

function deviceName(share: FileShare): string {
  if (share.servingDevice.isCurrent)
    return t('files.shares.overview.thisDevice')
  return share.servingDevice.name ?? t('files.peer.unnamed')
}

function stateText(share: FileShare): string {
  return t(`files.shares.overview.state.${share.state}`, {
    device: deviceName(share),
  })
}

function contactText(grant: FileShareGrant): string {
  return grant.contactName ?? grant.vaultPubkey.slice(0, 8)
}

function edit(share: FileShare) {
  editing.value = share
  editOpen.value = true
}

async function revokeAsync(share: FileShare, grant: FileShareGrant) {
  if (busy.value) return
  busy.value = true
  try {
    await revokeGrant({ shareId: share.id, vaultPubkey: grant.vaultPubkey })
    await reloadAsync()
  } catch (cause) {
    failed(cause)
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <UiDrawerModal v-model:open="open" :title="t('files.shares.overview.title')">
    <template #content>
      <div class="flex flex-col gap-3" data-testid="files-shares-overview">
        <p
          v-if="error"
          class="text-sm text-destructive"
          role="alert"
          data-testid="files-shares-overview-error"
        >
          {{ error }}
        </p>
        <p
          v-if="shares && shares.length === 0"
          class="text-sm text-muted-foreground"
        >
          {{ t('files.shares.overview.empty') }}
        </p>
        <ul v-if="shares?.length" class="flex flex-col gap-3">
          <li
            v-for="share in shares"
            :key="share.id"
            class="flex flex-col gap-2 rounded-lg border p-3"
            :data-testid="`files-shares-overview-${share.name}`"
          >
            <div class="flex items-start gap-2">
              <Icon
                :name="
                  share.itemKind === 'folder' ? 'lucide:folder' : 'lucide:file'
                "
                class="mt-0.5 size-4 shrink-0 text-muted-foreground"
              />
              <div class="flex min-w-0 flex-1 flex-col">
                <span class="truncate font-medium">{{ share.name }}</span>
                <span class="truncate text-xs text-muted-foreground">
                  {{ share.path }} · {{ deviceName(share) }}
                </span>
                <span
                  class="text-xs"
                  :class="
                    share.state === 'active'
                      ? 'text-muted-foreground'
                      : 'text-warning'
                  "
                  :data-testid="`files-shares-overview-state-${share.name}`"
                  >{{ stateText(share) }}</span
                >
              </div>
              <UiButton
                type="button"
                variant="outline"
                size="sm"
                :data-testid="`files-shares-overview-edit-${share.name}`"
                @click="edit(share)"
                >{{ t('files.shares.overview.edit') }}</UiButton
              >
            </div>
            <p
              v-if="share.grants.length === 0"
              class="text-xs text-muted-foreground"
            >
              {{ t('files.shares.overview.noContacts') }}
            </p>
            <ul v-else class="flex flex-col gap-1">
              <li
                v-for="grant in share.grants"
                :key="grant.vaultPubkey"
                class="flex items-center gap-2 text-sm"
              >
                <span class="min-w-0 flex-1 truncate">{{
                  contactText(grant)
                }}</span>
                <span class="text-xs text-muted-foreground">{{
                  t(`files.shares.level.${grant.level}`)
                }}</span>
                <UiButton
                  type="button"
                  variant="ghost"
                  size="icon-sm"
                  :disabled="busy"
                  :aria-label="t('files.shares.remove')"
                  :tooltip="t('files.shares.remove')"
                  :data-testid="`files-shares-overview-revoke-${contactText(grant)}`"
                  @click="revokeAsync(share, grant)"
                >
                  <Icon name="lucide:x" class="size-4" />
                </UiButton>
              </li>
            </ul>
          </li>
        </ul>
      </div>
    </template>
  </UiDrawerModal>
  <FilesShareDialog
    v-model:open="editOpen"
    :entry="null"
    :share="editing"
    :contacts="contacts"
  />
</template>
