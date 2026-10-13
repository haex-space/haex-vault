<script setup lang="ts">
/**
 * A storage request of an extension (spec 038, US3, research R6), drawn over the extension's own
 * tab like `FrameDialog.vue`. It shows what the extension proposes and asks only for a
 * confirmation; for a new storage it offers the connections on the same endpoint and region, or new
 * credentials, which holzi then asks for in its own window over the whole app. This dialog NEVER
 * has a field for credentials: an extension can draw a look-alike in its frame, and what the user
 * types there would reach it. Escape and the cancel button answer "no".
 */
import { computed, onMounted, ref } from 'vue'
import {
  isFolderProtocol,
  type StorageChoice,
  type StorageRequest,
} from '~/composables/useStorageRequests'

const props = defineProps<{ request: StorageRequest }>()
const emit = defineEmits<{ answer: [choice: StorageChoice] }>()
const { t } = useI18n()

const root = ref<HTMLElement | null>(null)
const proposal = computed(() => props.request.proposal)
const connections = computed(() => proposal.value.connections ?? [])
/** Reka reserves the empty radio value for clearing the group, so use a non-empty sentinel. */
const NEW_CREDENTIALS_VALUE = '__holzi_new_credentials__'
/** For a new storage: an existing connection, or the sentinel for new credentials. */
const chosen = ref(connections.value[0]?.id ?? NEW_CREDENTIALS_VALUE)
/** For a change: new credentials in holzi's window. */
const newCredentials = ref(false)
/** WebDAV and SFTP keep a storage in a folder, which holzi makes only on the user's word. */
const inFolder = computed(() => isFolderProtocol(proposal.value.protocol))
const createFolder = ref(false)

const needsCredentials = computed(() => {
  if (props.request.kind === 'add')
    return (
      !proposal.value.sameProvider && chosen.value === NEW_CREDENTIALS_VALUE
    )
  return props.request.kind === 'update' && newCredentials.value
})

function confirm() {
  emit('answer', {
    confirm: true,
    connectionId:
      props.request.kind === 'add' && chosen.value !== NEW_CREDENTIALS_VALUE
        ? chosen.value
        : undefined,
    newCredentials: needsCredentials.value,
    createFolder: inFolder.value && createFolder.value,
  })
}

onMounted(() => {
  root.value
    ?.querySelector<HTMLElement>('[data-testid="storage-dialog-confirm"]')
    ?.focus()
})
</script>

