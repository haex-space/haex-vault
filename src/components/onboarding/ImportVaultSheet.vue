<script setup lang="ts">
/**
 * "Vault-Datei öffnen" (spec 043 FR-002a, contract import-instance.md): the chosen vault file and
 * its passphrase. holzi copies the file into its own storage and opens the copy; the chosen file is
 * only read. An error goes out as a toast; the sheet stays open.
 */
import { toast } from 'vue-sonner'
import type { PickedFile } from '@bindings/PickedFile'

interface Props {
  open: boolean
  file: PickedFile | null
  fileName: string
}

const props = defineProps<Props>()
const emit = defineEmits<{
  'update:open': [value: boolean]
  imported: [name: string]
}>()

const { t } = useI18n()
const { errString } = useErrorString()
const { password: passwordLabels } = useFieldLabels()
const { importAsync } = useInstance()

const passphrase = ref('')
const submitting = ref(false)

watch(
  () => props.open,
  (isOpen) => {
    if (!isOpen) passphrase.value = ''
  },
)

const canSubmit = computed(
  () => passphrase.value.length > 0 && !submitting.value && props.file != null,
)

function message(cause: unknown): string {
  const kind =
    cause && typeof cause === 'object' && 'kind' in cause
      ? (cause as { kind: unknown }).kind
      : undefined
  switch (kind) {
    case 'WrongPassphrase':
      return t('onboarding.import.errors.wrongPassphrase')
    case 'NotAValidInstance':
      return t('onboarding.import.errors.notAVault')
    case 'AlreadyOnThisDevice':
      return t('onboarding.import.errors.alreadyHere', {
        name: String((cause as { name?: unknown }).name ?? ''),
      })
    case 'NotEnoughSpace':
      return t('onboarding.import.errors.notEnoughSpace')
    case 'Unreadable':
      return t('onboarding.import.errors.unreadable')
    case 'VaultAlreadyOpenElsewhere':
      return t('errors.vaultAlreadyOpenElsewhere')
    default:
      return errString(cause)
  }
}

async function onSubmit() {
  if (!canSubmit.value || props.file == null) return
  submitting.value = true
  try {
    const result = await importAsync({
      file: props.file,
      passphrase: passphrase.value,
    })
    // Kept only until the vault is open (as in the unlock sheet).
    passphrase.value = ''
    emit('imported', result.info.name)
    emit('update:open', false)
  } catch (cause) {
    toast.error(message(cause))
  } finally {
    submitting.value = false
  }
}
</script>

<template>
  <UiDrawerModal
    :open="props.open"
    :title="t('onboarding.import.title')"
    :description="props.fileName"
    @update:open="$emit('update:open', $event)"
  >
    <template #content>
      <form
        id="import-vault-form"
        class="space-y-4 px-6 py-2"
        data-testid="import-vault-sheet"
        @submit.prevent="onSubmit"
      >
        <p class="text-sm text-muted-foreground">
          {{ t('onboarding.import.hint') }}
        </p>
        <div class="space-y-1.5">
          <ShadcnLabel for="import-vault-passphrase">
            {{ t('onboarding.import.passphrase') }}
          </ShadcnLabel>
          <UiInputPassword
            id="import-vault-passphrase"
            v-model="passphrase"
            :labels="passwordLabels"
            autofocus
          />
        </div>
      </form>
    </template>
    <template #footer>
      <UiButton
        form="import-vault-form"
        type="submit"
        :disabled="!canSubmit"
        :loading="submitting"
        class="w-full"
        data-testid="import-vault-submit"
      >
        {{ t('onboarding.import.submit') }}
      </UiButton>
    </template>
  </UiDrawerModal>
</template>
