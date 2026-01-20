<template>
  <UiDrawer
    v-model:open="open"
    direction="right"
    :title="t('launcher.title')"
    :description="t('launcher.description')"
    :overlay="false"
    :modal="false"
    :handle-only="true"
    :dismissible="true"
  >
    <span
      ref="launcherButtonWrapperRef"
      class="inline-block"
      data-testid="launcher-button"
    >
      <UButton
        icon="material-symbols:apps"
        color="neutral"
        variant="outline"
        v-bind="$attrs"
        size="lg"
      />
    </span>

    <template #body>
      <div class="flex flex-wrap">
        <!-- All launcher items (system windows + enabled extensions, alphabetically sorted) -->
        <HaexExtensionLauncherItem
          v-for="item in launcherItems"
          :id="item.id"
          :key="item.id"
          :type="item.type"
          :name="item.name"
          :icon="item.icon"
          @click="openItem(item)"
          @drag-move="handleLauncherDragMove"
        />

        <!-- Disabled Extensions (grayed out) -->
        <UiButton
          v-for="extension in disabledExtensions"
          :key="extension.id"
          square
          size="lg"
          variant="ghost"
          :disabled="true"
          :ui="{
            base: 'size-24 flex flex-wrap text-sm items-center justify-center overflow-visible opacity-40',
            leadingIcon: 'size-10',
            label: 'w-full',
          }"
          :icon="extension.iconUrl || extension.icon || 'i-heroicons-puzzle-piece-solid'"
          :label="extension.name"
          :tooltip="`${extension.name} (${t('disabled')})`"
        />
      </div>
    </template>

    <template #footer>
      <UButton
        color="neutral"
        variant="outline"
        block
        size="lg"
        icon="i-heroicons-arrow-left-on-rectangle"
        :label="t('logout.label')"
        @click="onLogout"
      />
    </template>
  </UiDrawer>
</template>

<script setup lang="ts">
defineOptions({
  inheritAttrs: false,
})

const extensionStore = useExtensionsStore()
const windowManagerStore = useWindowManagerStore()

const { t } = useI18n()

const open = ref(false)
const launcherButtonWrapperRef = useTemplateRef<HTMLElement>(
  'launcherButtonWrapperRef',
)

// Update launcher button position for window animations
const updateLauncherButtonPosition = () => {
  if (!launcherButtonWrapperRef.value) return

  const rect = launcherButtonWrapperRef.value.getBoundingClientRect()
  windowManagerStore.setLauncherButtonPosition({
    x: rect.left,
    y: rect.top,
    width: rect.width,
    height: rect.height,
  })
}

// Update position on mount and when window resizes
onMounted(() => {
  nextTick(() => {
    updateLauncherButtonPosition()
  })
})

useEventListener(window, 'resize', updateLauncherButtonPosition)

// Unified launcher item type
interface LauncherItem {
  id: string
  name: string
  icon: string
  type: 'system' | 'extension'
}

// Combine system windows and enabled extensions, sorted alphabetically
const launcherItems = computed(() => {
  const items: LauncherItem[] = []

  // Add system windows
  const systemWindows = windowManagerStore.getAllSystemWindows()
  systemWindows.forEach((sysWin: SystemWindowDefinition) => {
    items.push({
      id: sysWin.id,
      name: sysWin.name,
      icon: sysWin.icon,
      type: 'system',
    })
  })

  // Add enabled extensions (iconUrl is computed in store)
  const enabledExtensions = extensionStore.availableExtensions.filter(
    (ext) => ext.enabled,
  )
  enabledExtensions.forEach((ext) => {
    items.push({
      id: ext.id,
      name: ext.name,
      icon: ext.iconUrl || 'i-heroicons-puzzle-piece-solid',
      type: 'extension',
    })
  })

  // Sort alphabetically by name
  return items.sort((a, b) => a.name.localeCompare(b.name))
})

// Disabled extensions (shown grayed out at the end)
const disabledExtensions = computed(() => {
  return extensionStore.availableExtensions.filter((ext) => !ext.enabled)
})

// Open launcher item (system window or extension)
const openItem = async (item: LauncherItem) => {
  try {
    // Open the window with correct type and sourceId
    await windowManagerStore.openWindowAsync({
      sourceId: item.id,
      type: item.type,
      icon: item.icon,
      title: item.name,
    })

    open.value = false
  } catch (error) {
    console.log(error)
  }
}

// Handle drag move - close launcher when actual movement is detected
const handleLauncherDragMove = () => {
  // Now that movement is detected, it's safe to close the drawer
  // The drag overlay should be fully operational at this point
  open.value = false
}

// Logout - close vault and navigate back to vault page
const onLogout = async () => {
  open.value = false

  // Close the current vault (removes it from openVaults)
  const vaultStore = useVaultStore()
  await vaultStore.closeAsync()

  // Navigate back to vault selection page
  await navigateTo(useLocalePath()({ name: 'vault' }))
}
</script>

<i18n lang="yaml">
de:
  disabled: Deaktiviert
  marketplace: Marketplace
  launcher:
    title: App Launcher
    description: Wähle eine App zum Öffnen
  logout:
    label: Vault schließen
  contextMenu:
    open: Öffnen
    addToDesktop: Zum Desktop hinzufügen
    uninstall: Deinstallieren
  success:
    addedToDesktop: Zum Desktop hinzugefügt
  error:
    noWorkspace: Kein Workspace aktiv
    addToDesktop: Konnte nicht zum Desktop hinzugefügt werden
  uninstall:
    confirm:
      title: Erweiterung deinstallieren
      description: Möchtest du wirklich "{name}" deinstallieren? Diese Aktion kann nicht rückgängig gemacht werden.
      button: Deinstallieren

en:
  disabled: Disabled
  marketplace: Marketplace
  launcher:
    title: App Launcher
    description: Select an app to open
  logout:
    label: Close Vault
  contextMenu:
    open: Open
    addToDesktop: Add to Desktop
    uninstall: Uninstall
  success:
    addedToDesktop: Added to Desktop
  error:
    noWorkspace: No workspace active
    addToDesktop: Could not add to Desktop
  uninstall:
    confirm:
      title: Uninstall Extension
      description: Do you really want to uninstall "{name}"? This action cannot be undone.
      button: Uninstall
</i18n>
