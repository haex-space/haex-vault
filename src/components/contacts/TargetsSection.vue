<script setup lang="ts">
/**
 * The phones a contact goes to (spec 051 FR-018b to FR-018d, FR-025): one row per phone of the
 * vault with the sync on (and per phone that is still a target), a switch for "goes to this
 * phone", the account and what did not reach it. Switching off asks whether the phone deletes the
 * contact or keeps it unsynced.
 */
import { toast } from 'vue-sonner'
import type { Contact } from '@bindings/Contact'

const props = defineProps<{
  contact: Contact
  /** The name of a device of the vault. */
  deviceName: (device: string) => string
}>()

const emit = defineEmits<{
  changed: []
}>()

const { t } = useI18n()
const { errString } = useErrorString()
const store = useContactsStore()
const nameAccount = useAccountLabel()
const { targetsSetAsync } = useContactTargets()

const removing = ref<string | null>(null)
const busy = ref<string | null>(null)

/** Every phone the contact can go to: those with the sync on, and those that still are a target. */
const rows = computed(() => {
  const devices = new Set([
    ...store.phones.map((phone) => phone.device),
    ...props.contact.targets.map((target) => target.device),
  ])
  return [...devices].map((device) => ({
    device,
    target: props.contact.targets.find((each) => each.device === device),
  }))
})

/** Issues of phones that are no target (any more) still show. */
const strayIssues = computed(() =>
  props.contact.issues.filter(
    (issue) =>
      !props.contact.targets.some((target) => target.device === issue.device),
  ),
)

function phoneName(device: string): string {
  return store.phoneName(device) ?? props.deviceName(device)
}

function issueOf(device: string): string | null {
  const issue = props.contact.issues.find((each) => each.device === device)
  return issue ? t(`settings.contacts.phone.issues.${issue.reason}`) : null
}

function toggle(device: string, on: boolean) {
  if (on) void addAsync(device)
  else removing.value = device
}

async function addAsync(device: string) {
  busy.value = device
  try {
    // action-exempt: the contact actions join the catalog with spec 051 US6 (T083); until then the
    // choice of the window calls the command.
    await targetsSetAsync([props.contact.id], device)
    await store.quietReloadAsync()
    emit('changed')
  } catch (cause) {
    toast.error(errString(cause))
  } finally {
    busy.value = null
  }
}
</script>

<template>
  <SettingsGroup
    v-if="rows.length || strayIssues.length"
    :label="t('contacts.targets.title')"
    data-testid="contacts-targets"
  >
    <SettingsRow
      v-for="{ device, target } in rows"
      :key="device"
      :title="phoneName(device)"
      :description="
        target
          ? [
              nameAccount(target.accountType, target.accountName),
              issueOf(device),
            ]
              .filter(Boolean)
              .join(' · ')
          : undefined
      "
      :label-for="`contacts-target-switch-${device}`"
      :data-testid="`contacts-target-${device}`"
    >
      <Icon
        v-if="issueOf(device)"
        name="lucide:circle-alert"
        class="size-4 text-destructive"
        data-testid="contacts-target-issue"
      />
      <ShadcnSwitch
        :id="`contacts-target-switch-${device}`"
        :model-value="Boolean(target)"
        :disabled="busy === device"
        :data-testid="`contacts-target-switch-${device}`"
        @update:model-value="toggle(device, $event === true)"
      />
    </SettingsRow>
    <SettingsRow
      v-for="issue in strayIssues"
      :key="`issue-${issue.device}`"
      :title="phoneName(issue.device)"
      :description="t(`settings.contacts.phone.issues.${issue.reason}`)"
    >
      <Icon name="lucide:circle-alert" class="size-4 text-destructive" />
    </SettingsRow>
  </SettingsGroup>
  <ContactsTargetRemoveDialog
    v-if="removing"
    :open="removing !== null"
    :ids="[contact.id]"
    :device="removing"
    :phone="phoneName(removing)"
    @update:open="(value: boolean) => (removing = value ? removing : null)"
    @done="emit('changed')"
  />
</template>
