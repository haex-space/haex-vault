<template>
  <HaexExtensionManifestPermissionsAccordion
    :title="t('permission.read')"
    :items="filesystem?.read"
  />
  <HaexExtensionManifestPermissionsAccordion
    :title="t('permission.write')"
    :items="filesystem?.write"
  />
</template>

<script setup lang="ts">
defineProps<{
  filesystem?: {
    read?: Record<string, boolean>[]
    write?: Record<string, boolean>[]
  }
}>()
const { t } = useI18n()
</script>

<i18n lang="yaml">
de:
  permission:
    read: Lesen
    write: Schreiben

en:
  permission:
    read: Read
    write: Write
</i18n>
