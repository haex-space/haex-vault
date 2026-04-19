<template>
  <UiListContainer>
    <UiListItem
      highlight
      class="cursor-pointer"
    >
      <div class="flex items-center gap-2">
        <UIcon
          name="i-lucide-key-round"
          class="size-4 shrink-0 text-primary"
        />
        <span class="text-sm font-medium">{{ t('allPasswords') }}</span>
      </div>
    </UiListItem>
  </UiListContainer>
</template>

<script setup lang="ts">
const { t } = useI18n()
</script>

<i18n lang="yaml">
de:
  allPasswords: Alle Passwörter
en:
  allPasswords: All Passwords
</i18n>
