<script setup lang="ts">
/**
 * The open requests of copies of the vault file to join (spec 024, user story 7, FR-045), on a
 * main device: each with the name it announced and the buttons "Aufnehmen" and "Ablehnen". Nothing
 * admits a copy without this decision. Hidden while there is no request. The list comes from the
 * device store, so it follows `sync-devices-changed`.
 */
import { toast } from 'vue-sonner'
import { elapsedSince } from '~/lib/sync/deviceStatus'

const { t } = useI18n()
const { errString } = useErrorString()
const store = useSyncDevicesStore()
const decide = useActionOrThrow('settings.devices.admit')

const busy = ref<string | null>(null)

const requests = computed(() => store.status?.openAdmissions ?? [])

// The ages are read when the list changes; a request does not need a ticking clock.
const now = computed(() => (requests.value, Date.now()))

/** "angefragt vor 3 Min." for a request of `requestedAt` ms. */
function asked(requestedAt: number): string {
  const elapsed = elapsedSince(requestedAt, now.value)
  return elapsed.unit === 'justNow'
    ? t('settings.admission.asked.justNow')
    : t(`settings.admission.asked.${elapsed.unit}`, { n: elapsed.value })
}

/** Answers one request, ignoring a second click while an answer is pending. */
async function onDecide(devicePubkey: string, admit: boolean) {
  if (busy.value) return
  busy.value = devicePubkey
  try {
    await decide({ devicePubkey, admit })
    await store.loadAsync()
  } catch (e) {
    const kind = (e as { kind?: string })?.kind
    toast.error(
      kind === 'NotMainDevice'
        ? t('settings.admission.notMain')
        : `${t('settings.admission.failed')} ${errString(e)}`,
    )
  } finally {
    busy.value = null
  }
}
</script>

<template>
  <section
    v-if="requests.length > 0"
    class="flex flex-col gap-2"
    data-testid="admission-requests"
  >
    <SettingsGroup :label="t('settings.admission.title')">
      <SettingsRow
        v-for="request in requests"
        :key="request.devicePubkey"
        :title="request.name"
        icon="lucide:monitor-down"
        data-testid="admission-request"
      >
        <template #description>
          {{ asked(request.requestedAt) }}
        </template>
        <UiButton
          variant="outline"
          :disabled="busy !== null"
          :aria-label="
            t('settings.admission.rejectLabel', { name: request.name })
          "
          data-testid="admission-reject"
          @click="onDecide(request.devicePubkey, false)"
        >
          {{ t('settings.admission.reject') }}
        </UiButton>
        <UiButton
          :loading="busy === request.devicePubkey"
          :disabled="busy !== null"
          :aria-label="
            t('settings.admission.admitLabel', { name: request.name })
          "
          data-testid="admission-admit"
          @click="onDecide(request.devicePubkey, true)"
        >
          {{ t('settings.admission.admit') }}
        </UiButton>
      </SettingsRow>
    </SettingsGroup>
    <p class="px-1 text-sm text-muted-foreground">
      {{ t('settings.admission.description') }}
    </p>
  </section>
</template>
