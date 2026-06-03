<template>
  <UiDrawerModal
    :open="visible"
    :title="title"
    :description="description"
    :ui="{
      content: 'max-w-xl',
      overlay: 'backdrop-blur-sm',
    }"
    @update:open="onOpenChange"
  >
    <template #body>
      <!-- Step 1: names -->
      <div
        v-if="step === 1"
        class="space-y-4"
      >
        <div class="flex justify-center">
          <UiAvatar
            :seed="deviceStore.localDeviceId"
            size="xl"
          />
        </div>

        <UiInput
          v-model="userName"
          :label="t('nameLabel')"
          :placeholder="t('namePlaceholder')"
          data-testid="welcome-user-name"
          @keydown.enter.prevent="onProceed"
        />

        <UiInput
          v-model="deviceName"
          :label="t('deviceLabel')"
          data-testid="welcome-device-name"
          @keydown.enter.prevent="onProceed"
        />

        <!-- Reclaim: only when this vault already knows other device rows -->
        <div
          v-if="hasKnownDevices"
          class="pt-3 border-t border-default space-y-2"
        >
          <button
            class="text-sm text-primary hover:underline text-left"
            data-testid="welcome-reclaim-toggle"
            @click="reclaimExpanded = !reclaimExpanded"
          >
            {{ t('reclaim.toggle') }}
          </button>

          <div
            v-if="reclaimExpanded"
            class="space-y-2"
          >
            <p class="text-sm text-muted">
              {{ t('reclaim.intro') }}
            </p>
            <UiListContainer>
              <UiListItem
                v-for="device in knownDevices"
                :key="device.id"
                :data-testid="`welcome-reclaim-${device.id}`"
                :highlight="selectedReclaimId === device.id"
                class="cursor-pointer"
                @click="toggleReclaim(device.id)"
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
      </div>

      <!-- Step 2: tour offer -->
      <div
        v-else
        class="space-y-3"
      >
        <p class="text-sm text-muted">
          {{ t('tour.intro') }}
        </p>
        <p class="text-sm font-medium">
          {{ t('tour.stops') }}
        </p>
      </div>
    </template>

    <template #footer>
      <div class="flex justify-between gap-2">
        <template v-if="step === 1">
          <UiButton
            variant="ghost"
            color="neutral"
            data-testid="welcome-skip"
            @click="onSkipStep1"
          >
            {{ t('later') }}
          </UiButton>
          <UiButton
            :disabled="!canProceed"
            :loading="submitting"
            data-testid="welcome-next"
            @click="onProceed"
          >
            {{ t('next') }}
          </UiButton>
        </template>
        <template v-else>
          <UiButton
            variant="ghost"
            color="neutral"
            data-testid="welcome-tour-skip"
            @click="onSkipTour"
          >
            {{ t('tour.skip') }}
          </UiButton>
          <UiButton
            data-testid="welcome-tour-start"
            @click="onStartTour"
          >
            {{ t('tour.start') }}
          </UiButton>
        </template>
      </div>
    </template>
  </UiDrawerModal>
</template>

<script setup lang="ts">
const deviceStore = useDeviceStore()
const identityStore = useIdentityStore()
const spacesStore = useSpacesStore()
const publishingStore = useSpacePublishingStore()
const tourStore = useTourStore()
const { t } = useI18n()

const visible = ref(false)
const step = ref<1 | 2>(1)
const userName = ref('')
const deviceName = ref('')
const reclaimExpanded = ref(false)
const selectedReclaimId = ref<string | null>(null)
const submitting = ref(false)

const knownDevices = computed(() => deviceStore.pendingResolution?.knownDevices ?? [])
const hasKnownDevices = computed(() => knownDevices.value.length > 0)
const defaultIdentity = computed(() => identityStore.ownIdentities[0] ?? null)

const title = computed(() => (step.value === 1 ? t('step1.title') : t('step2.title')))
const description = computed(() =>
  step.value === 1 ? t('step1.description') : t('step2.description'),
)

const canProceed = computed(
  () =>
    !submitting.value
    && userName.value.trim().length > 0
    && deviceName.value.trim().length > 0,
)

// The auto-created default identity carries a locale placeholder name; treat it
// as "no name yet" so Step 1 starts empty instead of pre-filling "My Identity".
const isPlaceholderName = (name?: string | null) =>
  !name || name === 'My Identity' || name === 'Meine Identität'

