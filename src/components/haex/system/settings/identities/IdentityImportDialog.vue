<template>
  <UiDrawerModal
    v-model:open="open"
    :title="t('title')"
    :description="t('description')"
  >
    <template #body>
      <!-- Step 1: Load data -->
      <template v-if="!parsed">
        <div class="space-y-4">
          <UButton
            color="neutral"
            variant="outline"
            icon="i-lucide-file-up"
            block
            @click="onSelectFileAsync"
          >
            {{ t('selectFile') }}
          </UButton>

          <USeparator :label="t('orPaste')" />

          <UiTextarea
            v-model="json"
            :label="t('jsonLabel')"
            :placeholder="t('jsonPlaceholder')"
            :rows="6"
          />
        </div>
      </template>

      <!-- Step 2: Preview & select -->
      <template v-else>
        <div class="space-y-4">
          <!-- Identity info -->
          <div
            class="flex items-center gap-3 p-3 rounded-lg border border-default"
          >
            <UiAvatar
              v-if="parsed.avatar"
              :src="parsed.avatar"
              :seed="parsed.did"
              avatar-style="toon-head"
              size="sm"
            />
            <div class="min-w-0 flex-1">
              <p class="font-medium truncate">
                {{ parsed.name || parsed.did.slice(0, 20) + '...' }}
              </p>
              <p class="text-xs text-muted truncate">{{ parsed.did }}</p>
            </div>
            <UBadge
              :color="parsed.privateKey ? 'primary' : 'neutral'"
              variant="subtle"
              size="sm"
            >
              {{ parsed.privateKey ? t('typeIdentity') : t('typeContact') }}
            </UBadge>
          </div>

          <!-- Avatar toggle -->
          <div
            v-if="parsed.avatar"
            class="flex items-center gap-3 p-2 rounded bg-gray-50 dark:bg-gray-800/50"
          >
            <UCheckbox v-model="includeAvatar" />
            <UiAvatar
              :src="parsed.avatar"
              :seed="parsed.did"
              avatar-style="toon-head"
              size="sm"
            />
            <span class="text-sm">{{ t('includeAvatar') }}</span>
          </div>

          <!-- Claims -->
          <div
            v-if="parsed.claims.length"
            class="space-y-2"
          >
            <span class="text-sm font-medium">{{ t('selectClaims') }}</span>
            <div
              v-for="(claim, index) in parsed.claims"
              :key="index"
              class="flex items-center gap-3 p-2 rounded bg-gray-50 dark:bg-gray-800/50"
            >
              <UCheckbox
                :model-value="selectedClaimIndices.has(index)"
                @update:model-value="toggleClaim(index)"
              />
              <div class="min-w-0 flex-1">
                <span class="text-xs font-medium text-muted">{{
                  claim.type
                }}</span>
                <p class="text-sm truncate">{{ claim.value }}</p>
              </div>
            </div>
          </div>
        </div>
      </template>
    </template>
    <template #footer>
      <div class="flex justify-between gap-4">
        <UButton
          color="neutral"
          variant="outline"
          @click="onBack"
        >
          {{ parsed ? t('back') : t('cancel') }}
        </UButton>
        <UiButton
          v-if="!parsed"
          icon="i-lucide-arrow-right"
          :disabled="!json.trim()"
          @click="onParse"
        >
          {{ t('preview') }}
        </UiButton>
        <UiButton
          v-else
          icon="i-lucide-import"
          :loading="submitting"
          @click="onSubmit"
        >
          {{ t('submit') }}
        </UiButton>
      </div>
    </template>
  </UiDrawerModal>
</template>

<script setup lang="ts">
import type { ParsedIdentityImport } from '@/composables/useIdentityImport'

export interface ImportSubmitPayload {
  parsed: ParsedIdentityImport
  selectedClaimIndices: Set<number>
  includeAvatar: boolean
}

const open = defineModel<boolean>('open', { required: true })

const props = defineProps<{
  submitting: boolean
}>()

const emit = defineEmits<{
  /** Step 1 → Step 2: parent parses the raw JSON and sets `parsed` prop. */
  parse: [rawJson: string]
  submit: [payload: ImportSubmitPayload]
  'select-file': []
}>()

// Two-way state owned by the parent: parsed preview + raw JSON text.
// The parent uses `parsed` to drive step 1↔step 2 transitions and writes
// `json` after a successful file pick.
const parsed = defineModel<ParsedIdentityImport | null>('parsed', {
  default: null,
})
const json = defineModel<string>('json', { default: '' })

const { t } = useI18n()

const selectedClaimIndices = ref(new Set<number>())
const includeAvatar = ref(true)

// When parsed result arrives, pre-select all claims and mirror the avatar.
watch(parsed, (value) => {
  if (!value) return
  selectedClaimIndices.value = new Set(value.claims.map((_, i) => i))
  includeAvatar.value = !!value.avatar
})

// Reset on close.
watch(open, (isOpen) => {
  if (isOpen) return
  json.value = ''
  parsed.value = null
  selectedClaimIndices.value = new Set()
  includeAvatar.value = true
})

const toggleClaim = (index: number) => {
  if (selectedClaimIndices.value.has(index)) {
    selectedClaimIndices.value.delete(index)
  } else {
    selectedClaimIndices.value.add(index)
  }
}

const onSelectFileAsync = () => {
  emit('select-file')
}

const onParse = () => {
  if (!json.value.trim()) return
  emit('parse', json.value)
}

const onBack = () => {
  if (parsed.value) {
    parsed.value = null
    return
  }
  open.value = false
}

const onSubmit = () => {
  if (!parsed.value) return
  emit('submit', {
    parsed: parsed.value,
    selectedClaimIndices: selectedClaimIndices.value,
    includeAvatar: includeAvatar.value,
  })
}

</script>

<i18n lang="yaml">
de:
  title: Identität importieren
  description: Importiere eine Identität oder einen Kontakt aus einer JSON-Datei. Enthält die Datei einen privaten Schlüssel, wird sie als Identität importiert — andernfalls als Kontakt.
  selectFile: JSON-Datei auswählen
  orPaste: oder einfügen
  jsonLabel: Identitäts-JSON
  jsonPlaceholder: Exportiertes Identitäts-JSON hier einfügen
  preview: Vorschau
  typeIdentity: Identität
  typeContact: Kontakt
  includeAvatar: Profilbild übernehmen
  selectClaims: Claims zum Importieren auswählen
  submit: Importieren
  cancel: Abbrechen
  back: Zurück
en:
  title: Import Identity
  description: Import an identity or contact from a JSON file. If the file contains a private key it's imported as an identity — otherwise as a contact.
  selectFile: Select JSON file
  orPaste: or paste
  jsonLabel: Identity JSON
  jsonPlaceholder: Paste exported identity JSON here
  preview: Preview
  typeIdentity: Identity
  typeContact: Contact
  includeAvatar: Include avatar
  selectClaims: Select claims to import
  submit: Import
  cancel: Cancel
  back: Back
</i18n>
