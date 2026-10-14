<script setup lang="ts">
/**
 * "Mit Telefon-Adressbuch abgleichen" (spec 051 FR-015, FR-015a, FR-017, FR-018, FR-025, FR-027): the
 * switch of this device, saved as soon as it flips; the target account; the state of the last run
 * and the changes that did not reach the phone. Switching on asks for the permission and opens the
 * overview of the first run. A device without a phone address book shows the settings switched off
 * and says so.
 */
import type { ContactsPhoneAccount } from '@bindings/ContactsPhoneAccount'
import { toast } from 'vue-sonner'
import { accountKey } from '~/composables/useContactsPhone'

const { t } = useI18n()
const { errString } = useErrorString()
const { capabilities } = useDeviceCapabilities()
const store = useContactsStore()
const phone = useContactsPhone()
phone.follow()

const available = computed(() => capabilities.value?.phoneContacts === true)
const status = phone.status
const on = computed(() => (status.value?.state ?? 'off') !== 'off')
const accounts = shallowRef<ContactsPhoneAccount[]>([])
const busy = ref(false)
const denied = ref(false)
const otherVault = ref(false)
const reviewOpen = ref(false)
const massDeleteOpen = ref(false)
const error = ref<string | null>(null)

async function loadAsync() {
  if (!available.value) return
  try {
    await phone.statusAsync()
    if (on.value) accounts.value = await phone.accountsAsync()
    error.value = null
  } catch (cause) {
    error.value = errString(cause)
  }
}

watch(available, (now) => now && void loadAsync(), { immediate: true })
watch(
  () => phone.massDelete.value,
  (held) => {
    if (held) massDeleteOpen.value = true
  },
)

async function toggleAsync(next: boolean) {
  if (busy.value) return
  busy.value = true
  denied.value = false
  error.value = null
  try {
    if (next) {
      // action-exempt: the switch of this device's phone sync is the person's own choice with a
      // system permission; no agent gets an action for it (spec 051 US6 lists none).
      const result = await phone.enableAsync()
      otherVault.value = result.otherVault
      await loadAsync()
      if (result.review) reviewOpen.value = true
    } else {
      // action-exempt: as above.
      await phone.disableAsync()
      otherVault.value = false
      await loadAsync()
    }
  } catch (cause) {
    if ((cause as { kind?: unknown } | null)?.kind === 'ContactsDenied')
      denied.value = true
    else error.value = errString(cause)
  } finally {
    busy.value = false
  }
}

const target = computed(() => {
  const chosen = status.value?.target
  if (chosen) return accountKey(chosen)
  const suggested =
    accounts.value.find((account) => account.writable && account.isDefault) ??
    accounts.value.find((account) => account.writable && account.accountType) ??
    accounts.value.find((account) => account.writable)
  return suggested ? accountKey(suggested) : ''
})

function accountLabel(account: ContactsPhoneAccount): string {
  if (!account.accountType) return t('settings.contacts.phone.localAccount')
  return `${account.name} (${account.accountType})`
}

const accountOptions = computed(() =>
  accounts.value
    .filter((account) => account.writable)
    .map((account) => ({
      value: accountKey(account),
      label: accountLabel(account),
    })),
)

async function chooseAsync(key: string) {
  const account = accounts.value.find((each) => accountKey(each) === key)
  if (!account) return
  try {
    // action-exempt: as the switch above.
    await phone.setTargetAsync(account)
    await phone.statusAsync()
  } catch (cause) {
    error.value = errString(cause)
  }
}

async function runNowAsync() {
  busy.value = true
  try {
    // action-exempt: as the switch above.
    await phone.runAsync()
    await store.quietReloadAsync()
  } catch (cause) {
    toast.error(errString(cause))
  } finally {
    busy.value = false
  }
}

