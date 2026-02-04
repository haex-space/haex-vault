<template>
  <HaexExtensionManifestPermissionsAccordion
    :title="t('http.access')"
    :items="http"
  />
</template>

<script setup lang="ts">
defineProps<{ http?: Record<string, boolean>[] }>()
const { t } = useI18n()
</script>

<i18n lang="yaml">
de:
  http:
    access: Internet Zugriff

en:
  http:
    access: Internet Access
</i18n>
