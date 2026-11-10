<script setup lang="ts">
import { toast } from 'vue-sonner'

interface Props {
  open: boolean
  name: string | null
}

const props = defineProps<Props>()
const emit = defineEmits<{
  'update:open': [value: boolean]
  unlocked: [name: string]
}>()

const { t } = useI18n()
const { password: passwordLabels } = useFieldLabels()
const { openAsync } = useInstance()

const passphrase = ref('')
const submitting = ref(false)

function reset() {
  passphrase.value = ''
}

watch(
  () => props.open,
  (isOpen) => {
    if (!isOpen) reset()
  },
)

const canSubmit = computed(
  () => passphrase.value.length > 0 && !submitting.value && props.name != null,
)

function errorKind(e: unknown): unknown {
  if (e && typeof e === 'object' && 'kind' in e) {
    return (e as { kind: unknown }).kind
  }
  if (typeof e !== 'string') return undefined

  try {
    const parsed: unknown = JSON.parse(e)
    return parsed && typeof parsed === 'object' && 'kind' in parsed
      ? (parsed as { kind: unknown }).kind
      : undefined
  } catch {
    return undefined
  }
}

async function onSubmit() {
  if (!canSubmit.value || props.name == null) return
  submitting.value = true
  try {
    const info = await openAsync({
      name: props.name,
      passphrase: passphrase.value,
    })
    // FR-016: keep the value only until the unlock succeeds. Clear it here, before emitting —
    // `onUnlocked` navigates away instead of ever setting `open` back to `false`, so the dismiss
    // watch's `reset()` below would otherwise never run for the success path.
    passphrase.value = ''
    emit('unlocked', info.name)
    emit('update:open', false)
  } catch (e) {
    const kind = errorKind(e)
    let message: string
    switch (kind) {
      case 'WrongPassphrase':
        message = t('errors.wrongPassphrase')
        break
      case 'NotFound':
        message = t('errors.vaultNotFound')
        break
      case 'VaultAlreadyOpenElsewhere':
        message = t('errors.vaultAlreadyOpenElsewhere')
        break
      case 'VaultAlreadyActive':
        message = t('errors.vaultAlreadyActive')
        break
      case 'CrdtIo':
        message = t('errors.vaultIo')
        break
      case 'CrdtSqlite':
      case 'NotAValidInstance':
        message = t('errors.vaultUnreadable')
        break
      case 'PathResolution':
      case 'Io':
        message = t('errors.vaultIo')
        break
      case 'MigrationMissingFromSource':
        // The vault holds a migration this build does not know: it was last opened by a newer one.
        message = t('errors.vaultNewerVersion')
        break
      case 'CrdtInit':
      case 'CrdtHlc':
      case 'DeviceIdMismatch':
      case 'MigrationContentDrift':
      case 'MigrationCompatibility':
      case 'CrdtSignatureVerificationFailed':
      case 'CrdtUnexpectedSignatureUnderNoop':
      case 'CrdtAlreadyInstalled':
        message = t('errors.vaultIncompatible')
        break
      default:
        message = t('errors.openFailed')
    }
    toast.error(message)
  } finally {
    submitting.value = false
  }
}
</script>

<template>
  <UiDrawerModal
    :open="props.open"
    :title="t('onboarding.unlock.title')"
    :description="props.name ?? undefined"
    @update:open="$emit('update:open', $event)"
  >
    <template #content>
      <form
        id="unlock-form"
        class="space-y-4 px-6 py-2"
        @submit.prevent="onSubmit"
      >
        <div class="space-y-1.5">
          <ShadcnLabel for="unlock-passphrase">
            {{ t('onboarding.unlock.passphrase') }}
          </ShadcnLabel>
          <UiInputPassword
            id="unlock-passphrase"
            v-model="passphrase"
            :labels="passwordLabels"
            autofocus
            data-testid="unlock-passphrase"
          />
        </div>
      </form>
    </template>
    <template #footer>
      <UiButton
        form="unlock-form"
        type="submit"
        :disabled="!canSubmit"
        :loading="submitting"
        class="w-full"
        data-testid="unlock-submit"
      >
        {{ t('onboarding.unlock.submit') }}
      </UiButton>
    </template>
  </UiDrawerModal>
</template>
