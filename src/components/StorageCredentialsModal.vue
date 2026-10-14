<script setup lang="ts">
/**
 * holzi's window for the credentials of a storage an extension asked for (spec 038, US3, FR-013,
 * research R6). Mounted in the desktop, not in an extension's frame, so it lies over the tab bar
 * and the toolbar, which no extension can draw. It is the only place besides Einstellungen →
 * Speicher with fields for credentials; they go to Rust (`storage_dialog_resolve`), are tested
 * there and stored in the password manager, and never reach the extension. The window stays open
 * while holzi tests them; a failed test shows why, as in the settings, and the user corrects them or
 * cancels. Closing cancels. WebDAV and SFTP ask for a login, SFTP also by key (spec 050); when its
 * folder is missing the user may say here to make it, and an SFTP server's host key is shown here
 * to compare and confirm before holzi sends any credentials (research R5).
 */
import type { ShownHostKey } from '@bindings/ShownHostKey'
import type { TestOutcome } from '@bindings/TestOutcome'
import { computed, ref, watch } from 'vue'
import CredentialsEntryPicker from '~/components/settings/storage/CredentialsEntryPicker.vue'
import CredentialsFields from '~/components/settings/storage/CredentialsFields.vue'
import HostKeyConfirm from '~/components/settings/storage/HostKeyConfirm.vue'
import {
  credentialsComplete,
  credentialsInput,
  emptyCredentials,
} from '~/components/settings/storage/credentials'
import { useStorageCredentials } from '~/composables/useStorageRequests'

const { t } = useI18n()
const { request, confirmAsync, cancel } = useStorageCredentials()

const credentials = ref(emptyCredentials())
const failure = ref<string | null>(null)
const failedWith = ref<TestOutcome | null>(null)
/** The host key the server showed, waiting for the user's word. */
const shownKey = ref<ShownHostKey | null>(null)
const busy = ref(false)
const protocol = computed(() => request.value?.proposal.protocol ?? 's3')
const sourceOptions = computed(() =>
  (['new', 'entry'] as const).map((source) => ({
    value: source,
    label: t(`settings.storage.credentialsSource.${source}`),
  })),
)

watch(
  () => request.value?.requestId,
  () => {
    credentials.value = emptyCredentials()
    failure.value = null
    failedWith.value = null
    shownKey.value = null
    busy.value = false
  },
)

async function confirm(createFolder = false, hostKey?: string) {
  if (busy.value) return
  if (credentials.value.source === 'entry' && !credentials.value.entryId) {
    failure.value = t('settings.storage.entryPicker.required')
    return
  }
  if (!credentialsComplete(protocol.value, credentials.value)) {
    failure.value = t('settings.storage.invalid.credentials')
    return
  }
  failure.value = null
  failedWith.value = null
  shownKey.value = null
  busy.value = true
  const trial = await confirmAsync(
    credentialsInput(protocol.value, credentials.value),
    createFolder,
    hostKey,
  )
  busy.value = false
  if (trial.kind === 'failed') {
    failedWith.value = trial.outcome
    shownKey.value = trial.hostKey ?? null
    const text = t(`settings.storage.outcome.${trial.outcome}`)
    failure.value = trial.leftoverKey
      ? `${text}. ${t('settings.storage.leftover', { key: trial.leftoverKey })}`
      : text
  }
}

function onUpdateOpen(open: boolean) {
  if (!open) cancel()
}
</script>

<template>
  <UiDrawerModal
    v-if="request"
    :open="true"
    :title="t('extensions.storageCredentials.title')"
    @update:open="onUpdateOpen"
  >
    <template #content>
      <form
        id="storage-credentials-form"
        class="space-y-3"
        data-testid="storage-credentials-modal"
        @submit.prevent="confirm()"
      >
        <p class="text-sm">
          {{
            t('extensions.storageCredentials.for', {
              name: request.extensionName,
              provider: request.proposal.providerName ?? '',
            })
          }}
        </p>
        <p
          v-if="request.proposal.endpoint"
          class="font-mono text-xs break-all text-muted-foreground"
        >
          {{ request.proposal.endpoint }} · {{ request.proposal.bucket }}
        </p>
        <p class="rounded-xl bg-muted px-3 py-2 text-xs">
          {{ t('extensions.storageCredentials.onlyHere') }}
        </p>
        <SettingsGroup>
          <SettingsRow
            :title="t('settings.storage.credentialsSource.label')"
            label-for="storage-dialog-credentials-source"
          >
            <SettingsSelect
              id="storage-dialog-credentials-source"
              v-model="credentials.source"
              :options="sourceOptions"
              data-testid="storage-dialog-credentials-source"
            />
          </SettingsRow>
          <CredentialsFields
            v-if="credentials.source === 'new'"
            v-model="credentials"
            :protocol="protocol"
          />
        </SettingsGroup>
        <CredentialsEntryPicker
          v-if="credentials.source === 'entry'"
          v-model="credentials.entryId"
          :protocol="protocol"
        />
        <p
          v-if="failure"
          class="text-sm text-destructive"
          role="alert"
          data-testid="storage-credentials-failure"
        >
          {{ failure }}
        </p>
        <HostKeyConfirm
          v-if="shownKey"
          :shown="shownKey"
          :busy="busy"
          @confirm="confirm(false, shownKey.key)"
          @cancel="shownKey = null"
        />
      </form>
    </template>
    <template #footer>
      <div class="flex justify-end gap-2">
        <UiButton
          type="button"
          size="sm"
          variant="outline"
          data-testid="storage-credentials-cancel"
          @click="cancel"
        >
          {{ t('extensions.dialog.cancel') }}
        </UiButton>
        <UiButton
          v-if="failedWith === 'folderMissing'"
          type="button"
          size="sm"
          variant="outline"
          :disabled="busy"
          data-testid="storage-credentials-create-folder"
          @click="confirm(true)"
        >
          {{ t('settings.storage.createFolderAndSave') }}
        </UiButton>
        <UiButton
          type="submit"
          form="storage-credentials-form"
          size="sm"
          :loading="busy"
          data-testid="storage-credentials-confirm"
        >
          {{
            busy
              ? t('settings.storage.testing')
              : t('extensions.storageCredentials.confirm')
          }}
        </UiButton>
      </div>
    </template>
  </UiDrawerModal>
</template>
