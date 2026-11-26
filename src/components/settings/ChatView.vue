<script setup lang="ts">
/**
 * The settings category "Chat" (spec 072 FR-080): the files (Blossom-Server, automatic download
 * limit, the Blossom upload key) and the messages (requests of strangers, spec 099 FR-012; read
 * receipts, notifications).
 */
const { t } = useI18n()
</script>

<template>
  <div class="flex flex-col gap-6" data-testid="settings-chat">
    <SettingsGroup :label="t('settings.chat.groups.files')">
      <SettingsChatBlossomServerSetting />
      <SettingsChatBlossomKeySetting />
      <SettingsChatAutoDownloadSetting />
    </SettingsGroup>
    <SettingsGroup :label="t('settings.chat.groups.messages')">
      <SettingsChatFlagSetting
        pref-key="chat.requests.allowStrangers"
        scope="vault"
        :title="t('settings.chat.allowStrangers.label')"
        :description="t('settings.chat.allowStrangers.hint')"
        test-id="settings-chat-allow-strangers"
      />
      <SettingsChatFlagSetting
        pref-key="chat.receipts.read"
        scope="vault"
        :title="t('settings.chat.readReceipts.label')"
        :description="t('settings.chat.readReceipts.hint')"
        test-id="settings-chat-read-receipts"
      />
      <SettingsChatFlagSetting
        pref-key="chat.notifications"
        scope="device"
        :title="t('settings.chat.notifications.label')"
        :description="t('settings.chat.notifications.hint')"
        test-id="settings-chat-notifications"
      />
    </SettingsGroup>
  </div>
</template>
