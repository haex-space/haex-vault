<template>
  <div class="space-y-4">
    <!-- Preset buttons -->
    <div class="space-y-2">
      <label class="text-sm font-medium">{{ t('presetLabel') }}</label>
      <div class="flex gap-2 flex-wrap">
        <UButton
          v-for="p in presets"
          :key="p.key"
          :color="activePreset === p.key ? 'primary' : 'neutral'"
          :variant="activePreset === p.key ? 'solid' : 'outline'"
          size="sm"
          @click="applyPreset(p.key)"
        >
          {{ p.label }}
        </UButton>
      </div>
    </div>

    <!-- Custom checkboxes (only when Custom preset is active) -->
    <div
      v-if="activePreset === 'custom'"
      class="space-y-2 rounded-md border border-muted p-3"
    >
      <UCheckbox
        v-for="f in customFlags"
        :key="f.bit"
        :model-value="hasBit(f.bit)"
        :label="f.label"
        @update:model-value="(v: boolean | 'indeterminate') => toggleBit(f.bit, v === true)"
      />
    </div>

    <!-- Orthogonality warning (design doc §4) -->
    <UAlert
      color="warning"
      variant="soft"
      :icon="'i-lucide-alert-triangle'"
      :title="t('warningTitle')"
      :description="t('warningDescription', { spaceName })"
    />
  </div>
</template>

<script setup lang="ts">
import {
  ShareAccessFlags,
  SHARE_ACCESS_READ_ONLY,
  SHARE_ACCESS_READ_WRITE,
} from '~/lib/storage/shareAccessFlags'

type PresetKey = 'readOnly' | 'readWrite' | 'custom'

const props = defineProps<{
  spaceName: string
}>()

const accessFlags = defineModel<number>('accessFlags', { required: true })

const { t } = useI18n()

const presets = computed(() => [
  { key: 'readOnly' as const, label: t('presetReadOnly') },
  { key: 'readWrite' as const, label: t('presetReadWrite') },
  { key: 'custom' as const, label: t('presetCustom') },
])

const customFlags = computed(() => [
  { bit: ShareAccessFlags.LIST, label: t('flagList') },
  { bit: ShareAccessFlags.GET, label: t('flagGet') },
  { bit: ShareAccessFlags.PUT, label: t('flagPut') },
  { bit: ShareAccessFlags.DELETE, label: t('flagDelete') },
])

// activePreset derives from the current mask so external resets stay in sync.
// A mask that matches neither preset (e.g. LIST alone) falls through to 'custom'.
const activePreset = ref<PresetKey>('readOnly')

watch(
  accessFlags,
  (mask) => {
    if (mask === SHARE_ACCESS_READ_ONLY) activePreset.value = 'readOnly'
    else if (mask === SHARE_ACCESS_READ_WRITE) activePreset.value = 'readWrite'
    else activePreset.value = 'custom'
  },
  { immediate: true },
)

const applyPreset = (key: PresetKey) => {
  activePreset.value = key
  if (key === 'readOnly') accessFlags.value = SHARE_ACCESS_READ_ONLY
  else if (key === 'readWrite') accessFlags.value = SHARE_ACCESS_READ_WRITE
  // custom: keep current mask (user edits via checkboxes)
}

const hasBit = (bit: number) => (accessFlags.value & bit) === bit

const toggleBit = (bit: number, on: boolean) => {
  accessFlags.value = on
    ? accessFlags.value | bit
    : accessFlags.value & ~bit
}

// Referenced so lint won't complain about unused prop when only used in template
void props
</script>

<i18n lang="yaml">
de:
  presetLabel: Berechtigungs-Preset
  presetReadOnly: Nur Lesen
  presetReadWrite: Lesen + Schreiben
  presetCustom: Benutzerdefiniert
  flagList: Objekte auflisten
  flagGet: Objekte herunterladen
  flagPut: Objekte hochladen
  flagDelete: Objekte löschen
  warningTitle: Space-übergreifende Rechte
  warningDescription: 'Alle Mitglieder von Space „{spaceName}“ erhalten diese Rechte am Bucket – unabhängig von ihrer Space-Rolle.'
en:
  presetLabel: Permission preset
  presetReadOnly: Read-only
  presetReadWrite: Read + Write
  presetCustom: Custom
  flagList: List objects
  flagGet: Download objects
  flagPut: Upload objects
  flagDelete: Delete objects
  warningTitle: Space-wide grant
  warningDescription: 'All members of space “{spaceName}” receive these rights on the bucket — regardless of their role in the space.'
</i18n>
