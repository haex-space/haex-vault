<template>
  <UModal v-model:open="open" :title="isEdit ? t('editTitle') : t('createTitle')">
    <template #body>
      <form class="space-y-4" @submit.prevent="onSubmitAsync">
        <UFormField :label="t('field.name')" required>
          <UInput
            v-model="form.name"
            :placeholder="t('placeholder.name')"
            class="w-full"
            required
          />
        </UFormField>

        <UFormField :label="t('field.baseUrl')" required>
          <UInput
            v-model="form.baseUrl"
            placeholder="https://marketplace.example.com"
            class="w-full"
            required
          />
        </UFormField>

        <UFormField :label="t('field.authType')">
          <USelectMenu
            v-model="form.authType"
            :items="authTypeItems"
            value-key="value"
            class="w-full"
          />
        </UFormField>

        <UFormField v-if="form.authType === 'bearer'" :label="t('field.authToken')" required>
          <UInput
            v-model="form.authToken"
            type="password"
            class="w-full"
            required
          />
        </UFormField>

        <template v-if="form.authType === 'basic'">
          <UFormField :label="t('field.authUsername')" required>
            <UInput v-model="form.authUsername" class="w-full" required />
          </UFormField>
          <UFormField :label="t('field.authPassword')" required>
            <UInput v-model="form.authPassword" type="password" class="w-full" required />
          </UFormField>
        </template>

        <UFormField v-if="form.authType === 'did'" :label="t('field.authIdentity')" required>
          <USelectMenu
            v-model="selectedIdentityId"
            :items="identityItems"
            value-key="value"
            class="w-full"
          />
          <p class="text-xs text-muted mt-1">{{ t('hint.did') }}</p>
        </UFormField>

        <UFormField :label="t('field.sortOrder')" :hint="t('hint.sortOrder')">
          <UInput
            v-model.number="form.sortOrder"
            type="number"
            min="1"
            class="w-full"
          />
        </UFormField>

        <UFormField>
          <UCheckbox v-model="form.enabled" :label="t('field.enabled')" />
        </UFormField>
      </form>
    </template>

    <template #footer>
      <div class="flex justify-end gap-2 w-full">
        <UiButton variant="ghost" :label="t('cancel')" @click="open = false" />
        <UiButton
          color="primary"
          :label="isEdit ? t('save') : t('create')"
          :loading="isSaving"
          :disabled="!canSubmit"
          @click="onSubmitAsync"
        />
      </div>
    </template>
  </UModal>
</template>

<script setup lang="ts">
import type {
  MarketplaceAuthType,
  MarketplaceInput,
} from '@/composables/useMarketplaces'
import {
  createMarketplaceAsync,
  updateMarketplaceAsync,
} from '@/composables/useMarketplaces'
import type { SelectHaexMarketplaces } from '@/database/schemas/marketplaces'

const props = defineProps<{
  /** Existing row when editing; null/undefined when creating. */
  row?: SelectHaexMarketplaces | null
}>()

const emit = defineEmits<{
  saved: []
}>()

const open = defineModel<boolean>('open', { default: false })

const { t } = useI18n()
const { add } = useToast()
const identityStore = useIdentityStore()

const isEdit = computed(() => !!props.row)

interface FormState {
  name: string
  baseUrl: string
  enabled: boolean
  sortOrder: number
  authType: MarketplaceAuthType
  authToken: string
  authUsername: string
  authPassword: string
  authIdentityId: string | null
}

const blankForm = (): FormState => ({
  name: '',
  baseUrl: '',
  enabled: true,
  sortOrder: 100,
  authType: 'none',
  authToken: '',
  authUsername: '',
  authPassword: '',
  authIdentityId: null,
})

const form = reactive<FormState>(blankForm())
const isSaving = ref(false)

// Reset form whenever the dialog opens — for edit, prefill from the row.
watch(open, (isOpen) => {
  if (!isOpen) return
  const row = props.row
  if (row) {
    form.name = row.name
    form.baseUrl = row.baseUrl
    form.enabled = row.enabled
    form.sortOrder = row.sortOrder
    form.authType = row.authType as MarketplaceAuthType
    form.authToken = row.authToken ?? ''
    form.authUsername = row.authUsername ?? ''
    form.authPassword = row.authPassword ?? ''
    form.authIdentityId = row.authIdentityId
  } else {
    Object.assign(form, blankForm())
  }
})

