<script setup lang="ts">
/**
 * One contact (spec 051, FR-002, FR-003, FR-006, FR-007, FR-018a): photo or initials, name and
 * organization, every value grouped by kind with copy and open, the holzi addresses (confirmed ones
 * from the contact's Visitenkarte, FR-008), the note, the
 * groups, the phones it goes to and where the contact came from. Values of a kind this version does not know show as they
 * are, read-only. Reloads quietly when the contact changes elsewhere.
 */
import { toast } from 'vue-sonner'
import type { Contact } from '@bindings/Contact'
import type { ContactValue } from '@bindings/ContactValue'
import type { VaultDevice } from '@bindings/VaultDevice'
import { displayValue, HOLZI_ADDRESS_KINDS, KINDS } from '~/lib/contacts/kinds'
import { contactTitle } from '~/lib/contacts/self'

const { t, d } = useI18n()
const { errString } = useErrorString()
const router = useTabRouter()
const store = useContactsStore()
const { getAsync, setPhotoAsync } = useContacts()
const { pickOneAsync } = usePickedFile()
const { kindName, labelName } = useContactLabels()

const id = computed(() => router.route.params.id ?? '')
const contact = ref<Contact | null>(null)
const missing = ref(false)
const deleteOpen = ref(false)
const shareOpen = ref(false)

async function loadAsync() {
  try {
    contact.value = await getAsync(id.value)
    missing.value = false
  } catch {
    contact.value = null
    missing.value = true
  }
}

watch(id, loadAsync, { immediate: true })
// Another window, an agent or a sync changed the contact: show the new state.
watch(
  () => store.headersById.get(id.value),
  (header, before) => {
    if (header !== before && store.hasLoadedOnce) void loadAsync()
  },
)

const KIND_ORDER = KINDS.map((kind) => kind.id as string)

/** The values without the holzi addresses, by kind; unknown kinds last. */
const groupedValues = computed(() => {
  const groups = new Map<string, ContactValue[]>()
  for (const value of contact.value?.values ?? []) {
    if ((HOLZI_ADDRESS_KINDS as readonly string[]).includes(value.kind))
      continue
    groups.set(value.kind, [...(groups.get(value.kind) ?? []), value])
  }
  return [...groups.entries()].sort(([a], [b]) => {
    const ia = KIND_ORDER.indexOf(a)
    const ib = KIND_ORDER.indexOf(b)
    return (ia < 0 ? KIND_ORDER.length : ia) - (ib < 0 ? KIND_ORDER.length : ib)
  })
})

const holziAddresses = computed(() =>
  (contact.value?.values ?? []).filter((value) =>
    (HOLZI_ADDRESS_KINDS as readonly string[]).includes(value.kind),
  ),
)

/** "Ich" for the own contact (spec 060); the name parts under it keep the real name. */
const title = computed(() =>
  contact.value
    ? contactTitle(contact.value, {
        self: t('contacts.self'),
        noName: t('contacts.noName'),
      })
    : '',
)

const subtitle = computed(() =>
  [contact.value?.head.jobTitle, contact.value?.head.organization]
    .filter(Boolean)
    .join(' · '),
)

const { listVaultDevicesAsync } = useDevice()
const devices = ref<VaultDevice[]>([])
onMounted(async () => {
  try {
    devices.value = await listVaultDevicesAsync()
  } catch {
    devices.value = []
  }
})

/** The name of a device of this vault: this one, its alias, or "another device". */
function deviceName(uuid: string): string {
  const device = devices.value.find(
    (candidate) => candidate.vaultDeviceUuid === uuid,
  )
  if (device?.isCurrent) return t('contacts.origin.thisDevice')
  return device?.alias ?? t('contacts.origin.otherDevice')
}

const origin = computed(() => {
  const value = contact.value?.origin
  if (!value) return ''
  const device = deviceName(value.device)
  switch (value.kind) {
    case 'phone':
      return t('contacts.origin.phone', {
        device,
        account: value.account ?? '',
      })
    case 'card':
      return t('contacts.origin.card', { device })
    default:
      return value.caller === 'user'
        ? t('contacts.origin.holzi', { device })
        : t('contacts.origin.holziBy', { device, caller: value.caller })
  }
})

