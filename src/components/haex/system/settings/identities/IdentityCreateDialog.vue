<template>
  <UiDrawerModal
    v-model:open="open"
    :title="t('title')"
    :description="t('description')"
  >
    <template #body>
      <div class="space-y-4">
        <div class="flex justify-center">
          <UiAvatarPicker
            v-model="form.avatar"
            v-model:avatar-options="form.avatarOptions"
            :seed="form.label || 'new'"
            avatar-style="toon-head"
            size="xl"
          />
        </div>

        <UiInput
          v-model="form.label"
          :label="t('labelField')"
          :placeholder="t('labelPlaceholder')"
        />

        <USeparator :label="t('syncCredentials')" />

        <UiInput
          v-model="form.email"
          label="Email"
          placeholder="user@example.com"
          leading-icon="i-lucide-mail"
          type="email"
          required
          :custom-validators="[emailValidator]"
          check
        />

        <UCheckbox
          v-model="form.useVaultPassword"
          :label="t('useVaultPassword')"
        />

        <template v-if="!form.useVaultPassword">
          <UiInputPassword
            v-model="form.password"
            :label="t('identityPassword')"
            :description="t('identityPasswordDescription')"
            leading-icon="i-lucide-lock"
          />
          <UiInputPassword
            v-model="form.passwordConfirm"
            :label="t('identityPasswordConfirm')"
            leading-icon="i-lucide-lock"
          />
          <p
            v-if="form.passwordConfirm && form.password !== form.passwordConfirm"
            class="text-sm text-error -mt-3"
          >
            {{ t('passwordMismatch') }}
          </p>
        </template>

        <USeparator :label="t('claimsOptional')" />

        <UiInput
          v-model="form.name"
          label="Name"
          placeholder="Max Mustermann"
          leading-icon="i-lucide-user"
        />
        <UiInput
          v-model="form.phone"
          :label="t('phone')"
          placeholder="+49 123 456789"
          leading-icon="i-lucide-phone"
        />
        <UiInput
          v-model="form.address"
          :label="t('address')"
          placeholder="Musterstraße 1, 12345 Berlin"
          leading-icon="i-lucide-map-pin"
        />
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
          icon="i-lucide-plus"
          :loading="submitting"
          :disabled="!canCreate"
          @click="onSubmit"
        >
          {{ t('submit') }}
        </UiButton>
      </div>
    </template>
  </UiDrawerModal>
</template>

<script setup lang="ts">
export interface CreateSubmitPayload {
  label: string
  avatar: string | null
  avatarOptions: Record<string, unknown> | null
  /** Empty when user opted to reuse the vault password. */
  identityPassword: string
  useVaultPassword: boolean
  claims: {
    email: string
    name: string
    phone: string
    address: string
  }
}

const open = defineModel<boolean>('open', { required: true })

const props = defineProps<{
  submitting: boolean
  /** Whether a vault password is known — disables the reuse option when not. */
  vaultPasswordAvailable: boolean
}>()

const emit = defineEmits<{
  submit: [payload: CreateSubmitPayload]
}>()

const { t } = useI18n()

const form = reactive({
  label: '',
  avatar: null as string | null,
  avatarOptions: null as Record<string, unknown> | null,
  useVaultPassword: true,
  password: '',
  passwordConfirm: '',
  email: '',
  name: '',
  phone: '',
  address: '',
})

const isValidEmail = (email: string): boolean =>
  /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email)

const emailValidator = (value: unknown): string | null => {
  const v = String(value ?? '').trim()
  if (!v) return null
  return isValidEmail(v) ? null : t('invalidEmail')
}

const canCreate = computed(() => {
  if (!form.label.trim() || !isValidEmail(form.email)) return false
  if (form.useVaultPassword) return props.vaultPasswordAvailable
  return (
    form.password.length >= 8 && form.password === form.passwordConfirm
  )
})

watch(open, (isOpen) => {
  if (!isOpen) return
  form.label = ''
  form.avatar = null
  form.avatarOptions = null
  form.useVaultPassword = true
  form.password = ''
  form.passwordConfirm = ''
  form.email = ''
  form.name = ''
  form.phone = ''
  form.address = ''
})

const onSubmit = () => {
  if (!canCreate.value) return
  emit('submit', {
    label: form.label.trim(),
    avatar: form.avatar,
    avatarOptions: form.avatarOptions,
    identityPassword: form.useVaultPassword ? '' : form.password,
    useVaultPassword: form.useVaultPassword,
    claims: {
      email: form.email,
      name: form.name,
      phone: form.phone,
      address: form.address,
    },
  })
}
</script>

<i18n lang="yaml">
de:
  title: Identität erstellen
  description: Erstelle eine neue kryptographische Identität. Jede Identität hat ihren eigenen Schlüssel und kann unabhängig in verschiedenen Spaces genutzt werden.
  labelField: Name
  labelPlaceholder: z.B. Persönlich, Arbeit, Anonym
  syncCredentials: Sync-Zugangsdaten
  useVaultPassword: Gleiches Passwort wie Vault verwenden
  identityPassword: Identity-Passwort
  identityPasswordDescription: Dieses Passwort schützt deinen privaten Schlüssel auf dem Sync-Server. Merke es dir gut – es wird für die Wiederherstellung benötigt.
  identityPasswordConfirm: Identity-Passwort bestätigen
  passwordMismatch: Passwörter stimmen nicht überein
  invalidEmail: Bitte eine gültige E-Mail-Adresse eingeben
  claimsOptional: Weitere Angaben (optional)
  phone: Telefon
  address: Adresse
  submit: Erstellen
  cancel: Abbrechen
en:
  title: Create Identity
  description: Create a new cryptographic identity. Each identity has its own key and can be used independently across spaces.
  labelField: Name
  labelPlaceholder: e.g. Personal, Work, Anonymous
  syncCredentials: Sync credentials
  useVaultPassword: Use the same password as the vault
  identityPassword: Identity password
  identityPasswordDescription: This password protects your private key on the sync server. Remember it — it's required for recovery.
  identityPasswordConfirm: Confirm identity password
  passwordMismatch: Passwords do not match
  invalidEmail: Please enter a valid email address
  claimsOptional: Additional fields (optional)
  phone: Phone
  address: Address
  submit: Create
  cancel: Cancel
</i18n>