const openDialog = () => {
  step.value = 1
  reclaimExpanded.value = false
  selectedReclaimId.value = null
  const current = defaultIdentity.value?.name
  userName.value = isPlaceholderName(current) ? '' : current ?? ''
  deviceName.value = deviceStore.hostname ?? ''
  visible.value = true
}

// Open on a fresh pending resolution. `immediate` covers the case where the
// resolution was already set before this component mounted (initVaultAsync runs
// resolveAsync before `isVaultReady` flips on in vault.vue).
watch(
  () => deviceStore.pendingResolution,
  (res) => {
    if (res) openDialog()
  },
  { immediate: true },
)

// hostname resolves asynchronously; backfill the device field if it arrives
// after the dialog opened and the user hasn't typed anything yet.
watch(
  () => deviceStore.hostname,
  (h) => {
    if (visible.value && step.value === 1 && !deviceName.value && h) {
      deviceName.value = h
    }
  },
)

const platformIcon = (platform: string) => {
  switch (platform) {
    case 'desktop':
      return 'i-lucide-monitor'
    case 'android':
    case 'ios':
      return 'i-lucide-smartphone'
    default:
      return 'i-lucide-cpu'
  }
}

const toggleReclaim = (id: string) => {
  selectedReclaimId.value = selectedReclaimId.value === id ? null : id
}

const maybeOpenPublishing = () => {
  if (spacesStore.foreignSpaces.length > 0) {
    publishingStore.openForNewDevice()
  }
}

const onProceed = async () => {
  if (!canProceed.value) return
  submitting.value = true
  try {
    const identity = defaultIdentity.value
    const name = userName.value.trim()
    if (identity && name && name !== identity.name) {
      await identityStore.updateNameAsync(identity.id, name)
    }

    const devName = deviceName.value.trim()
    if (selectedReclaimId.value) {
      await deviceStore.reclaimAsync(selectedReclaimId.value, devName)
    } else {
      await deviceStore.registerNewAsync(devName)
    }

    // registerNewAsync/reclaimAsync clear pendingResolution; `visible` is owned
    // locally so the dialog stays open for Step 2.
    step.value = 2
  } finally {
    submitting.value = false
  }
}

const onSkipStep1 = () => {
  deviceStore.skipResolution()
  visible.value = false
}

const onStartTour = async () => {
  visible.value = false
  await tourStore.start()
  maybeOpenPublishing()
}

const onSkipTour = () => {
  visible.value = false
  maybeOpenPublishing()
}

const onOpenChange = (open: boolean) => {
  if (open) return
  if (step.value === 1) onSkipStep1()
  else onSkipTour()
}
</script>

<i18n lang="yaml">
de:
  step1:
    title: Willkommen
    description: Richte dieses Gerät für diese Vault ein.
  step2:
    title: Kleine Tour gefällig?
    description: In wenigen Schritten zeigen wir dir die wichtigsten Funktionen.
  nameLabel: Dein Name
  namePlaceholder: z. B. Marcel
  deviceLabel: Gerätename
  later: Später
  next: Weiter
  reclaim:
    toggle: Ich hatte dieses Gerät schon einmal in dieser Vault
    intro: Wähle das Gerät aus, das du gerade benutzt. Der bestehende Eintrag bekommt einen frischen Schlüssel.
    warningTitle: Das alte Gerät könnte noch online sein.
    warningBody: Du überschreibst Schlüssel und EndpointId. Verbindungen vom alten Gerät brechen ab.
  tour:
    intro: Die Tour zeigt dir die wichtigsten Bereiche:
    stops: Launcher · Einstellungen · Erweiterungen · Spaces (Einladen & Teilen) · Sync
    start: Tour starten
    skip: Überspringen
en:
  step1:
    title: Welcome
    description: Set up this device for this vault.
  step2:
    title: A quick tour?
    description: A few steps to show you the most important features.
  nameLabel: Your name
  namePlaceholder: e.g. Marcel
  deviceLabel: Device name
  later: Later
  next: Continue
  reclaim:
    toggle: I have had this device in this vault before
    intro: Pick the device you are currently using. The existing row gets a fresh key.
    warningTitle: The old device might still be online.
    warningBody: You overwrite key and EndpointId. Existing connections from the old device will drop.
  tour:
    intro: The tour walks you through the key areas:
    stops: Launcher · Settings · Extensions · Spaces (invite & share) · Sync
    start: Start tour
    skip: Skip
</i18n>
