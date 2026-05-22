<template>
  <UiDrawerModal
    :open="isOpen"
    :title="t('title')"
    :description="t('description')"
    :ui="{
      content: 'max-w-2xl',
      overlay: 'backdrop-blur-sm',
    }"
    @update:open="(v: boolean) => !v && onSkip()"
  >
    <template #body>
      <div class="space-y-4">
        <!-- Mode tabs -->
        <div class="grid grid-cols-2 gap-2">
          <button
            class="flex flex-col items-start gap-1 p-3 rounded-lg border transition-colors text-left"
            :class="
              mode === 'new'
                ? 'border-primary bg-primary/5 text-primary'
                : 'border-default hover:border-primary/50'
            "
            data-testid="reconciliation-mode-new"
            @click="mode = 'new'"
          >
            <UIcon
              name="i-lucide-sparkles"
              class="w-5 h-5"
            />
            <span class="text-sm font-medium">{{ t('mode.new.title') }}</span>
            <span class="text-xs text-muted">{{ t('mode.new.hint') }}</span>
          </button>
          <button
            :disabled="!hasKnownDevices"
            class="flex flex-col items-start gap-1 p-3 rounded-lg border transition-colors text-left"
            :class="[
              mode === 'reclaim'
                ? 'border-primary bg-primary/5 text-primary'
                : 'border-default hover:border-primary/50',
              !hasKnownDevices && 'opacity-40 cursor-not-allowed hover:border-default',
            ]"
            data-testid="reconciliation-mode-reclaim"
            @click="hasKnownDevices && (mode = 'reclaim')"
          >
            <UIcon
              name="i-lucide-history"
              class="w-5 h-5"
            />
            <span class="text-sm font-medium">{{ t('mode.reclaim.title') }}</span>
            <span class="text-xs text-muted">{{ t('mode.reclaim.hint') }}</span>
          </button>
        </div>

        <!-- New device form -->
        <div
          v-if="mode === 'new'"
          class="space-y-3"
        >
          <UiInput
            v-model="newName"
            :label="t('newDevice.nameLabel')"
            :placeholder="newNamePlaceholder"
            data-testid="reconciliation-new-name"
            @keydown.enter.prevent="onSubmitNew"
          />
          <p class="text-xs text-muted">
            {{ t('newDevice.platformHint', { platform: platformLabel }) }}
          </p>
        </div>

        <!-- Reclaim list -->
        <div
          v-else-if="mode === 'reclaim'"
          class="space-y-2"
        >
          <p class="text-sm text-muted">
            {{ t('reclaim.intro') }}
          </p>
          <UiListContainer>
            <UiListItem
              v-for="device in knownDevices"
              :key="device.id"
              :data-testid="`reconciliation-reclaim-${device.id}`"
              :highlight="selectedReclaimId === device.id"
              class="cursor-pointer"
              @click="selectedReclaimId = device.id"
            >
              <div class="flex items-center gap-3">
                <UiAvatar
                  :seed="device.endpointId"
                  size="md"
                />
                <div class="min-w-0">
                  <div class="text-sm font-medium truncate">
                    {{ device.name }}
                  </div>
                  <div class="flex items-center gap-2 text-xs text-muted">
                    <UIcon
                      :name="platformIcon(device.platform)"
                      class="w-3.5 h-3.5"
                    />
                    <span>{{ device.platform }}</span>
                    <span class="font-mono">{{ device.endpointId.slice(0, 12) }}…</span>
                  </div>
                </div>
              </div>
              <template
                v-if="selectedReclaimId === device.id"
                #actions
              >
                <UIcon
                  name="i-lucide-check-circle-2"
                  class="w-5 h-5 text-primary"
                />
              </template>
            </UiListItem>
          </UiListContainer>

          <UAlert
            v-if="selectedReclaimId"
            color="warning"
            variant="subtle"
            :title="t('reclaim.warningTitle')"
            :description="t('reclaim.warningBody')"
            icon="i-lucide-alert-triangle"
          />
        </div>
      </div>
    </template>

    <template #footer>
      <div class="flex justify-between gap-2">
        <UiButton
          variant="ghost"
          color="neutral"
          data-testid="reconciliation-skip"
          @click="onSkip"
        >
          {{ t('skip') }}
        </UiButton>
        <UiButton
          :disabled="!canSubmit"
          :loading="submitting"
          data-testid="reconciliation-submit"
          @click="onSubmit"
        >
          {{ submitLabel }}
        </UiButton>
      </div>
    </template>
  </UiDrawerModal>
