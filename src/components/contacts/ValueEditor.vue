<script setup lang="ts">
/**
 * One value in the editor of a contact (spec 051, FR-003, FR-007, FR-008): its label (a fixed one
 * of the kind or one of the user's own) and its value, entered as the kind needs it — an address
 * in parts, a messenger as service and handle, a date as `YYYY-MM-DD` or `--MM-DD`. A value of a
 * kind this version does not know is shown read-only.
 */
import type { DraftValue } from '~/lib/contacts/draft'
import {
  ADDRESS_PARTS,
  customLabel,
  displayValue,
  kindOf,
  makeCustomLabel,
  parseAddress,
  parseEventDate,
  parseIm,
  serializeAddress,
  serializeEventDate,
  serializeIm,
} from '~/lib/contacts/kinds'

const model = defineModel<DraftValue>({ required: true })

const emit = defineEmits<{
  remove: []
}>()

const { t } = useI18n()
const fieldLabels = useFieldLabels()
const { kindName, labelName } = useContactLabels()

const definition = computed(() => kindOf(model.value.kind))
const uid = useId()

const CUSTOM = '__custom__'

const labelOptions = computed(() => [
  ...(definition.value?.labels ?? []).map((label) => ({
    value: label,
    label: labelName(label),
  })),
  { value: CUSTOM, label: t('contacts.editor.ownLabel') },
])

const own = computed(() => customLabel(model.value.label))

const labelChoice = computed({
  get: () => (own.value !== null ? CUSTOM : model.value.label),
  set: (choice: string) => {
    model.value = {
      ...model.value,
      label: choice === CUSTOM ? makeCustomLabel(own.value ?? '') : choice,
    }
  },
})

function setOwnLabel(text: string | number | null | undefined) {
  model.value = { ...model.value, label: `custom:${String(text ?? '')}` }
}

function setValue(value: string) {
  model.value = { ...model.value, value }
}

const address = computed(() => parseAddress(model.value.value))
function setAddressPart(part: (typeof ADDRESS_PARTS)[number], text: string) {
  setValue(serializeAddress({ ...address.value, [part]: text }))
}

const im = computed(() => parseIm(model.value.value))
const date = computed(() => parseEventDate(model.value.value))
</script>

<template>
  <li
    class="flex flex-col gap-2 px-4 py-3"
    :data-testid="`contacts-editor-value-${model.kind}`"
  >
    <div class="flex items-center gap-2">
      <Icon
        :name="definition?.icon ?? 'lucide:circle-help'"
        class="size-4 shrink-0"
      />
      <span class="min-w-0 flex-1 truncate text-sm font-medium">{{
        kindName(model.kind)
      }}</span>
      <UiButton
        v-if="model.known"
        type="button"
        variant="ghost"
        size="icon"
        :aria-label="
          t('contacts.editor.removeValue', { what: kindName(model.kind) })
        "
        :tooltip="
          t('contacts.editor.removeValue', { what: kindName(model.kind) })
        "
        data-testid="contacts-editor-remove-value"
        @click="emit('remove')"
      >
        <Icon name="lucide:x" class="size-4" />
      </UiButton>
    </div>

    <p
      v-if="!model.known"
      class="text-sm text-muted-foreground"
      data-testid="contacts-editor-readonly"
    >
      {{ labelName(model.label) }}:
      {{ displayValue(model.kind, model.value) }} ·
      {{ t('contacts.readOnly') }}
    </p>

    <template v-else>
      <div v-if="model.kind === 'custom'" class="flex flex-col gap-2">
        <UiInput
          :id="`${uid}-name`"
          :model-value="own ?? ''"
          :label="t('contacts.editor.customName')"
          :labels="fieldLabels.input.value"
          label-bg="var(--muted)"
          data-testid="contacts-editor-custom-name"
          @update:model-value="setOwnLabel"
        />
      </div>
      <div v-else class="flex flex-wrap gap-2">
        <UiSelect
          :id="`${uid}-label`"
          v-model="labelChoice"
          class="min-w-36"
          :options="labelOptions"
          :label="t('contacts.editor.label')"
          data-testid="contacts-editor-label"
        />
        <UiInput
          v-if="own !== null"
          :id="`${uid}-own`"
          class="min-w-36 flex-1"
          :model-value="own"
          :label="t('contacts.editor.ownLabel')"
          :labels="fieldLabels.input.value"
          label-bg="var(--muted)"
          @update:model-value="setOwnLabel"
        />
      </div>

      <div
        v-if="definition?.input === 'address'"
        class="grid gap-2 sm:grid-cols-2"
      >
        <UiInput
          v-for="part in ADDRESS_PARTS"
          :id="`${uid}-${part}`"
          :key="part"
          :model-value="address[part] ?? ''"
          :label="t(`contacts.address.${part}`)"
          :labels="fieldLabels.input.value"
          label-bg="var(--muted)"
          :data-testid="`contacts-editor-address-${part}`"
          @update:model-value="setAddressPart(part, String($event ?? ''))"
        />
      </div>
      <div
        v-else-if="definition?.input === 'im'"
        class="grid gap-2 sm:grid-cols-2"
      >
        <UiInput
          :id="`${uid}-service`"
          :model-value="im.service"
          :label="t('contacts.editor.service')"
          :labels="fieldLabels.input.value"
          label-bg="var(--muted)"
          @update:model-value="
            setValue(serializeIm(String($event ?? ''), im.handle))
          "
        />
        <UiInput
          :id="`${uid}-handle`"
          :model-value="im.handle"
          :label="t('contacts.editor.handle')"
          :labels="fieldLabels.input.value"
          label-bg="var(--muted)"
          @update:model-value="
            setValue(serializeIm(im.service, String($event ?? '')))
          "
        />
      </div>
      <UiInput
        v-else-if="definition?.input === 'date'"
        :id="`${uid}-date`"
        :model-value="date"
        :label="t('contacts.editor.date')"
        :placeholder="t('contacts.editor.datePlaceholder')"
        :labels="fieldLabels.input.value"
        label-bg="var(--muted)"
        data-testid="contacts-editor-date"
        @update:model-value="setValue(serializeEventDate(String($event ?? '')))"
      />
      <UiInput
        v-else
        :id="`${uid}-value`"
        :model-value="model.value"
        :type="
          model.kind === 'email'
            ? 'email'
            : model.kind === 'phone'
              ? 'tel'
              : 'text'
        "
        :label="kindName(model.kind)"
        :placeholder="model.kind === 'holzi_vault' ? 'npub1…' : undefined"
        :labels="fieldLabels.input.value"
        label-bg="var(--muted)"
        data-testid="contacts-editor-value"
        @update:model-value="setValue(String($event ?? ''))"
      />
    </template>
  </li>
</template>
