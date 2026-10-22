<script setup lang="ts">
/**
 * Is the user here for this one passkey ceremony? (spec 049, US4, FR-030, FR-031, FR-033,
 * ADR-0015). Shows the extension (its name as its own claim), the site, the origin and the account;
 * for a new passkey the entry it goes to ("Neuer Eintrag" first), for a sign-in the passkey when
 * several fit. A site that requires user verification also gets the vault password. There is no
 * "remember": every ceremony asks again. Closing the dialog declines.
 */
import type { PresenceChoice } from '~/types/bindings/PresenceChoice'
import type { PresenceQuestion } from '~/types/bindings/PresenceQuestion'

const { t } = useI18n()
const { errString } = useErrorString()
const fieldLabels = useFieldLabels()
const store = useBrowserBridgeStore()

const question = computed<PresenceQuestion | null>(() =>
  store.shown?.question.kind === 'presence' ? store.shown.question : null,
)
const picked = ref('0')
const passphrase = ref('')
const failure = ref<string | null>(null)
const now = ref(Date.now())
let ticker: ReturnType<typeof setInterval> | undefined

watch(
  () => store.shown?.promptId,
  () => {
    picked.value = '0'
    passphrase.value = ''
    failure.value = null
  },
  { immediate: true },
)
onMounted(() => {
  ticker = setInterval(() => (now.value = Date.now()), 1000)
})
onBeforeUnmount(() => clearInterval(ticker))

const secondsLeft = computed(() =>
  Math.max(0, Math.ceil(((store.shown?.expiresAt ?? 0) - now.value) / 1000)),
)
const showChoices = computed(
  () =>
    !!question.value &&
    (question.value.ceremony === 'create' || question.value.choices.length > 1),
)
const canConfirm = computed(
  () => !question.value?.requireUv || passphrase.value.length > 0,
)

function titleOf(choice: PresenceChoice): string {
  if (choice.kind === 'newEntry') return t('browserBridge.presence.newEntry')
  if (choice.kind === 'entry') return choice.title
  return choice.userName ?? t('browserBridge.presence.noUser')
}

function descriptionOf(choice: PresenceChoice): string | undefined {
  if (choice.kind === 'entry') return choice.url ?? undefined
  if (choice.kind === 'passkey') return choice.itemTitle ?? undefined
  return undefined
}

async function answerAsync(confirm: boolean) {
  const shown = store.shown
  const asked = question.value
  if (!shown || !asked) return
  const choice = asked.choices[Number(picked.value)] ?? null
  try {
    await store.answerAsync(shown.promptId, {
      kind: 'presence',
      confirm,
      choice: confirm ? choice : null,
      passphrase: confirm && asked.requireUv ? passphrase.value : null,
    })
  } catch (error) {
    failure.value = errString(error)
  } finally {
    passphrase.value = ''
  }
}

function onUpdateOpen(open: boolean) {
  if (!open) void answerAsync(false)
}
</script>

<template>
  <UiDrawerModal
    v-if="question"
    :open="true"
    :title="t(`browserBridge.presence.title.${question.ceremony}`)"
    @update:open="onUpdateOpen"
  >
    <template #content>
      <form
        id="bridge-presence-form"
        class="space-y-4"
        data-testid="bridge-presence"
        @submit.prevent="canConfirm && answerAsync(true)"
      >
        <p class="text-sm">
          {{
            t(`browserBridge.presence.body.${question.ceremony}`, {
              name:
                question.client.clientName || t('browserBridge.client.noName'),
              site: question.rpName || question.rpId,
            })
          }}
        </p>
        <dl class="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-sm">
          <dt class="text-muted-foreground">
            {{ t('browserBridge.presence.site') }}
          </dt>
          <dd class="min-w-0 break-all" data-testid="bridge-presence-rp">
            {{ question.rpId }}
          </dd>
          <dt class="text-muted-foreground">
            {{ t('browserBridge.client.origin') }}
          </dt>
          <dd class="min-w-0 font-mono text-xs break-all">
            {{ question.origin }}
          </dd>
          <dt class="text-muted-foreground">
            {{ t('browserBridge.presence.challenge') }}
          </dt>
          <dd class="min-w-0 font-mono text-xs break-all">
            {{ question.challengeHash }}
          </dd>
          <template v-if="question.userName">
            <dt class="text-muted-foreground">
              {{ t('browserBridge.presence.user') }}
            </dt>
            <dd class="min-w-0 break-all">{{ question.userName }}</dd>
          </template>
          <dt class="text-muted-foreground">
            {{ t('browserBridge.presence.extension') }}
          </dt>
          <dd class="min-w-0 break-words">
            {{ question.client.clientName || t('browserBridge.client.noName') }}
            <span class="text-xs text-muted-foreground">
              ({{ t('browserBridge.client.claimed') }})
            </span>
          </dd>
        </dl>

        <SettingsGroup
          v-if="showChoices"
          v-model="picked"
          radio
          :label="t(`browserBridge.presence.choices.${question.ceremony}`)"
        >
          <SettingsOptionRow
            v-for="(choice, index) in question.choices"
            :key="index"
            type="radio"
            :value="String(index)"
            :title="titleOf(choice)"
            :description="descriptionOf(choice)"
            :data-testid="`bridge-presence-choice-${index}`"
          />
        </SettingsGroup>

        <UiInputPassword
          v-if="question.requireUv"
          id="bridge-presence-passphrase"
          v-model="passphrase"
          :label="t('browserBridge.presence.passphrase')"
          :labels="fieldLabels.password.value"
          autocomplete="current-password"
          data-testid="bridge-presence-passphrase"
        />
        <p class="text-xs text-muted-foreground">
          {{ t('browserBridge.presence.expires', { seconds: secondsLeft }) }}
        </p>
        <p v-if="failure" class="text-sm text-destructive" role="alert">
          {{ failure }}
        </p>
      </form>
    </template>
    <template #footer>
      <div class="flex justify-end gap-2">
        <UiButton
          type="button"
          size="sm"
          variant="outline"
          data-testid="bridge-presence-deny"
          @click="answerAsync(false)"
        >
          {{ t('browserBridge.approval.deny') }}
        </UiButton>
        <UiButton
          type="submit"
          form="bridge-presence-form"
          size="sm"
          :disabled="!canConfirm"
          data-testid="bridge-presence-confirm"
        >
          {{ t('browserBridge.presence.confirm') }}
        </UiButton>
      </div>
    </template>
  </UiDrawerModal>
</template>
