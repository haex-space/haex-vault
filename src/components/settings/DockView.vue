<script setup lang="ts">
/**
 * "Allgemein → Dock" (spec 045): style, edge, alignment and mode of the dock on this device
 * (FR-022–FR-024, FR-036), and the dock's entries for the whole vault — reorder, remove, add
 * (FR-020). The launcher cannot be removed (FR-006); an app not available on this device stays
 * listed as such and can be removed (FR-021). Every choice is saved at once.
 */
import type { SettingsSelectOption } from '~/components/settings/Select.vue'
import {
  dockItemKey,
  type DockControlId,
  type DockItemState,
  type DockPlacement,
} from '~/lib/wm/dock'

const { t } = useI18n()
const wm = useWindowManagerStore()
const dock = useDock()

const FIELDS = [
  { field: 'style', values: ['bar', 'wheel'] },
  { field: 'edge', values: ['top', 'bottom', 'left', 'right'] },
  { field: 'align', values: ['start', 'center', 'end'] },
  { field: 'mode', values: ['reserved', 'floating', 'autohide'] },
] as const

function optionsFor(
  field: keyof DockPlacement,
  values: readonly string[],
): SettingsSelectOption[] {
  return values.map((value) => ({
    value,
    label: t(`settings.dock.${field}Options.${value}`),
  }))
}

function setPlacement(field: keyof DockPlacement, value: string) {
  void dock.setPlacementAsync({ [field]: value })
}

const CONTROL_ICONS: Record<DockControlId, string> = {
  launcher: 'lucide:layout-grid',
  workspaces: 'lucide:monitor',
  windows: 'lucide:copy',
  assistant: 'lucide:mic',
}

function appOf(item: DockItemState) {
  return item.kind === 'app'
    ? wm.apps().find((app) => app.id === item.appId)
    : undefined
}

function titleOf(item: DockItemState): string {
  if (item.kind === 'control') return t(`wm.dock.controls.${item.id}`)
  const app = appOf(item)
  return app ? (app.title ?? t(app.titleKey)) : item.appId
}

function iconOf(item: DockItemState): string {
  if (item.kind === 'control') return CONTROL_ICONS[item.id]
  return appOf(item)?.icon ?? 'lucide:puzzle'
}

/** Apps not yet in the dock and removed controls, as "control:<id>" / "app:<id>". */
const addable = computed<SettingsSelectOption[]>(() => {
  const present = new Set(dock.items.value.map(dockItemKey))
  const controls = (['assistant', 'workspaces', 'windows'] as const)
    .filter((id) => !present.has(`control:${id}`))
    .map((id) => ({
      value: `control:${id}`,
      label: t(`wm.dock.controls.${id}`),
      icon: CONTROL_ICONS[id],
    }))
  const apps = wm
    .apps()
    .filter((app) => !present.has(`app:${app.id}`))
    .map((app) => ({
      value: `app:${app.id}`,
      label: app.title ?? t(app.titleKey),
      icon: app.icon,
      iconUrl: app.iconUrl,
    }))
  return [...controls, ...apps]
})

function add(value: string) {
  const [kind, id] = [
    value.slice(0, value.indexOf(':')),
    value.slice(value.indexOf(':') + 1),
  ]
  if (kind === 'control') void dock.addControlAsync(id as DockControlId)
  else if (kind === 'app') void dock.pinAsync(id)
}
</script>

<template>
  <div class="flex flex-col gap-4">
    <SettingsGroup :label="t('settings.dock.placement')">
      <template v-for="{ field, values } in FIELDS" :key="field">
        <SettingsRow
          :title="t(`settings.dock.${field}`)"
          :label-for="`settings-dock-${field}`"
        >
          <SettingsSelect
            :id="`settings-dock-${field}`"
            :model-value="dock.placement.value[field]"
            :options="optionsFor(field, values)"
            :data-testid="`settings-dock-${field}`"
            @update:model-value="(value) => setPlacement(field, value)"
          />
        </SettingsRow>
      </template>
    </SettingsGroup>

    <SettingsGroup :label="t('settings.dock.items')">
      <SettingsRow
        v-for="(item, index) in dock.items.value"
        :key="dockItemKey(item)"
        :title="titleOf(item)"
        :icon="iconOf(item)"
        :icon-url="appOf(item)?.iconUrl"
        :description="
          item.available ? undefined : t('settings.dock.unavailable')
        "
        :class="item.available ? '' : 'opacity-60'"
        :data-testid="`settings-dock-item-${dockItemKey(item)}`"
      >
        <UiButton
          variant="ghost"
          size="icon"
          :disabled="index === 0"
          :aria-label="t('settings.dock.moveUp')"
          :title="t('settings.dock.moveUp')"
          @click="dock.moveAsync(index, index - 1)"
        >
          <Icon name="lucide:arrow-up" class="h-4 w-4" :aria-hidden="true" />
        </UiButton>
        <UiButton
          variant="ghost"
          size="icon"
          :disabled="index === dock.items.value.length - 1"
          :aria-label="t('settings.dock.moveDown')"
          :title="t('settings.dock.moveDown')"
          @click="dock.moveAsync(index, index + 1)"
        >
          <Icon name="lucide:arrow-down" class="h-4 w-4" :aria-hidden="true" />
        </UiButton>
        <UiButton
          v-if="!(item.kind === 'control' && item.id === 'launcher')"
          variant="ghost"
          size="icon"
          :aria-label="t('settings.dock.remove')"
          :title="t('settings.dock.remove')"
          data-testid="settings-dock-remove"
          @click="dock.removeAsync(index)"
        >
          <Icon name="lucide:x" class="h-4 w-4" :aria-hidden="true" />
        </UiButton>
      </SettingsRow>
      <SettingsRow
        v-if="addable.length > 0"
        :title="t('settings.dock.add')"
        label-for="settings-dock-add"
      >
        <SettingsSelect
          id="settings-dock-add"
          model-value=""
          :options="addable"
          data-testid="settings-dock-add"
          @update:model-value="add"
        />
      </SettingsRow>
    </SettingsGroup>
  </div>
</template>
