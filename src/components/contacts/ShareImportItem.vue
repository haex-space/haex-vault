<script setup lang="ts">
/**
 * One item of a share that is read in (spec 060 FR-024b to FR-025): what it says, whether the
 * target has it already, and the choices the person has: add, update (naming the value to
 * replace), skip; keep or update for a field the target filled; add or skip the holzi addresses as
 * a whole. An item that is the same, or a field the target fills without asking, has no control.
 */
import type { ContactShareItem } from '@bindings/ContactShareItem'
import { displayValue } from '~/lib/contacts/kinds'
import type { ImportChoice, ImportDecision } from '~/lib/contacts/shareImport'

const props = defineProps<{
  item: ContactShareItem
  decision?: ImportDecision
}>()

const emit = defineEmits<{
  choose: [choice: ImportChoice]
  replace: [valueId: string]
}>()

const { t, locale } = useI18n()
const { kindName, labelName } = useContactLabels()

const FIELDS: Record<string, string> = {
  prefix: 'namePrefix',
  given: 'givenName',
  middle: 'middleName',
  family: 'familyName',
  suffix: 'nameSuffix',
}

const title = computed(() => {
  const { item } = props
  switch (item.item.type) {
    case 'value':
      return displayValue(item.valueKind ?? '', item.value, locale.value)
    case 'holziAddresses':
      return t('contacts.holziAddresses.title')
    default:
      return item.value
  }
})

const description = computed(() => {
  const { item } = props
  switch (item.item.type) {
    case 'value':
      return `${kindName(item.valueKind ?? '')} · ${labelName(item.label)}`
    case 'holziAddresses': {
      const found = item.addresses
      return found
        ? t('contacts.import.addresses', {
            npub: found.npub,
            devices: found.devices,
            relays: found.relays,
            newDevices: found.newDevices,
            newRelays: found.newRelays,
          })
        : ''
    }
    case 'namePart':
      return t(`contacts.fields.${FIELDS[item.item.field] ?? 'givenName'}`)
    default:
      return t(`contacts.fields.${item.item.type}`)
  }
})

const choice = computed(
  () => props.decision?.choice ?? props.item.default ?? props.item.choices[0],
)

const options = computed(() =>
  props.item.choices.map((value) => ({
    value,
    label: t(`contacts.import.choice.${value}`),
  })),
)

const replaceOptions = computed(() =>
  props.item.existing.map((existing) => ({
    value: existing.id,
    label: `${displayValue(existing.kind, existing.value, locale.value)} · ${labelName(existing.label)}`,
  })),
)
</script>

<template>
  <li
    class="flex flex-col gap-2 px-4 py-3"
    :data-testid="`contacts-import-item-${item.id}`"
  >
    <div class="min-w-0">
      <p class="break-words">{{ title }}</p>
      <p class="truncate text-sm text-muted-foreground">{{ description }}</p>
      <p
        v-if="item.state === 'same'"
        class="text-sm text-muted-foreground"
        data-testid="contacts-import-same"
      >
        {{ t('contacts.import.same') }}
      </p>
      <p
        v-else-if="item.choices.length === 0"
        class="text-sm text-muted-foreground"
        data-testid="contacts-import-fills"
      >
        {{ t('contacts.import.fills') }}
      </p>
      <p
        v-else-if="item.state === 'conflict' && item.current"
        class="text-sm text-muted-foreground"
        data-testid="contacts-import-current"
      >
        {{ t('contacts.import.current', { value: item.current }) }}
      </p>
    </div>
    <template v-if="item.choices.length > 0">
      <UiSelect
        :model-value="choice"
        :options="options"
        :label="t('contacts.import.choiceLabel')"
        :data-testid="`contacts-import-choice-${item.id}`"
        @update:model-value="emit('choose', $event as ImportChoice)"
      />
      <UiSelect
        v-if="choice === 'update' && replaceOptions.length > 1"
        :model-value="decision?.valueId ?? replaceOptions[0]?.value"
        :options="replaceOptions"
        :label="t('contacts.import.replaces')"
        :data-testid="`contacts-import-replaces-${item.id}`"
        @update:model-value="emit('replace', String($event))"
      />
      <p
        v-else-if="choice === 'update' && replaceOptions.length === 1"
        class="text-sm text-muted-foreground"
      >
        {{
          t('contacts.import.replacesOne', { value: replaceOptions[0]?.label })
        }}
      </p>
      <p
        v-if="choice === 'replace'"
        class="text-sm text-muted-foreground"
        data-testid="contacts-import-replace-hint"
      >
        {{ t('contacts.import.replaceHint') }}
      </p>
    </template>
  </li>
</template>