</template>

<script setup lang="ts">
const deviceStore = useDeviceStore()
const publishingStore = useSpacePublishingStore()
const { t } = useI18n()

const mode = ref<'new' | 'reclaim'>('new')
const newName = ref('')
const selectedReclaimId = ref<string | null>(null)
const submitting = ref(false)

const isOpen = computed(() => deviceStore.pendingResolution !== null)
const knownDevices = computed(() => deviceStore.pendingResolution?.knownDevices ?? [])
const hasKnownDevices = computed(() => knownDevices.value.length > 0)

const platformLabel = computed(() => deviceStore.platform)
const newNamePlaceholder = computed(
  () => deviceStore.hostname || `${platformLabel.value} device`,
)

const canSubmit = computed(() => {
  if (submitting.value) return false
  if (mode.value === 'new') return newName.value.trim().length > 0
  return selectedReclaimId.value !== null
})

const submitLabel = computed(() =>
  mode.value === 'new' ? t('newDevice.submit') : t('reclaim.submit'),
)

// Reset the form whenever a new pending resolution arrives so values from a
// previous vault don't leak in.
watch(isOpen, (open) => {
  if (open) {
    mode.value = hasKnownDevices.value ? 'reclaim' : 'new'
    newName.value = newNamePlaceholder.value
    selectedReclaimId.value = null
  }
})

const platformIcon = (platform: string) => {
  switch (platform) {
    case 'desktop':
      return 'i-lucide-monitor'
    case 'android':
      return 'i-lucide-smartphone'
    case 'ios':
      return 'i-lucide-smartphone'
    default:
      return 'i-lucide-cpu'
  }
}

const onSubmit = async () => {
  if (!canSubmit.value) return
  submitting.value = true
  try {
    if (mode.value === 'new') {
      await deviceStore.registerNewAsync(newName.value.trim())
    } else if (selectedReclaimId.value) {
      const device = knownDevices.value.find(d => d.id === selectedReclaimId.value)
      await deviceStore.reclaimAsync(selectedReclaimId.value, device?.name)
    }
    publishingStore.openForNewDevice()
  } finally {
    submitting.value = false
  }
}

const onSubmitNew = () => {
  if (mode.value === 'new') void onSubmit()
}

const onSkip = () => {
  // Skip leaves `pendingResolution` cleared so the dialog stays closed for the
  // session. P2P does not start (no device row), and spaces will not appear
  // as published — the Geräte & Spaces settings page can revisit the choice.
  deviceStore.skipResolution()
}
</script>

<i18n lang="yaml">
de:
  title: Gerät erkennen
  description: Diese Vault hat noch keinen Eintrag für dieses Gerät. Bitte sage uns, welches Gerät du bist.
  skip: Später entscheiden
  mode:
    new:
      title: Neues Gerät
      hint: Dieses Gerät war noch nie in dieser Vault.
    reclaim:
      title: Bekanntes Gerät übernehmen
      hint: Eines deiner alten Geräte hier — z. B. nach App-Daten-Verlust.
  newDevice:
    nameLabel: Gerätename
    submit: Gerät registrieren
    platformHint: Plattform wird automatisch als „{platform}" gesetzt.
  reclaim:
    intro: Wähle das Gerät aus, das du gerade benutzt. Der bestehende Eintrag bekommt einen frischen Schlüssel.
    submit: Gerät übernehmen
    warningTitle: Das alte Gerät könnte noch online sein.
    warningBody: Du überschreibst Schlüssel und EndpointId. Verbindungen vom alten Gerät brechen ab.
en:
  title: Recognise device
  description: This vault has no entry for this device yet. Tell us which device you are.
  skip: Decide later
  mode:
    new:
      title: New device
      hint: This device has never been in this vault.
    reclaim:
      title: Reclaim known device
      hint: One of your old devices — e.g. after an app-data wipe.
  newDevice:
    nameLabel: Device name
    submit: Register device
    platformHint: Platform will be set automatically to "{platform}".
  reclaim:
    intro: Pick the device you are currently using. The existing row gets a fresh key.
    submit: Reclaim device
    warningTitle: The old device might still be online.
    warningBody: You overwrite key and EndpointId. Existing connections from the old device will drop.
</i18n>
