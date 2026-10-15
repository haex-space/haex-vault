<script setup lang="ts">
/**
 * The phones a contact goes to (spec 051 FR-018b to FR-018d, FR-025): each with its account and
 * what did not reach it, a way to add another phone of the vault with the sync on, and removing
 * one with the question whether the phone deletes the contact or keeps it unsynced.
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

const addable = computed(() =>
  store.phones.filter(
    (phone) =>
      !props.contact.targets.some((target) => target.device === phone.device),
  ),
)

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

async function addAsync(device: string) {
  try {
    // action-exempt: the contact actions join the catalog with spec 051 US6 (T083); until then the
    // choice of the window calls the command.
    await targetsSetAsync([props.contact.id], device)
    await store.quietReloadAsync()
    emit('changed')
  } catch (cause) {
    toast.error(errString(cause))
  }
}
</script>

<template>
  <SettingsGroup
    v-if="contact.targets.length || contact.issues.length || addable.length"
    :label="t('contacts.targets.title')"
    data-testid="contacts-targets"
  >
    <SettingsRow
      v-for="target in contact.targets"
      :key="target.device"
      :title="phoneName(target.device)"
      :description="
        [
          nameAccount(target.accountType, target.accountName),
          issueOf(target.device),
        ]
          .filter(Boolean)
          .join(' · ')
      "
      :data-testid="`contacts-target-${target.device}`"
    >
      <Icon
        v-if="issueOf(target.device)"
        name="lucide:circle-alert"
        class="size-4 text-destructive"
        data-testid="contacts-target-issue"
      />
      <UiButton
        variant="ghost"
        size="icon"
        :aria-label="
          t('contacts.targets.remove', { phone: phoneName(target.device) })
        "
        :tooltip="
          t('contacts.targets.remove', { phone: phoneName(target.device) })
        "
        :data-testid="`contacts-target-remove-${target.device}`"
        @click="removing = target.device"
      >
        <Icon name="lucide:x" class="size-4" />
      </UiButton>
    </SettingsRow>
    <SettingsRow
      v-for="issue in strayIssues"
      :key="`issue-${issue.device}`"
      :title="phoneName(issue.device)"
      :description="t(`settings.contacts.phone.issues.${issue.reason}`)"
    >
      <Icon name="lucide:circle-alert" class="size-4 text-destructive" />
    </SettingsRow>
    <li v-if="addable.length" class="px-4 py-3">
      <ShadcnDropdownMenu>
        <ShadcnDropdownMenuTrigger as-child>
          <UiButton
            type="button"
            variant="outline"
            data-testid="contacts-target-add"
          >
            <Icon name="lucide:smartphone" class="size-4" />
            {{ t('contacts.targets.add') }}
          </UiButton>
        </ShadcnDropdownMenuTrigger>
        <ShadcnDropdownMenuContent align="start">
          <ShadcnDropdownMenuItem
            v-for="phone in addable"
            :key="phone.device"
            class="gap-2"
            :data-testid="`contacts-target-add-${phone.device}`"
            @select="addAsync(phone.device)"
          >
            <Icon name="lucide:smartphone" class="size-4" />
            {{ phone.name ?? t('contacts.targets.unnamedPhone') }}
          </ShadcnDropdownMenuItem>
        </ShadcnDropdownMenuContent>
      </ShadcnDropdownMenu>
    </li>
    <li
      v-else-if="contact.targets.length === 0"
      class="px-4 py-3 text-sm text-muted-foreground"
    >
      {{ t('contacts.targets.onlyHolzi') }}
    </li>
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
