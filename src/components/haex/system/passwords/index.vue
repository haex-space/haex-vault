<template>
  <HaexSystem>
    <template #header>
      <div class="flex items-center gap-2">
        <UIcon name="i-mdi-key-variant" class="size-5 text-primary" />
        <span class="font-semibold">{{ t('title') }}</span>
      </div>
    </template>
    <div class="flex flex-col items-center justify-center h-full gap-4 text-muted">
      <UIcon name="i-mdi-key-variant" class="size-16 opacity-40" />
      <p class="text-sm">{{ t('placeholder') }}</p>
    </div>
  </HaexSystem>
</template>

<script setup lang="ts">
const { t } = useI18n()
</script>

<i18n lang="yaml">
de:
  title: Passwörter
  placeholder: Passwort-Manager – UI wird in Etappe 1 implementiert.
en:
  title: Passwords
  placeholder: Password manager — UI lands in stage 1.
</i18n>