const authTypeItems = computed(() => [
  { value: 'none', label: t('authType.none') },
  { value: 'bearer', label: t('authType.bearer') },
  { value: 'basic', label: t('authType.basic') },
  { value: 'did', label: t('authType.did') },
])

const identityItems = computed(() =>
  identityStore.identities
    .filter(i => !!i.privateKey)
    .map(i => ({ value: i.id, label: i.name || i.did })),
)

// USelectMenu uses string|undefined; form.authIdentityId is string|null. Bridge.
const selectedIdentityId = computed<string | undefined>({
  get: () => form.authIdentityId ?? undefined,
  set: (v) => { form.authIdentityId = v ?? null },
})

const canSubmit = computed(() => {
  if (!form.name.trim() || !form.baseUrl.trim()) return false
  if (form.authType === 'bearer' && !form.authToken) return false
  if (form.authType === 'basic' && (!form.authUsername || !form.authPassword)) return false
  if (form.authType === 'did' && !form.authIdentityId) return false
  return true
})

const buildPayload = (): MarketplaceInput => ({
  name: form.name.trim(),
  baseUrl: form.baseUrl.trim().replace(/\/$/, ''),
  enabled: form.enabled,
  sortOrder: form.sortOrder,
  authType: form.authType,
  authToken: form.authType === 'bearer' ? form.authToken : null,
  authUsername: form.authType === 'basic' ? form.authUsername : null,
  authPassword: form.authType === 'basic' ? form.authPassword : null,
  authIdentityId: form.authType === 'did' ? form.authIdentityId : null,
})

const onSubmitAsync = async () => {
  if (!canSubmit.value || isSaving.value) return
  isSaving.value = true
  try {
    const payload = buildPayload()
    if (props.row) {
      await updateMarketplaceAsync(props.row.id, payload)
    } else {
      await createMarketplaceAsync(payload)
    }
    add({ description: t(props.row ? 'success.updated' : 'success.created'), color: 'success' })
    emit('saved')
    open.value = false
  } catch (error) {
    add({ description: t('error.save', { msg: (error as Error).message }), color: 'error' })
  } finally {
    isSaving.value = false
  }
}
</script>

<i18n lang="yaml">
de:
  createTitle: Marketplace hinzufügen
  editTitle: Marketplace bearbeiten
  cancel: Abbrechen
  create: Erstellen
  save: Speichern
  field:
    name: Name
    baseUrl: Basis-URL
    authType: Authentifizierung
    authToken: Token
    authUsername: Benutzername
    authPassword: Passwort
    authIdentity: Identität
    sortOrder: Sortier-Priorität
    enabled: Aktiviert
  placeholder:
    name: z.B. Mein Marketplace
  hint:
    sortOrder: Niedrigere Werte erscheinen zuerst (Standard 100).
    did: Verwendet die signierte DID-Auth des Vaults — der private Schlüssel verlässt das Gerät nicht.
  authType:
    none: Keine
    bearer: Bearer Token
    basic: Benutzername / Passwort
    did: DID (zero-knowledge)
  success:
    created: Marketplace erstellt
    updated: Marketplace aktualisiert
  error:
    save: Speichern fehlgeschlagen — {msg}
en:
  createTitle: Add marketplace
  editTitle: Edit marketplace
  cancel: Cancel
  create: Create
  save: Save
  field:
    name: Name
    baseUrl: Base URL
    authType: Authentication
    authToken: Token
    authUsername: Username
    authPassword: Password
    authIdentity: Identity
    sortOrder: Sort priority
    enabled: Enabled
  placeholder:
    name: e.g. My Marketplace
  hint:
    sortOrder: Lower values appear first (default 100).
    did: Uses the vault's signed DID auth — the private key never leaves the device.
  authType:
    none: None
    bearer: Bearer token
    basic: Username / password
    did: DID (zero-knowledge)
  success:
    created: Marketplace created
    updated: Marketplace updated
  error:
    save: Save failed — {msg}
</i18n>
