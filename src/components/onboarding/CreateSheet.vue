<script setup lang="ts">
import { toast } from 'vue-sonner'

interface Props {
  open: boolean
}

const props = defineProps<Props>()
const emit = defineEmits<{
  'update:open': [value: boolean]
  created: [name: string]
}>()

const { t } = useI18n()
const fieldLabels = useFieldLabels()
const { createAsync } = useInstance()

const name = ref('')
const passphrase = ref('')
const passphraseConfirm = ref('')
const submitting = ref(false)

function reset() {
  name.value = ''
  passphrase.value = ''
  passphraseConfirm.value = ''
}

watch(
  () => props.open,
  (isOpen) => {
    if (!isOpen) reset()
  },
)

const canSubmit = computed(
  () =>
    name.value.length > 0 &&
    passphrase.value.length >= 8 &&
    passphrase.value === passphraseConfirm.value &&
    !submitting.value,
)

async function onSubmit() {
  if (!canSubmit.value) return
  submitting.value = true
  try {
    const result = await createAsync({
      name: name.value,
      passphrase: passphrase.value,
    })
    // FR-016: keep the value only until the create succeeds. Clear it here, before emitting —
    // `onCreated` navigates away instead of ever setting `open` back to `false`, so the dismiss
    // watch's `reset()` below would otherwise never run for the success path.
    passphrase.value = ''
    passphraseConfirm.value = ''
    emit('created', result.info.name)
    emit('update:open', false)
  } catch (e: unknown) {
    const kind = (e as { kind?: string })?.kind
    toast.error(t(`errors.${kind ?? 'openFailed'}`, t('errors.openFailed')))
  } finally {
    submitting.value = false
  }
}
</script>

<template>
  <UiDrawerModal
    :open="props.open"
    :title="t('onboarding.create.title')"
    @update:open="$emit('update:open', $event)"
  >
    <template #content>
      <form
        id="create-form"
        class="space-y-4 px-6 py-2"
        @submit.prevent="onSubmit"
      >
        <UiInput
          id="create-name"
          v-model="name"
          :label="t('onboarding.create.name')"
          :labels="fieldLabels.input.value"
          autofocus
        />
        <UiInputPassword
          id="create-passphrase"
          v-model="passphrase"
          :label="t('onboarding.create.passphrase')"
          :labels="fieldLabels.password.value"
        />
        <UiInputPassword
          id="create-passphrase-confirm"
          v-model="passphraseConfirm"
          :label="t('onboarding.create.passphraseConfirm')"
          :labels="fieldLabels.password.value"
        />
      </form>
    </template>
    <template #footer>
      <UiButton
        form="create-form"
        type="submit"
        :disabled="!canSubmit"
        :loading="submitting"
        class="w-full"
      >
        {{ t('onboarding.create.submit') }}
      </UiButton>
    </template>
  </UiDrawerModal>
</template>
