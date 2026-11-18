<script setup lang="ts">
/**
 * A secret that stays hidden until the user asks (spec 034, FR-005, US1 scenario 2): a toggle that
 * shows the value until it is switched off again (amends spec 034, which held it while pressed).
 * The value is fetched for that moment, lives only in this component and ends when the toggle is
 * switched off, when the entry or place is left, or when the entry changes. It is never written to
 * the store, the title or the history.
 */
import { toast } from 'vue-sonner'

const props = defineProps<{
  /** Fetches the value for the moment the user asks (one backend call; nothing is cached). */
  fetch: () => Promise<string>
  /** Changes when the secret it stands for changes (entry or field), which hides it again. */
  identity: string
  /** Whether the secret has a value at all; without one nothing can be revealed. */
  present: boolean
  label: string
  /** Part of the test ids. */
  kind: string
}>()

const { t } = useI18n()
const { errString } = useErrorString()

const value = ref<string | null>(null)
let token = 0

function hide() {
  token += 1
  value.value = null
}

async function showAsync() {
  const mine = ++token
  try {
    const revealed = await props.fetch()
    // A later hide or show outran this one.
    if (mine === token) value.value = revealed
  } catch (cause) {
    toast.error(errString(cause))
  }
}

function toggle() {
  if (value.value === null) void showAsync()
  else hide()
}

watch(() => props.identity, hide)
onBeforeUnmount(hide)
</script>

<template>
  <span class="flex min-w-0 max-w-full items-center gap-2" data-no-swipe>
    <span
      class="min-w-0 flex-1 font-mono text-sm"
      :class="value === null ? 'truncate text-muted-foreground' : 'break-all'"
      :data-testid="`passwords-value-${kind}`"
    >
      <template v-if="!present">{{ t('passwords.noValue') }}</template>
      <template v-else-if="value === null">••••••••</template>
      <template v-else>{{ value }}</template>
    </span>
    <UiButton
      v-if="present"
      type="button"
      variant="ghost"
      size="icon"
      class="shrink-0"
      :aria-label="t('passwords.reveal', { field: label })"
      :aria-pressed="value !== null"
      :tooltip="
        value === null ? t('passwords.revealHint') : t('passwords.hideHint')
      "
      :data-testid="`passwords-reveal-${kind}`"
      @click="toggle"
    >
      <Icon
        :name="value === null ? 'lucide:eye' : 'lucide:eye-off'"
        class="size-4"
      />
    </UiButton>
  </span>
</template>
