<script setup lang="ts">
/**
 * Persistent banner that surfaces critical vault-failure events
 * (mutex poisoning, schema drift, audit-log write failures) recorded
 * by `crate::critical::lock_or_fail` on the Rust side.
 *
 * Renders nothing when no unacknowledged event exists — composable's
 * `current.value` is `null` outside of a vault-degraded state.
 *
 * Severity drives both color and action set:
 *   Critical → red, primary action restarts the vault.
 *   Warning  → orange (warning palette), single "Understood" action.
 *
 * The component is mounted ONCE in `app.vue` so it appears on every
 * page; gating per-route would risk hiding the banner during exactly
 * the navigation that exposes the user to data risk.
 */
const { translated, current, acting, acknowledge, restartApp } =
  useCriticalFailureBanner()

const isCritical = computed(() => translated.value?.severity === 'Critical')

// UAlert color mapping. We keep this here (not in the composable) so a
// designer tweaking the visual hierarchy doesn't have to touch backend-
// adjacent code.
const alertColor = computed(() => (isCritical.value ? 'error' : 'warning'))
const alertIcon = computed(() =>
  isCritical.value ? 'i-lucide-octagon-alert' : 'i-lucide-triangle-alert',
)
</script>

<template>
  <Teleport to="body">
    <Transition
      enter-active-class="transition duration-150 ease-out"
      enter-from-class="opacity-0 -translate-y-2"
      leave-active-class="transition duration-100 ease-in"
      leave-to-class="opacity-0 -translate-y-2"
    >
      <div
        v-if="current && translated"
        class="fixed inset-x-0 top-0 z-[100] flex justify-center pointer-events-none"
        role="alert"
        aria-live="assertive"
      >
        <UAlert
          :color="alertColor"
          :icon="alertIcon"
          variant="solid"
          class="m-3 max-w-3xl w-full shadow-lg pointer-events-auto"
        >
          <template #title>
            <div class="flex items-center gap-2">
              <span>{{ translated.title }}</span>
              <span
                v-if="translated.countSuffix"
                class="text-xs font-normal opacity-80"
              >
                {{ translated.countSuffix }}
              </span>
            </div>
          </template>

          <template #description>
            <div class="space-y-2">
              <p>{{ translated.description }}</p>
              <p v-if="translated.risk" class="text-sm opacity-90">
                {{ translated.risk }}
              </p>
              <p v-if="translated.action" class="text-sm opacity-90">
                {{ translated.action }}
              </p>
              <div class="flex gap-2 pt-1">
                <UButton
                  v-if="isCritical"
                  :loading="acting"
                  :disabled="acting"
                  color="error"
                  variant="solid"
                  size="sm"
                  @click="restartApp"
                >
                  {{ translated.actionLabel }}
                </UButton>
                <UButton
                  :loading="acting && !isCritical"
                  :disabled="acting"
                  :color="isCritical ? 'neutral' : 'warning'"
                  variant="soft"
                  size="sm"
                  @click="acknowledge"
                >
                  {{ isCritical ? $t('criticalFailures.dismissed') : translated.actionLabel }}
                </UButton>
              </div>
            </div>
          </template>
        </UAlert>
      </div>
    </Transition>
  </Teleport>
</template>
