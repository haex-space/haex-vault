<script setup lang="ts">
/**
 * The settings category "Kontakte" (spec 051, FR-015a): the way into the address book, the sync
 * with this phone's address book, what the own Visitenkarte carries and the blocked vaults.
 */
const { t } = useI18n()
const store = useContactsStore()
const { openApp } = useWmTab()

onMounted(() => {
  if (!store.hasLoadedOnce) void store.reloadAsync()
})
</script>

<template>
  <div class="flex flex-col gap-6" data-testid="settings-contacts">
    <SettingsGroup>
      <SettingsRow
        icon="lucide:contact"
        :title="t('settings.contacts.open')"
        :description="
          t(
            'settings.contacts.count',
            { count: store.headers.length },
            store.headers.length,
          )
        "
        navigates
        data-testid="settings-contacts-open"
        @select="openApp('system.contacts')"
      />
    </SettingsGroup>
    <SettingsContactsPhoneSetting />
    <SettingsContactsCardSetting />
    <SettingsContactsBlockedSetting />
  </div>
</template>
