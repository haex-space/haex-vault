<template>
  <UiDrawerModal
    v-model:open="open"
    :title="t('title')"
  >
    <template #body>
      <div class="space-y-4">
        <div class="flex justify-center">
          <UiAvatarPicker
            :model-value="target?.avatar"
            :avatar-options="parsedAvatarOptions"
            :seed="target?.publicKey"
            avatar-style="toon-head"
            size="xl"
            @update:model-value="onAvatarModelUpdate"
            @update:avatar-options="onAvatarOptionsUpdate"
          />
        </div>

        <UiInput
          v-model="label"
          :label="t('labelField')"
          @keydown.enter.prevent="onSubmit"
        />

        <USeparator :label="t('changePassword')" />

        <UiInputPassword
          v-model="password"
          :label="t('identityPassword')"
          :description="t('passwordOptional')"
          leading-icon="i-lucide-lock"
        />
        <UiInputPassword
          v-if="password"
          v-model="passwordConfirm"
          :label="t('identityPasswordConfirm')"
          leading-icon="i-lucide-lock"
        />
        <p
          v-if="passwordConfirm && password !== passwordConfirm"
          class="text-sm text-error -mt-3"
        >
          {{ t('passwordMismatch') }}
        </p>
      </div>
    </template>
    <template #footer>
      <div class="flex justify-between gap-4">
        <UButton
          color="neutral"
          variant="outline"
          @click="open = false"
        >
          {{ t('cancel') }}
        </UButton>
        <UiButton
          icon="i-lucide-check"
          :loading="submitting"
          :disabled="!canSave"
          @click="onSubmit"
        >
          {{ t('submit') }}
        </UiButton>
      </div>
    </template>
  </UiDrawerModal>
</template>

<script setup lang="ts">
import type { SelectHaexIdentities } from '~/database/schemas'

export interface EditSubmitPayload {
  label: string
  /** Empty when the user did not set a new password. */
  newPassword: string
}

export interface AvatarUpdatePayload {
  avatar: string | null
  /** undefined = no change, null = cleared, object = new options. */
  options: Record<string, unknown> | null | undefined
}

const open = defineModel<boolean>('open', { required: true })

const props = defineProps<{
  target: SelectHaexIdentities | null
  submitting: boolean
}>()

const emit = defineEmits<{
  submit: [payload: EditSubmitPayload]
  'avatar-update': [payload: AvatarUpdatePayload]
}>()

const { t } = useI18n()

const label = ref('')
const password = ref('')
const passwordConfirm = ref('')

// AvatarPicker emits `update:avatarOptions` before `update:modelValue`.
// We buffer the options and flush both together when modelValue arrives.
const pendingAvatarOptions = ref<
  Record<string, unknown> | null | undefined
>(undefined)

const parsedAvatarOptions = computed(() => {
  if (!props.target?.avatarOptions) return null
  try {
    return JSON.parse(props.target.avatarOptions)
  } catch {
    return null
  }
})

const canSave = computed(() => {
  if (!label.value.trim()) return false
  if (password.value) {
    return (
      password.value.length >= 8 && password.value === passwordConfirm.value
    )
  }
  return true
})

// Seed form fields whenever the dialog opens with a fresh target.
watch(
  () => [open.value, props.target] as const,
  ([isOpen, target]) => {
    if (!isOpen || !target) return
    label.value = target.label
    password.value = ''
    passwordConfirm.value = ''
    pendingAvatarOptions.value = undefined
  },
)

const onAvatarOptionsUpdate = (options: Record<string, unknown> | null) => {
  pendingAvatarOptions.value = options
}

const onAvatarModelUpdate = (avatar: string | null) => {
  emit('avatar-update', {
    avatar,
    options: pendingAvatarOptions.value,
  })
  pendingAvatarOptions.value = undefined
}

const onSubmit = () => {
  if (!canSave.value) return
  emit('submit', {
    label: label.value.trim(),
    newPassword: password.value,
  })
}
</script>

<i18n lang="yaml">
de:
  title: Identität bearbeiten
  labelField: Name
  changePassword: Passwort ändern
  identityPassword: Identity-Passwort
  identityPasswordConfirm: Identity-Passwort bestätigen
  passwordOptional: Leer lassen, um das Passwort nicht zu ändern
  passwordMismatch: Passwörter stimmen nicht überein
  submit: Speichern
  cancel: Abbrechen
en:
  title: Edit Identity
  labelField: Name
  changePassword: Change password
  identityPassword: Identity password
  identityPasswordConfirm: Confirm identity password
  passwordOptional: Leave empty to keep the current password
  passwordMismatch: Passwords do not match
  submit: Save
  cancel: Cancel
</i18n>
