<script setup lang="ts">
/** The place `/generator` (spec 034, US3): the generator on its own, outside an entry. */
const { t } = useI18n()
const router = useTabRouter()
</script>

<template>
  <div class="min-h-0 flex-1 overflow-y-auto px-4 pb-6 @md:px-6">
    <div class="mx-auto flex w-full max-w-3xl flex-col gap-4">
      <div class="flex items-center gap-2 pt-1">
        <UiButton
          variant="ghost"
          size="icon"
          class="-ml-2 shrink-0"
          :aria-label="t('passwords.back')"
          @click="router.back()"
        >
          <Icon name="lucide:arrow-left" class="size-5" />
        </UiButton>
        <h1 class="text-2xl font-bold">{{ t('passwords.generator.title') }}</h1>
      </div>
      <PasswordsGeneratorPanel />
    </div>
  </div>
</template>