<template>
  <div
    ref="root"
    class="flex items-center justify-center bg-background/70 p-4"
    data-testid="storage-dialog"
    @keydown.esc="emit('answer', { confirm: false })"
  >
    <div
      role="alertdialog"
      aria-modal="true"
      class="w-full max-w-md space-y-4 rounded-lg border border-border bg-card p-5 shadow-lg"
    >
      <p class="text-xs text-muted-foreground">
        {{ t('extensions.dialog.fromExtension') }}
      </p>
      <h2 class="text-base font-semibold">
        {{
          t(`extensions.storageDialog.title.${request.kind}`, {
            name: request.extensionName,
          })
        }}
      </h2>

      <dl class="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-sm">
        <template v-if="proposal.name">
          <dt class="text-muted-foreground">
            {{ t('extensions.storageDialog.name') }}
          </dt>
          <dd data-testid="storage-dialog-name">
            <span
              v-if="
                proposal.currentName && proposal.currentName !== proposal.name
              "
            >
              {{ proposal.currentName }} →
            </span>
            {{ proposal.name }}
          </dd>
        </template>
        <template v-if="proposal.bucket">
          <dt class="text-muted-foreground">
            {{
              inFolder
                ? t('extensions.storageDialog.folder')
                : t('extensions.storageDialog.bucket')
            }}
          </dt>
          <dd class="font-mono text-xs break-all">
            <span
              v-if="
                proposal.currentBucket &&
                proposal.currentBucket !== proposal.bucket
              "
            >
              {{ proposal.currentBucket }} →
            </span>
            {{ proposal.bucket }}
          </dd>
        </template>
        <template v-if="proposal.providerName">
          <dt class="text-muted-foreground">
            {{ t('extensions.storageDialog.provider') }}
          </dt>
          <dd>{{ proposal.providerName }}</dd>
        </template>
        <template v-if="proposal.endpoint">
          <dt class="text-muted-foreground">
            {{ t('extensions.storageDialog.endpoint') }}
          </dt>
          <dd
            class="font-mono text-xs break-all"
            data-testid="storage-dialog-endpoint"
          >
            {{ proposal.endpoint }}
          </dd>
        </template>
      </dl>

      <div class="flex flex-wrap gap-2">
        <span
          v-if="proposal.scope === 'local'"
          class="rounded-md bg-muted px-2 py-0.5 text-xs"
          data-testid="storage-dialog-local"
        >
          {{ t('extensions.storageDialog.local') }}
        </span>
        <span
          v-if="proposal.insecure"
          class="rounded-md bg-destructive/15 px-2 py-0.5 text-xs text-destructive"
          data-testid="storage-dialog-insecure"
        >
          {{ t('extensions.storageDialog.insecure') }}
        </span>
      </div>

      <fieldset
        v-if="request.kind === 'add' && !proposal.sameProvider"
        class="space-y-2 text-sm"
      >
        <legend class="mb-1 text-muted-foreground">
          {{ t('extensions.storageDialog.credentialsFrom') }}
        </legend>
        <ShadcnRadioGroup
          class="gap-2"
          :model-value="chosen"
          @update:model-value="chosen = String($event)"
        >
          <label
            v-for="connection in connections"
            :key="connection.id"
            class="flex items-center gap-2"
          >
            <ShadcnRadioGroupItem :value="connection.id" />
            {{
              t('extensions.storageDialog.existing', {
                name: connection.providerName,
              })
            }}
          </label>
          <label class="flex items-center gap-2">
            <ShadcnRadioGroupItem
              :value="NEW_CREDENTIALS_VALUE"
              data-testid="storage-dialog-new-credentials"
            />
            {{ t('extensions.storageDialog.newCredentials') }}
          </label>
        </ShadcnRadioGroup>
      </fieldset>
      <label
        v-if="request.kind === 'update'"
        class="flex items-center gap-2 text-sm"
      >
        <ShadcnCheckbox
          v-model="newCredentials"
          data-testid="storage-dialog-new-credentials"
        />
        {{ t('extensions.storageDialog.newCredentials') }}
      </label>
      <label
        v-if="inFolder && (request.kind === 'add' || request.kind === 'update')"
        class="flex items-center gap-2 text-sm"
      >
        <ShadcnCheckbox
          v-model="createFolder"
          data-testid="storage-dialog-create-folder"
        />
        {{ t('extensions.storageDialog.createFolder') }}
      </label>
      <p v-if="needsCredentials" class="text-xs text-muted-foreground">
        {{ t('extensions.storageDialog.credentialsNext') }}
      </p>

      <p
        v-if="request.kind === 'remove' && request.otherExtensions.length > 0"
        class="rounded-xl bg-destructive/10 px-3 py-2 text-sm text-destructive"
        role="alert"
        data-testid="storage-dialog-others"
      >
        {{
          t('extensions.storageDialog.others', {
            names: request.otherExtensions.join(', '),
          })
        }}
      </p>

      <div class="flex justify-end gap-2">
        <UiButton
          type="button"
          variant="outline"
          data-testid="storage-dialog-cancel"
          @click="emit('answer', { confirm: false })"
        >
          {{ t('extensions.dialog.cancel') }}
        </UiButton>
        <UiButton
          type="button"
          :variant="request.kind === 'remove' ? 'destructive' : 'default'"
          data-testid="storage-dialog-confirm"
          @click="confirm"
        >
          {{
            needsCredentials
              ? t('extensions.storageDialog.continue')
              : t(`extensions.storageDialog.confirm.${request.kind}`)
          }}
        </UiButton>
      </div>
    </div>
  </div>
</template>
