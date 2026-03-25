<template>
  <HaexSystemSettingsLayout
    :title="t('title')"
    :description="t('description')"
    show-back
    @back="$emit('back')"
  >
    <template #actions>
      <UiButton
        icon="i-lucide-save"
        color="primary"
        :disabled="!hasChanges"
        @click="onSaveAsync"
      >
        {{ t('save') }}
      </UiButton>
    </template>

    <div>
      <UFormField :label="t('urlLabel')">
        <div class="flex items-center gap-2 max-w-lg">
          <USelectMenu
            v-model="relayUrlInput"
            :items="relayOptions"
            :placeholder="t('urlPlaceholder')"
            class="font-mono text-sm flex-1"
          />
          <UiButton
            icon="i-lucide-plus"
            color="neutral"
            variant="outline"
            @click="showAddDialog = true"
          />
        </div>
        <template #description>
          <span class="text-xs text-muted">{{ t('urlHint') }}</span>
        </template>
      </UFormField>
    </div>

    <!-- Add Relay Dialog -->
    <UiDrawerModal
      v-model:open="showAddDialog"
      :title="t('add.title')"
      :description="t('add.description')"
    >
      <template #content>
        <UiInput
          v-model="newRelayUrl"
          :label="t('add.urlLabel')"
          :placeholder="t('add.urlPlaceholder')"
          class="font-mono text-sm"
          @keydown.enter.prevent="onAddRelay"
        />
      </template>
      <template #footer>
        <div class="flex justify-between gap-4">
          <UButton
            color="neutral"
            variant="outline"
            @click="showAddDialog = false"
          >
            {{ t('add.cancel') }}
          </UButton>
          <UiButton
            icon="i-lucide-plus"
            :disabled="!newRelayUrl.trim()"
            @click="onAddRelay"
          >
            {{ t('add.confirm') }}
          </UiButton>
        </div>
      </template>
    </UiDrawerModal>
  </HaexSystemSettingsLayout>
</template>

<script setup lang="ts">
defineEmits<{ back: [] }>()

const { t } = useI18n()
const { add } = useToast()
const store = usePeerStorageStore()

const defaultRelay = 'relay.sync.haex.space'
const irohRelay = 'https://relay.iroh.network'

const customRelays = ref<string[]>([])
const relayOptions = computed(() => [defaultRelay, irohRelay, ...customRelays.value])

const relayUrlInput = ref('')
const initialValue = ref('')

const showAddDialog = ref(false)
const newRelayUrl = ref('')

const hasChanges = computed(() => relayUrlInput.value.trim() !== initialValue.value)

onMounted(async () => {
  await store.loadConfiguredRelayUrlAsync()
  relayUrlInput.value = store.configuredRelayUrl || defaultRelay
  initialValue.value = relayUrlInput.value

  // If configured relay is not the default, add it to custom list
  if (store.configuredRelayUrl && store.configuredRelayUrl !== defaultRelay) {
    customRelays.value.push(store.configuredRelayUrl)
  }
})

const onAddRelay = () => {
  const url = newRelayUrl.value.trim()
  if (!url) return
  if (!relayOptions.value.includes(url)) {
    customRelays.value.push(url)
  }
  relayUrlInput.value = url
  newRelayUrl.value = ''
  showAddDialog.value = false
}

const onSaveAsync = async () => {
  if (!hasChanges.value) return
  try {
    await store.saveConfiguredRelayUrlAsync(relayUrlInput.value.trim() || null)
    initialValue.value = relayUrlInput.value.trim()
    add({ title: t('saved'), color: 'success' })
  } catch {
    add({ title: t('error'), color: 'error' })
  }
}
</script>

<i18n lang="yaml">
de:
  title: Relay-Server
  description: Relay-Server für P2P-Verbindungen durch NAT konfigurieren
  urlLabel: Relay-URL
  urlHint: Wähle einen Relay aus der Liste. Leer lassen für den Standard-Relay.
  urlPlaceholder: Relay auswählen...
  save: Speichern
  saved: Relay-URL gespeichert
  error: Fehler
  add:
    title: Relay hinzufügen
    description: Füge einen eigenen Relay-Server zur Auswahl hinzu
    urlLabel: Relay-URL
    urlPlaceholder: "https://my-relay.example.com"
    cancel: Abbrechen
    confirm: Hinzufügen
en:
  title: Relay Server
  description: Configure the relay server for P2P connections through NAT
  urlLabel: Relay URL
  urlHint: Choose a relay from the list. Leave empty to use the default relay.
  urlPlaceholder: Select relay...
  save: Save
  saved: Relay URL saved
  error: Error
  add:
    title: Add Relay
    description: Add a custom relay server to the selection
    urlLabel: Relay URL
    urlPlaceholder: "https://my-relay.example.com"
    cancel: Cancel
    confirm: Add
</i18n>
