<script setup lang="ts">
/**
 * The rows of the share dialog (spec 060 FR-009 to FR-014): the head fields and the details of the
 * contact, each with its checkbox, then the holzi addresses: for the own vault every device and
 * every Nostr-Relay on its own, for another contact's confirmed addresses one row. The name is not
 * a row, it always goes along (FR-011).
 */
import type { ContactShareOptions } from '@bindings/ContactShareOptions'
import { displayValue } from '~/lib/contacts/kinds'
import {
  HEAD_ROWS,
  toggleCard,
  toggleDevice,
  toggleHead,
  toggleRelay,
  toggleValue,
  type ShareSelection,
} from '~/lib/contacts/share'

const props = defineProps<{
  options: ContactShareOptions
  /** The name of a device of the own vault, by its `vault_device_uuid`. */
  deviceName: (uuid: string) => string
}>()

const selection = defineModel<ShareSelection>('selection', { required: true })

const { t, d, locale } = useI18n()
const { kindName, labelName } = useContactLabels()

const headRows = computed(() =>
  HEAD_ROWS.filter((row) => Boolean(props.options.head[row])),
)

const own = computed(() =>
  props.options.holzi.kind === 'own' ? props.options.holzi : null,
)
const foreignCard = computed(() =>
  props.options.holzi.kind === 'card' ? props.options.holzi : null,
)
</script>

<template>
  <div class="flex flex-col gap-4">
    <SettingsGroup v-if="headRows.length" :label="t('contacts.editor.name')">
      <li v-for="row in headRows" :key="row" class="px-4 py-3">
        <label class="flex items-center gap-3">
          <ShadcnCheckbox
            :model-value="selection[row]"
            :data-testid="`contacts-share-head-${row}`"
            @update:model-value="selection = toggleHead(selection, row)"
          />
          <span class="min-w-0 flex-1">
            <span class="block text-sm text-muted-foreground">{{
              t(`contacts.fields.${row}`)
            }}</span>
            <span class="block truncate">{{ options.head[row] }}</span>
          </span>
        </label>
      </li>
    </SettingsGroup>

    <SettingsGroup
      v-if="options.rows.length"
      :label="t('contacts.editor.values')"
    >
      <li
        v-for="row in options.rows"
        :key="row.id"
        class="px-4 py-3"
        :data-testid="`contacts-share-row-${row.id}`"
      >
        <label class="flex items-center gap-3">
          <ShadcnCheckbox
            :model-value="selection.values.includes(row.id)"
            :data-testid="`contacts-share-check-${row.id}`"
            @update:model-value="selection = toggleValue(selection, row.id)"
          />
          <span class="min-w-0 flex-1">
            <span class="block truncate">{{
              displayValue(row.kind, row.value, locale)
            }}</span>
            <span class="block text-sm text-muted-foreground">{{
              `${kindName(row.kind)} · ${labelName(row.label)}`
            }}</span>
          </span>
        </label>
      </li>
    </SettingsGroup>

    <SettingsGroup
      v-if="own"
      :label="t('contacts.share.ownAddresses')"
      data-testid="contacts-share-own"
    >
      <li
        v-if="!own.canSign"
        class="px-4 py-3 text-sm text-muted-foreground"
        data-testid="contacts-share-linked"
      >
        {{ t('contacts.share.linkedDevice') }}
      </li>
      <template v-else>
        <li class="px-4 py-3">
          <label class="flex items-center gap-3">
            <ShadcnCheckbox
              :model-value="true"
              disabled
              data-testid="contacts-share-identity"
            />
            <span class="min-w-0 flex-1">
              <span class="block truncate">{{ own.npub }}</span>
              <span class="block text-sm text-muted-foreground">{{
                t('contacts.share.identity')
              }}</span>
            </span>
          </label>
        </li>
        <li
          v-for="device in own.devices"
          :key="device.devicePubkey"
          class="px-4 py-3"
        >
          <label class="flex items-center gap-3">
            <ShadcnCheckbox
              :model-value="
                selection.holzi.kind === 'own' &&
                selection.holzi.devices.includes(device.devicePubkey)
              "
              :data-testid="`contacts-share-device-${device.devicePubkey}`"
              @update:model-value="
                selection = toggleDevice(selection, device.devicePubkey)
              "
            />
            <span class="min-w-0 flex-1">
              <span class="block truncate">{{
                deviceName(device.vaultDeviceUuid)
              }}</span>
              <span class="block truncate text-sm text-muted-foreground">{{
                `${kindName('holzi_iroh')}${device.irohRelay ? ` · ${device.irohRelay}` : ''}`
              }}</span>
            </span>
          </label>
        </li>
        <li v-for="(relay, index) in own.relays" :key="relay" class="px-4 py-3">
          <label class="flex items-center gap-3">
            <ShadcnCheckbox
              :model-value="
                selection.holzi.kind === 'own' &&
                selection.holzi.relays.includes(relay)
              "
              :data-testid="`contacts-share-relay-${index}`"
              @update:model-value="selection = toggleRelay(selection, relay)"
            />
            <span class="min-w-0 flex-1">
              <span class="block truncate">{{ relay }}</span>
              <span class="block text-sm text-muted-foreground">{{
                kindName('holzi_nostr_relay')
              }}</span>
            </span>
          </label>
        </li>
      </template>
    </SettingsGroup>

    <SettingsGroup
      v-if="foreignCard"
      :label="t('contacts.holziAddresses.title')"
      data-testid="contacts-share-confirmed"
    >
      <li class="px-4 py-3">
        <label class="flex items-start gap-3">
          <ShadcnCheckbox
            class="mt-1"
            :model-value="selection.holzi.kind === 'card'"
            data-testid="contacts-share-card"
            @update:model-value="selection = toggleCard(selection)"
          />
          <span class="min-w-0 flex-1">
            <span class="block truncate">{{ foreignCard.npub }}</span>
            <span class="block text-sm text-muted-foreground">{{
              t('contacts.share.confirmedRow', {
                devices: foreignCard.devices,
                relays: foreignCard.relays,
                at: d(new Date(foreignCard.issuedAt), { dateStyle: 'medium' }),
              })
            }}</span>
            <span class="mt-1 block text-sm text-muted-foreground">{{
              t('contacts.share.confirmedHint')
            }}</span>
          </span>
        </label>
      </li>
    </SettingsGroup>
  </div>
</template>
