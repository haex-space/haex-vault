<script setup lang="ts">
/**
 * The version state of one device (spec 059, FR-001 to FR-003, FR-007): the three values two devices
 * compare before they sync. A value that differs from this device's is red, carries an arrow and a
 * text for screen readers, so it reads as lower or higher without the colour. A device whose state
 * this device never saw shows a hint, not values. The row wraps, so it never scrolls sideways.
 */
import type { VaultDevice } from '@bindings/VaultDevice'
import { SCHEMA_FIELDS, type SchemaDiff } from '~/lib/sync/deviceStatus'

const props = defineProps<{
  device: VaultDevice
  /** How this device's values stand against this device's own; `null` for the own row. */
  diff: SchemaDiff | null
}>()

const { t } = useI18n()

const LABELS = {
  protocol: 'protocol',
  holziMigration: 'migration',
  crdtTrigger: 'trigger',
} as const

const items = computed(() => {
  const schema = props.device.schema
  if (schema === null) return []
  return SCHEMA_FIELDS.map((field) => ({
    field,
    value: schema[field],
    label: t(`settings.federation.schema.${LABELS[field]}`),
    order: props.diff?.[field] ?? 'equal',
  }))
})
</script>

<template>
  <span
    v-if="items.length > 0"
    class="flex flex-wrap gap-x-3"
    data-testid="settings-device-schema"
  >
    <span
      v-for="item in items"
      :key="item.field"
      class="whitespace-nowrap"
      :class="item.order !== 'equal' && 'font-medium text-destructive'"
      :data-field="item.field"
      :data-order="item.order"
    >
      {{ item.label }} {{ item.value }}
      <template v-if="item.order !== 'equal'">
        <span aria-hidden="true">{{ item.order === 'lower' ? '↓' : '↑' }}</span>
        <span class="sr-only">{{
          t(`settings.federation.schema.${item.order}`)
        }}</span>
      </template>
    </span>
  </span>
  <span
    v-else-if="!device.isCurrent"
    data-testid="settings-device-schema-unknown"
  >
    {{ t('settings.federation.schema.unknown') }}
  </span>
</template>
