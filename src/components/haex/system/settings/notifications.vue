<template>
  <HaexSystemSettingsLayout :title="t('title')">
    <UFormField :label="t('label')" :description="t('description')">
      <UiButton
        :label="t('requestPermission')"
        @click="requestNotificationPermissionAsync"
      />
    </UFormField>
  </HaexSystemSettingsLayout>
</template>

<script setup lang="ts">
const { t } = useI18n()

const { requestNotificationPermissionAsync } = useNotificationStore()
</script>

<i18n lang="yaml">
de:
  title: Benachrichtigungen
  label: Benachrichtigungen
  description: Erlaube Benachrichtigungen für diese App
  requestPermission: Benachrichtigung erlauben
en:
  title: Notifications
  label: Notifications
  description: Allow notifications for this app
  requestPermission: Grant Permission
</i18n>
