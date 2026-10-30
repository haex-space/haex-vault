<script setup lang="ts">
/**
 * An entry of the password manager as the credentials of a connection or a marketplace (spec 052
 * US1, FR-001, FR-002, research R7; spec 069 US5): the entries a command lists as fitting, tagged
 * ones first, searchable by title, user name and address. It shows no secret; only the id of the
 * chosen entry goes back. `source` is that command and its arguments (the storage connection asks
 * `storage_credentials_candidates` for its protocol, a marketplace for its auth type); `labelKey`
 * names the heading and `hintKey` what the list is limited to.
 */
import { invoke } from '@tauri-apps/api/core'
import type { CredentialsCandidate } from '@bindings/CredentialsCandidate'

const props = withDefaults(
  defineProps<{
    source: { command: string; args: Record<string, unknown> }
    labelKey?: string
    hintKey?: string
  }>(),
  {
    labelKey: 'settings.storage.entryPicker.label',
    hintKey: 'settings.storage.entryPicker.onlyFitting',
  },
)
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
      props.source.command,
      props.source.args,
    )
  } catch (error) {
    failure.value = storageError(error)
    candidates.value = []
  }
  // An entry that does not fit (any more) is no choice.
  if (!candidates.value.some((candidate) => candidate.id === entryId.value))
    entryId.value = ''
}
watch(() => JSON.stringify(props.source), loadAsync, { immediate: true })

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
      {{ t(labelKey) }}
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
      {{ t(hintKey) }}
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
      {{ t(hintKey) }}
    </p>
  </section>
</template>
