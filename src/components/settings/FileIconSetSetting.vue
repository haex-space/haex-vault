<script setup lang="ts">
/**
 * "Icon-Set für Dateien" (spec 053, FR-001, FR-012, FR-013): the sets in a boxed list, each with a
 * preview, saved as soon as one is chosen, without a save button (spec 023 FR-021). The choice is
 * the vault preference `files.iconSet`; the file browser reads it again on its own, on every
 * device of the vault.
 */
import { invoke } from '@tauri-apps/api/core'
import type { IconSelection } from '@bindings/IconSelection'
import type { IconSetInfo } from '@bindings/IconSetInfo'
import { selectionValue, DEFAULT_SET } from '~/lib/files/fileIcons'

const KEY = 'files.iconSet'

const { t } = useI18n()
const { errString } = useErrorString()
const { setPrefAsync } = usePreferences()

const sets = ref<IconSetInfo[]>([])
const selection = ref<IconSelection | null>(null)
const busy = ref(false)
const savedFlash = ref(false)
const opError = ref<string | null>(null)

const chosen = computed(() =>
  selectionValue(selection.value?.set ?? DEFAULT_SET),
)

async function loadAsync() {
  try {
    const [list, current] = await Promise.all([
      invoke<IconSetInfo[]>('file_icon_sets_list'),
      invoke<IconSelection>('file_icon_set_selection'),
    ])
    sets.value = list
    selection.value = current
  } catch (cause) {
    opError.value = errString(cause)
  }
}

async function saveAsync(value: string) {
  busy.value = true
  savedFlash.value = false
  opError.value = null
  try {
    // action-exempt: the icons are the window's own look; agents get no action for them (FR-016).
    await setPrefAsync({ kind: 'vault' }, KEY, value)
    savedFlash.value = true
    await loadAsync()
  } catch (cause) {
    opError.value = errString(cause)
  } finally {
    busy.value = false
  }
}

function nameOf(info: IconSetInfo): string {
  return info.ref.kind === 'builtin'
    ? t(`settings.iconSet.sets.${info.ref.name}`)
    : info.name
}

const unknown = computed(() => selection.value?.state === 'unknown')

onMounted(() => void loadAsync())
onVaultTablesChanged(['preferences', 'haex_file_icon_sets'], loadAsync)
</script>

<template>
  <section class="flex flex-col gap-2">
    <SettingsGroup
      radio
      :label="t('settings.iconSet.label')"
      :model-value="chosen"
      :disabled="busy"
      data-testid="settings-icon-set"
      @update:model-value="saveAsync(String($event))"
    >
      <SettingsOptionRow
        v-for="info in sets"
        :key="selectionValue(info.ref)"
        type="radio"
        :value="selectionValue(info.ref)"
        :title="nameOf(info)"
        :description="`${info.author} · ${info.license}`"
        :data-testid="`settings-icon-set-option-${selectionValue(info.ref)}`"
      >
        <template #trailing>
          <SettingsFileIconSetPreview :set="info.ref" />
          <SettingsFileIconSetRemove
            v-if="info.removable && info.ref.kind === 'custom'"
            :id="info.ref.id"
            :name="info.name"
            @removed="loadAsync"
            @failed="opError = $event"
          />
        </template>
      </SettingsOptionRow>
    </SettingsGroup>

    <p class="px-1 text-sm text-muted-foreground">
      {{ t('settings.iconSet.description') }}
    </p>
    <SettingsFileIconSetImport @imported="loadAsync" />
    <p v-if="unknown" class="px-1 text-sm text-warning" role="status">
      {{ t('settings.iconSet.unknown') }}
    </p>
    <div class="flex flex-wrap items-center gap-2">
      <UiButton
        variant="outline"
        size="sm"
        :disabled="busy || chosen === selectionValue(DEFAULT_SET)"
        data-testid="settings-icon-set-reset"
        @click="saveAsync(selectionValue(DEFAULT_SET))"
      >
        {{ t('settings.iconSet.reset') }}
      </UiButton>
    </div>
    <p v-if="savedFlash" class="px-1 text-xs text-success" role="status">
      {{ t('settings.iconSet.saved') }}
    </p>
    <p v-if="opError" class="px-1 text-xs text-destructive" role="alert">
      {{ t('settings.iconSet.failed') }}: {{ opError }}
    </p>
  </section>
</template>
