<script setup lang="ts">
/**
 * The TOTP block of an entry (spec 034, FR-003, US1 scenario 3): the code and the time left, asked
 * from the backend (the secret never reaches the window) and asked again when the time is up. A
 * value that arrived invalid (sync, import) shows a message with the actions to replace or remove the
 * secret instead of a code. The code depends on the system clock, which the hint says.
 */
import { toast } from 'vue-sonner'
import type { OtpState } from '@bindings/OtpState'
import type { TotpCode } from '@bindings/TotpCode'

const props = defineProps<{
  itemId: string
  state: OtpState
}>()

const emit = defineEmits<{
  replace: []
  remove: []
  copy: []
}>()

const { t } = useI18n()
const { errString } = useErrorString()
const { totpCodeAsync } = usePasswords()

const current = ref<TotpCode | null>(null)
const remaining = ref(0)
let timer: ReturnType<typeof setInterval> | null = null
let deadline = 0

function stop() {
  if (timer !== null) clearInterval(timer)
  timer = null
}

async function loadAsync() {
  stop()
  if (props.state !== 'valid') {
    current.value = null
    return
  }
  try {
    const code = await totpCodeAsync(props.itemId)
    current.value = code
    deadline = Date.now() + code.remainingSeconds * 1000
    remaining.value = code.remainingSeconds
    timer = setInterval(() => {
      const left = Math.ceil((deadline - Date.now()) / 1000)
      remaining.value = Math.max(left, 0)
      // The next code is due: ask again, the backend knows the exact time.
      if (left <= 0) void loadAsync()
    }, 500)
  } catch (cause) {
    current.value = null
    toast.error(errString(cause))
  }
}

/** `123 456`, in groups of three from the left. */
const grouped = computed(() => {
  const code = current.value?.code ?? ''
  return code.replace(/(\d{3})(?=\d)/g, '$1 ')
})

const fraction = computed(() =>
  current.value ? remaining.value / current.value.period : 0,
)

watch(() => [props.itemId, props.state], loadAsync)
onMounted(loadAsync)
onBeforeUnmount(stop)
</script>

<template>
  <div
    v-if="state === 'invalid'"
    class="flex flex-col gap-2 rounded-lg border border-destructive/40 p-3"
    role="alert"
    data-testid="passwords-totp-invalid"
  >
    <p class="text-sm">{{ t('passwords.totp.invalid') }}</p>
    <div class="flex flex-wrap gap-2">
      <UiButton variant="outline" size="sm" @click="emit('replace')">
        {{ t('passwords.totp.replace') }}
      </UiButton>
      <UiButton variant="outline" size="sm" @click="emit('remove')">
        {{ t('passwords.totp.remove') }}
      </UiButton>
    </div>
  </div>
  <div v-else-if="state === 'valid'" class="flex flex-col gap-1">
    <div class="flex items-center gap-3">
      <span
        class="font-mono text-2xl tracking-wider"
        data-testid="passwords-totp-code"
        >{{ grouped }}</span
      >
      <span
        class="text-sm text-muted-foreground"
        data-testid="passwords-totp-left"
      >
        {{ t('passwords.totp.left', { seconds: remaining }) }}
      </span>
      <UiButton
        variant="ghost"
        size="icon"
        class="ml-auto shrink-0"
        :aria-label="t('passwords.copy', { field: t('passwords.fields.totp') })"
        data-testid="passwords-copy-totp"
        @click="emit('copy')"
      >
        <Icon name="lucide:copy" class="size-4" />
      </UiButton>
    </div>
    <div
      class="h-1 overflow-hidden rounded-full bg-background"
      aria-hidden="true"
    >
      <div class="h-full bg-primary" :style="{ width: `${fraction * 100}%` }" />
    </div>
    <p class="text-xs text-muted-foreground">
      {{ t('passwords.totp.clockHint') }}
    </p>
  </div>
</template>
