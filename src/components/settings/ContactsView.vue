<script setup lang="ts">
/**
 * The settings category "Kontakte" (spec 051, FR-015a): the way into the address book, the sync
 * with this phone's address book and the blocked vaults.
 */
const { t } = useI18n()
const store = useContactsStore()
const { openApp } = useWmTab()

// The own contact is no entry of the address book to count.
const contactCount = computed(
  () => store.headers.filter((header) => !header.isSelf).length,
)

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
          t('settings.contacts.count', { count: contactCount }, contactCount)
        "
        navigates
        data-testid="settings-contacts-open"
        @select="openApp('system.contacts')"
      />
    </SettingsGroup>
    <SettingsContactsPhoneSetting />
    <SettingsContactsBlockedSetting />
  </div>
</template>
