<script setup lang="ts">
/**
 * holzi's window for the credentials of a storage an extension asked for (spec 038, US3, FR-013,
 * research R6). Mounted in the desktop, not in an extension's frame, so it lies over the tab bar
 * and the toolbar, which no extension can draw. It is the only place besides Einstellungen →
 * Speicher with fields for credentials; they go to Rust (`storage_dialog_resolve`), are tested
 * there and stored in the password manager, and never reach the extension. The window stays open
 * while holzi tests them; a failed test shows why, as in the settings, and the user corrects them or
 * cancels. Closing cancels.
 */
import { reactive, ref, watch } from 'vue'
import { useStorageCredentials } from '~/composables/useStorageRequests'

const { t } = useI18n()
const fieldLabels = useFieldLabels()
const { request, confirmAsync, cancel } = useStorageCredentials()

const draft = reactive({
  accessKeyId: '',
  secretAccessKey: '',
  sessionToken: '',
})
const failure = ref<string | null>(null)
const busy = ref(false)

watch(
  () => request.value?.requestId,
  () => {
    draft.accessKeyId = ''
    draft.secretAccessKey = ''
    draft.sessionToken = ''
    failure.value = null
    busy.value = false
  },
)

async function confirm() {
  if (busy.value) return
  if (!draft.accessKeyId.trim() || !draft.secretAccessKey) {
    failure.value = t('settings.storage.invalid.credentials')
    return
  }
  failure.value = null
  busy.value = true
  const trial = await confirmAsync({
    kind: 's3',
    accessKeyId: draft.accessKeyId.trim(),
    secretAccessKey: draft.secretAccessKey,
    sessionToken: draft.sessionToken.trim() || undefined,
  })
  busy.value = false
  if (trial.kind === 'failed') {
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
        @submit.prevent="confirm"
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
        <UiInput
          id="storage-credentials-access-key"
          v-model="draft.accessKeyId"
          :label="t('settings.storage.fields.accessKeyId')"
          :labels="fieldLabels.input.value"
          autocomplete="off"
          data-testid="storage-credentials-access-key"
        />
        <UiInputPassword
          id="storage-credentials-secret"
          v-model="draft.secretAccessKey"
          :label="t('settings.storage.fields.secretAccessKey')"
          :labels="fieldLabels.password.value"
          autocomplete="new-password"
          data-testid="storage-credentials-secret"
        />
        <UiInputPassword
          id="storage-credentials-session-token"
          v-model="draft.sessionToken"
          :label="t('settings.storage.fields.sessionToken')"
          :labels="fieldLabels.password.value"
          autocomplete="off"
          data-testid="storage-credentials-session-token"
        />
        <p
          v-if="failure"
          class="text-sm text-destructive"
          role="alert"
          data-testid="storage-credentials-failure"
        >
          {{ failure }}
        </p>
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
