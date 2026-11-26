<script setup lang="ts">
/** The place `/generator` (spec 034, US3): the generator on its own, outside an entry. */
const { t } = useI18n()
const router = useTabRouter()
</script>

<template>
  <PagePasswordManager
    :back-label="t('passwords.back')"
    :title="t('passwords.generator.title')"
    @back="router.back()"
  >
    <PasswordsGeneratorPanel />
  </PagePasswordManager>
</template>
