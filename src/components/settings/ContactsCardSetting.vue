<script setup lang="ts">
/**
 * What the own Visitenkarte carries (spec 051 FR-009): the person's own contact, whose name the
 * card shows, and which of its values go on it (none by default: only the name). Saved as soon as
 * it is chosen; a main device then makes a new card.
 */
import { toast } from 'vue-sonner'
import type { Contact } from '@bindings/Contact'
import type { ContactsOwnCard } from '@bindings/ContactsOwnCard'
import { displayValue, HOLZI_ADDRESS_KINDS } from '~/lib/contacts/kinds'

const { t } = useI18n()
const { errString } = useErrorString()
const store = useContactsStore()
const { openApp } = useWmTab()
const { getAsync } = useContacts()
const { kindName } = useContactLabels()
const { ownCardAsync, ownCardSetAsync } = useContactCards()

const card = ref<ContactsOwnCard | null>(null)
const own = ref<Contact | null>(null)

async function loadAsync() {
  try {
    card.value = await ownCardAsync()
    own.value = card.value.ownContact
      ? await getAsync(card.value.ownContact).catch(() => null)
      : null
  } catch (cause) {
    toast.error(errString(cause))
  }
}
onMounted(loadAsync)

const contactOptions = computed(() => [
  { value: '', label: t('settings.contacts.card.noOwnContact') },
  ...store.headers.map((header) => ({
    value: header.id,
    label: header.name || t('contacts.noName'),
  })),
])

/** The own contact's values a card can carry; holzi addresses are on the card anyway. */
const values = computed(() =>
  (own.value?.values ?? []).filter(
    (value) =>
      value.known &&
      !(HOLZI_ADDRESS_KINDS as readonly string[]).includes(value.kind),
  ),
)

async function saveAsync(ownContact: string | null, valueIds: string[]) {
  try {
    // action-exempt: the contact actions join the catalog with spec 051 US6 (T083); until then the
    // choice in the settings calls the command.
    card.value = await ownCardSetAsync(ownContact, valueIds)
    own.value = ownContact ? await getAsync(ownContact) : null
  } catch (cause) {
    toast.error(errString(cause))
  }
}

function chooseContact(id: string) {
  void saveAsync(id || null, [])
}

function toggleValue(id: string, on: boolean) {
  const chosen = new Set(card.value?.valueIds ?? [])
  if (on) chosen.add(id)
  else chosen.delete(id)
  void saveAsync(card.value?.ownContact ?? null, [...chosen])
}
</script>

<template>
  <SettingsGroup
    :label="t('settings.contacts.card.title')"
    data-testid="contacts-card-settings"
  >
    <SettingsRow
      :title="t('settings.contacts.card.ownContact')"
      :description="t('settings.contacts.card.ownContactHint')"
      label-for="contacts-card-own"
    >
      <SettingsSelect
        id="contacts-card-own"
        :model-value="card?.ownContact ?? ''"
        :options="contactOptions"
        data-testid="contacts-card-own"
        @update:model-value="chooseContact"
      />
    </SettingsRow>
    <SettingsRow
      v-for="value in values"
      :key="value.id"
      :title="displayValue(value.kind, value.value)"
      :description="kindName(value.kind)"
    >
      <ShadcnCheckbox
        :model-value="card?.valueIds.includes(value.id) ?? false"
        :aria-label="t('settings.contacts.card.onCard')"
        :data-testid="`contacts-card-value-${value.id}`"
        @update:model-value="toggleValue(value.id, $event === true)"
      />
    </SettingsRow>
    <SettingsRow
      :title="t('contacts.card.own')"
      navigates
      data-testid="contacts-card-show"
      @select="openApp('system.contacts', '/card')"
    />
  </SettingsGroup>
</template>
