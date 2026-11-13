<script setup lang="ts">
/**
 * The place of an entry whose type this build does not know (spec 092-wallet-passes, FR-042): a
 * newer holzi wrote it. It says so and offers nothing that would write the entry; moving it to the
 * trash stays possible in the list.
 */
const { t } = useI18n()
</script>

<template>
  <div
    class="mx-auto flex max-w-xl flex-col items-center gap-3 p-8 text-center"
    data-testid="passwords-unknown-type"
  >
    <Icon name="lucide:circle-help" class="size-10 text-muted-foreground" />
    <h2 class="text-lg font-medium">{{ t('passwords.unknownType.title') }}</h2>
    <p class="text-muted-foreground">{{ t('passwords.unknownType.body') }}</p>
  </div>
</template>