async function choosePhotoAsync() {
  const file = await pickOneAsync([
    {
      name: t('contacts.photo.images'),
      extensions: ['jpg', 'jpeg', 'png', 'webp', 'gif'],
    },
  ])
  if (!file || !contact.value) return
  try {
    contact.value = await setPhotoAsync(contact.value.id, file)
  } catch (cause) {
    toast.error(errString(cause))
  }
}

async function removePhotoAsync() {
  if (!contact.value) return
  try {
    contact.value = await setPhotoAsync(contact.value.id, null)
  } catch (cause) {
    toast.error(errString(cause))
  }
}
</script>

<template>
  <div class="flex min-h-0 flex-col gap-4 p-3" data-testid="contacts-entry">
    <div class="flex items-center gap-1">
      <UiButton
        variant="ghost"
        size="icon"
        :aria-label="t('contacts.back')"
        :tooltip="t('contacts.back')"
        data-testid="contacts-back"
        @click="router.back()"
      >
        <Icon name="lucide:arrow-left" class="size-4" />
      </UiButton>
      <div class="flex-1" />
      <template v-if="contact">
        <UiButton
          variant="outline"
          data-testid="contacts-edit"
          @click="router.push(`/entry/${contact.id}/edit`)"
        >
          <Icon name="lucide:pencil" class="size-4" />
          {{ t('contacts.edit') }}
        </UiButton>
        <UiButton
          variant="outline"
          data-testid="contacts-share-open"
          @click="shareOpen = true"
        >
          <Icon name="lucide:share-2" class="size-4" />
          {{ t('contacts.share.action') }}
        </UiButton>
        <UiButton
          v-if="!contact.isSelf"
          variant="ghost"
          size="icon"
          :aria-label="t('contacts.delete.title')"
          :tooltip="t('contacts.delete.title')"
          data-testid="contacts-delete"
          @click="deleteOpen = true"
        >
          <Icon name="lucide:trash-2" class="size-4 text-destructive" />
        </UiButton>
      </template>
    </div>

    <p
      v-if="missing"
      class="p-4 text-muted-foreground"
      data-testid="contacts-missing"
    >
      {{ t('contacts.missing') }}
    </p>

    <template v-else-if="contact">
      <header class="flex items-center gap-4 px-1">
        <ShadcnDropdownMenu>
          <ShadcnDropdownMenuTrigger as-child>
            <button
              type="button"
              class="rounded-full focus-visible:ring-2 focus-visible:ring-primary"
              :aria-label="t('contacts.photo.change')"
              data-testid="contacts-photo"
            >
              <ContactsAvatar
                :name="contact.name"
                :photo="contact.photo"
                size="lg"
              />
            </button>
          </ShadcnDropdownMenuTrigger>
          <ShadcnDropdownMenuContent align="start">
            <ShadcnDropdownMenuItem class="gap-2" @select="choosePhotoAsync">
              <Icon name="lucide:image" class="size-4" />
              {{ t('contacts.photo.choose') }}
            </ShadcnDropdownMenuItem>
            <ShadcnDropdownMenuItem
              v-if="contact.photo"
              class="gap-2"
              @select="removePhotoAsync"
            >
              <Icon name="lucide:image-off" class="size-4" />
              {{ t('contacts.photo.remove') }}
            </ShadcnDropdownMenuItem>
          </ShadcnDropdownMenuContent>
        </ShadcnDropdownMenu>
        <div class="flex min-w-0 flex-col">
          <h1
            class="truncate text-2xl font-semibold"
            data-testid="contacts-entry-name"
          >
            {{ title }}
          </h1>
          <p
            v-if="contact.isSelf && contact.name"
            class="truncate text-muted-foreground"
            data-testid="contacts-entry-real-name"
          >
            {{ contact.name }}
          </p>
          <p
            v-if="contact.head.nickname"
            class="truncate text-muted-foreground"
          >
            „{{ contact.head.nickname }}“
          </p>
          <p v-if="subtitle" class="truncate text-muted-foreground">
            {{ subtitle }}
          </p>
        </div>
      </header>

      <SettingsGroup
        v-for="[kind, values] in groupedValues"
        :key="kind"
        :label="kindName(kind)"
        :data-testid="`contacts-kind-${kind}`"
      >
        <SettingsRow
          v-for="value in values"
          :key="value.id"
          :title="displayValue(value.kind, value.value)"
          :description="labelName(value.label)"
          :data-testid="`contacts-value-${value.id}`"
        >
          <span
            v-if="!value.known"
            class="text-xs text-muted-foreground"
            data-testid="contacts-value-readonly"
          >
            {{ t('contacts.readOnly') }}
          </span>
          <ContactsValueActions
            :kind="value.kind"
            :value="value.value"
            :label="kindName(value.kind)"
          />
        </SettingsRow>
      </SettingsGroup>

      <SettingsGroup
        v-if="holziAddresses.length"
        :label="t('contacts.holziAddresses.title')"
        data-testid="contacts-holzi-addresses"
      >
        <SettingsRow
          v-for="value in holziAddresses"
          :key="value.id"
          :title="value.value"
          :description="`${kindName(value.kind)} · ${t('contacts.holziAddresses.unconfirmed')}`"
        >
          <ContactsValueActions
            :kind="value.kind"
            :value="value.value"
            :label="kindName(value.kind)"
          />
        </SettingsRow>
      </SettingsGroup>

      <SettingsGroup
        v-if="contact.card"
        :label="t('contacts.card.confirmed')"
        data-testid="contacts-card-addresses"
      >
        <SettingsRow
          :title="contact.card.npub"
          :description="`${kindName('holzi_vault')} · ${t('contacts.card.confirmedAt', { at: d(new Date(contact.card.issuedAt), { dateStyle: 'medium' }) })}`"
          data-testid="contacts-card-vault"
        >
          <ContactsValueActions
            kind="holzi_vault"
            :value="contact.card.npub"
            :label="kindName('holzi_vault')"
          />
        </SettingsRow>
        <SettingsRow
          v-for="device in contact.card.devices"
          :key="device.devicePubkey"
          :title="device.endpointId"
          :description="device.irohRelay ?? kindName('holzi_iroh')"
          data-testid="contacts-card-device"
        >
          <ContactsValueActions
            kind="holzi_iroh"
            :value="device.endpointId"
            :label="kindName('holzi_iroh')"
          />
        </SettingsRow>
        <SettingsRow
          v-for="relay in contact.card.nostrRelays"
          :key="relay"
          :title="relay"
          :description="kindName('holzi_nostr_relay')"
        >
          <ContactsValueActions
            kind="holzi_nostr_relay"
            :value="relay"
            :label="kindName('holzi_nostr_relay')"
          />
        </SettingsRow>
      </SettingsGroup>

      <SettingsGroup
        v-if="contact.head.note"
        :label="t('contacts.fields.note')"
      >
        <li class="whitespace-pre-wrap px-4 py-3" data-testid="contacts-note">
          {{ contact.head.note }}
        </li>
      </SettingsGroup>

      <div v-if="contact.groups.length" class="flex flex-wrap gap-1.5 px-1">
        <button
          v-for="group in contact.groups"
          :key="group.id"
          type="button"
          class="rounded-full bg-muted px-3 py-1 text-sm hover:bg-foreground/10"
          @click="router.push(`/group/${group.id}`)"
        >
          {{ group.name }}
        </button>
      </div>

      <ContactsTargetsSection
        v-if="!contact.isSelf"
        :contact="contact"
        :device-name="deviceName"
        @changed="loadAsync"
      />

      <ContactsFileSharesSection
        v-if="!contact.isSelf"
        :contact-id="contact.id"
      />

      <p
        v-if="!contact.isSelf"
        class="px-1 text-sm text-muted-foreground"
        data-testid="contacts-origin"
      >
        {{ origin }}
      </p>

      <ContactsShareDialog v-model:open="shareOpen" :contact-id="contact.id" />

      <ContactsDeleteDialog
        v-if="!contact.isSelf"
        v-model:open="deleteOpen"
        :ids="[contact.id]"
        :name="contact.name"
        @done="router.replace('/')"
      />
    </template>
  </div>
</template>
