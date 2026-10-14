<script setup lang="ts">
/**
 * An entry of the password manager as the credentials of a connection (spec 052 US1, FR-001,
 * FR-002, research R7): the entries that fit the protocol, tagged ones first, searchable by title,
 * user name and address. It shows no secret; only the id of the chosen entry goes back.
 */
import { invoke } from '@tauri-apps/api/core'
import type { CredentialsCandidate } from '@bindings/CredentialsCandidate'
import type { Protocol } from '@bindings/Protocol'

const props = defineProps<{ protocol: Protocol }>()
const entryId = defineModel<string>({ required: true })
const { t } = useI18n()
const fieldLabels = useFieldLabels()
const storageError = useStorageError()
const candidates = ref<CredentialsCandidate[] | null>(null)
const failure = ref<string | null>(null)
const search = ref('')

async function loadAsync() {
  candidates.value = null
  failure.value = null
  try {
    candidates.value = await invoke<CredentialsCandidate[]>(
      'storage_credentials_candidates',
      { protocol: props.protocol },
    )
  } catch (error) {
    failure.value = storageError(error)
    candidates.value = []
  }
  // An entry that does not fit the protocol (any more) is no choice.
  if (!candidates.value.some((candidate) => candidate.id === entryId.value))
    entryId.value = ''
}
watch(() => props.protocol, loadAsync, { immediate: true })

const shown = computed(() => {
  const query = search.value.trim().toLowerCase()
  const all = candidates.value ?? []
  if (!query) return all
  return all.filter((candidate) =>
    [candidate.title, candidate.username, candidate.url].some((text) =>
      text?.toLowerCase().includes(query),
    ),
  )
})

function describe(candidate: CredentialsCandidate) {
  return [candidate.username, candidate.url].filter(Boolean).join(' · ')
}
</script>

<template>
  <section class="flex flex-col gap-2" data-testid="storage-entry-picker">
    <h2 class="px-1 text-sm font-semibold">
      {{ t('settings.storage.entryPicker.label') }}
    </h2>
    <UiInput
      id="storage-entry-search"
      v-model="search"
      :label="t('settings.storage.entryPicker.search')"
      :labels="fieldLabels.input.value"
      type="search"
      autocomplete="off"
      data-testid="storage-entry-search"
    />
    <p
      v-if="candidates === null"
      class="px-1 text-sm text-muted-foreground"
      role="status"
    >
      {{ t('settings.storage.entryPicker.loading') }}
    </p>
    <p v-else-if="failure" class="px-1 text-sm text-destructive" role="alert">
      {{ failure }}
    </p>
    <p
      v-else-if="candidates.length === 0"
      class="px-1 text-sm text-muted-foreground"
      role="status"
      data-testid="storage-entry-empty"
    >
      {{ t('settings.storage.entryPicker.empty') }}
      {{ t('settings.storage.entryPicker.onlyFitting') }}
    </p>
    <p
      v-else-if="shown.length === 0"
      class="px-1 text-sm text-muted-foreground"
      role="status"
    >
      {{ t('settings.storage.entryPicker.noMatch') }}
    </p>
    <div v-else class="max-h-96 overflow-y-auto rounded-xl">
      <SettingsGroup v-model="entryId" radio>
        <SettingsOptionRow
          v-for="candidate in shown"
          :key="candidate.id"
          type="radio"
          :value="candidate.id"
          :title="candidate.title || candidate.username"
          :description="describe(candidate)"
          :data-testid="`storage-entry-option-${candidate.id}`"
        />
      </SettingsGroup>
    </div>
    <p v-if="candidates?.length" class="px-1 text-sm text-muted-foreground">
      {{ t('settings.storage.entryPicker.onlyFitting') }}
    </p>
  </section>
</template>