const stateText = computed(() => {
  const now = status.value
  if (!now) return ''
  if (now.state === 'review') return t('settings.contacts.phone.state.review')
  if (now.paused === 'permission')
    return t('settings.contacts.phone.state.permission')
  if (now.paused === 'mass_delete')
    return t('settings.contacts.phone.state.massDelete')
  if (!now.lastRunAt) return t('settings.contacts.phone.state.never')
  const counts = now.counts
  return t('settings.contacts.phone.state.last', {
    at: new Date(now.lastRunAt).toLocaleString(),
    imported: counts?.imported ?? 0,
    written: counts?.written ?? 0,
    deleted: counts?.deleted ?? 0,
  })
})

function issueName(contactId: string): string {
  return (
    store.headersById.get(contactId)?.name ??
    t('settings.contacts.phone.issues.unknown')
  )
}

onMounted(() => {
  if (!store.hasLoadedOnce) void store.reloadAsync()
})
</script>

<template>
  <section class="flex flex-col gap-2" data-testid="settings-contacts-phone">
    <SettingsNotOnThisDevice v-if="capabilities && !available" />
    <SettingsGroup>
      <SettingsRow
        :title="t('settings.contacts.phone.title')"
        :description="t('settings.contacts.phone.description')"
        label-for="contacts-phone-switch"
      >
        <ShadcnSwitch
          id="contacts-phone-switch"
          :model-value="on"
          :disabled="!available || busy"
          data-testid="contacts-phone-switch"
          @update:model-value="toggleAsync($event === true)"
        />
      </SettingsRow>
      <SettingsRow
        v-if="on"
        :title="t('settings.contacts.phone.target')"
        :description="t('settings.contacts.phone.targetHint')"
        label-for="contacts-phone-target"
      >
        <SettingsSelect
          id="contacts-phone-target"
          :model-value="target"
          :options="accountOptions"
          data-testid="contacts-phone-target"
          @update:model-value="chooseAsync"
        />
      </SettingsRow>
      <SettingsRow
        v-if="on"
        :title="t('settings.contacts.phone.stateTitle')"
        :description="stateText"
        data-testid="contacts-phone-state"
      >
        <UiButton
          v-if="status?.state === 'review'"
          size="sm"
          variant="outline"
          data-testid="contacts-phone-review-open"
          @click="reviewOpen = true"
        >
          {{ t('settings.contacts.phone.reviewOpen') }}
        </UiButton>
        <UiButton
          v-else-if="status?.paused === 'mass_delete'"
          size="sm"
          variant="outline"
          data-testid="contacts-phone-mass-delete-open"
          @click="massDeleteOpen = true"
        >
          {{ t('settings.contacts.phone.decide') }}
        </UiButton>
        <UiButton
          v-else
          size="sm"
          variant="outline"
          :loading="busy"
          data-testid="contacts-phone-run"
          @click="runNowAsync"
        >
          {{ t('settings.contacts.phone.runNow') }}
        </UiButton>
      </SettingsRow>
    </SettingsGroup>
    <p class="px-1 text-xs text-muted-foreground">
      {{ t('settings.deviceOnly') }}
    </p>

    <p
      v-if="denied || status?.paused === 'permission'"
      class="px-1 text-sm text-destructive"
      role="alert"
      data-testid="contacts-phone-denied"
    >
      {{ t('contacts.error.contactsDenied') }}
    </p>
    <p
      v-if="otherVault"
      class="px-1 text-sm text-muted-foreground"
      role="status"
      data-testid="contacts-phone-other-vault"
    >
      {{ t('settings.contacts.phone.otherVault') }}
    </p>
    <p v-if="error" class="px-1 text-sm text-destructive" role="alert">
      {{ error }}
    </p>

    <SettingsGroup
      v-if="on && status && status.issues.length > 0"
      :title="t('settings.contacts.phone.issues.title')"
      data-testid="contacts-phone-issues"
    >
      <SettingsRow
        v-for="issue in status.issues"
        :key="issue.contactId"
        :title="issueName(issue.contactId)"
        :description="t(`settings.contacts.phone.issues.${issue.reason}`)"
      />
    </SettingsGroup>

    <ContactsPhoneReviewDialog v-model:open="reviewOpen" @done="loadAsync" />
    <ContactsMassDeleteDialog
      v-model:open="massDeleteOpen"
      :held="phone.massDelete.value"
      @done="loadAsync"
    />
  </section>
</template>
