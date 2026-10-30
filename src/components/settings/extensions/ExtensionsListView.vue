<script setup lang="ts">
import { formatFileSize } from '~/lib/passwords/format'
import { detailsPath } from '~/lib/marketplace/registry'

/**
 * The category "Erweiterungen" (spec 017, US1, T049): the installed extensions with icon, name,
 * version and their state on this device, installing one from a file, and the kept data of
 * removed extensions with its size here (US7, T092), and developer mode (US12, T096). The list
 * follows `extensions-changed` through the extensions store. Developer mode loads a project folder,
 * so it is left out where folders cannot be chosen (spec 043 FR-016). An extension with a known
 * update (spec 069, FR-017) gets a row below it that opens that entry in the catalog; the row only
 * shows what the last check or catalog call found and asks no marketplace.
 */
const { t } = useI18n()
const { capabilities } = useDeviceCapabilities()
const store = useExtensionsStore()
const updates = useKnownUpdates()
const { openApp } = useWmTab()
const installing = ref(false)
const installed = computed(() =>
  store.list.filter((e) => e.state === 'installed' && !e.dev),
)
const kept = computed(() => store.list.filter((e) => e.state !== 'installed'))

function openUpdate(name: string) {
  const update = updates.byName.value.get(name)
  if (update)
    openApp('system.marketplace', detailsPath(update.sourceId, update.slug))
}

function description(version: string | undefined, status: string | undefined) {
  const parts = [version ? t('settings.extensions.version', { version }) : null]
  if (status) parts.push(t(`extensions.status.${status}`))
  return parts.filter((part) => part !== null).join(' · ')
}
</script>

<template>
  <section class="flex flex-col gap-3">
    <SettingsGroup>
      <SettingsRow
        navigates
        icon="lucide:file-plus"
        :title="t('settings.extensions.installFromFile')"
        :description="t('settings.extensions.installFromFileHint')"
        data-testid="extensions-install-from-file"
        @select="installing = true"
      />
    </SettingsGroup>

    <SettingsGroup
      :label="t('settings.locations.extensions.browserBridge.title')"
    >
      <SettingsRow
        icon="lucide:globe"
        :title="t('settings.locations.extensions.browserBridge.title')"
        :description="
          t('settings.locations.extensions.browserBridge.description')
        "
        to="/extensions/browser-bridge"
        data-testid="extensions-browser-bridge"
      />
    </SettingsGroup>

    <SettingsGroup
      :label="t('settings.locations.extensions.marketplaces.title')"
    >
      <SettingsRow
        icon="lucide:store"
        :title="t('settings.locations.extensions.marketplaces.title')"
        :description="
          t('settings.locations.extensions.marketplaces.description')
        "
        to="/extensions/marketplaces"
        data-testid="extensions-marketplaces"
      />
    </SettingsGroup>

    <SettingsGroup
      v-if="installed.length > 0"
      :label="t('settings.extensions.installed')"
    >
      <template v-for="extension in installed" :key="extension.id">
        <SettingsRow
          :title="extension.title"
          :description="description(extension.version, extension.statusHere)"
          :to="`/extensions/${extension.id}`"
          :data-extension-id="extension.id"
          data-testid="extension-row"
        >
          <template #title>
            <span class="flex items-center gap-3">
              <img
                v-if="store.icons[extension.id]"
                :src="store.icons[extension.id]"
                alt=""
                class="size-5 object-contain"
              />
              <Icon v-else name="lucide:puzzle" class="size-5" />
              <span>{{ extension.title }}</span>
            </span>
          </template>
          <span
            v-if="!extension.enabled"
            class="text-xs text-muted-foreground"
            >{{ t('extensions.status.disabled') }}</span
          >
        </SettingsRow>
        <SettingsRow
          v-if="updates.byName.value.get(extension.name)"
          navigates
          icon="lucide:arrow-up-circle"
          :title="
            t('settings.extensions.updateAvailable', {
              version: updates.byName.value.get(extension.name)
                ?.latestCompatible,
            })
          "
          :description="t('settings.extensions.updateAvailableHint')"
          :data-testid="`extension-update-hint-${extension.id}`"
          @select="openUpdate(extension.name)"
        />
      </template>
    </SettingsGroup>
    <p v-else class="px-1 text-sm text-muted-foreground">
      {{ t('settings.extensions.none') }}
    </p>

    <SettingsGroup
      v-if="kept.length > 0"
      :label="t('settings.extensions.keptData')"
    >
      <SettingsRow
        v-for="extension in kept"
        :key="extension.id"
        icon="lucide:archive"
        :title="extension.title"
        :description="
          extension.keptDataBytes === undefined
            ? t('settings.extensions.keptDataSizeUnknown')
            : formatFileSize(extension.keptDataBytes)
        "
        :to="`/extensions/${extension.id}`"
        :data-extension-id="extension.id"
        data-testid="extension-kept-data"
      />
    </SettingsGroup>

    <SettingsExtensionsExtensionDevModeView
      v-if="capabilities?.folderPick !== false"
    />

    <ExtensionsInstallDialog v-model:open="installing" />
  </section>
</template>
