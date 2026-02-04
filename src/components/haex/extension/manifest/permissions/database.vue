<template>
  <HaexExtensionManifestPermissionsAccordion
    :title="t('permission.read')"
    :items="database?.read"
  />
  <HaexExtensionManifestPermissionsAccordion
    :title="t('permission.write')"
    :items="database?.write"
  />
  <HaexExtensionManifestPermissionsAccordion
    :title="t('permission.create')"
    :items="database?.create"
  />
</template>

<script setup lang="ts">
defineProps<{
  database?: {
    read?: Record<string, boolean>[]
    write?: Record<string, boolean>[]
    create?: Record<string, boolean>[]
  }
}>()
const { t } = useI18n()
</script>

<i18n lang="yaml">
de:
  permission:
    read: Lesen
    write: Schreiben
    create: Erstellen

en:
  permission:
    read: Read
    write: Write
    create: Create
</i18n>
