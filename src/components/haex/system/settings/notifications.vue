<template>
  <div>
    <div class="p-6 border-b border-base-content/10">
      <h2 class="text-2xl font-bold">
        {{ t('title') }}
      </h2>
    </div>

    <div class="p-6 space-y-6">
      <UFormField :label="t('label')" :description="t('description')">
        <UiButton
          :label="t('requestPermission')"
          @click="requestNotificationPermissionAsync"
        />
      </UFormField>
    </div>
  </div>
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
