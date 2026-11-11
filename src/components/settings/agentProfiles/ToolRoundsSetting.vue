<script setup lang="ts">
/**
 * "Tool rounds per answer" in "KI → Agent profiles": how many rounds of tool calls the chat agent
 * may use for one answer before the answer ends with "round limit reached". A choice in a list,
 * saved as soon as it is made, without a save button (spec 023 FR-021). Stored as the vault
 * setting `chat.maxToolRounds`; the backend takes a whole number from 1 to 200 and reads it before
 * every round, so a change applies to an answer that is running.
 */
import { toast } from 'vue-sonner'

const CHOICES = [10, 20, 30, 50, 100] as const
const DEFAULT_ROUNDS = 30
const KEY = 'chat.maxToolRounds'

const { t } = useI18n()
const { errString } = useErrorString()
const { getPrefAsync, setPrefAsync } = usePreferences()

const rounds = ref<number>(DEFAULT_ROUNDS)

/** The choices, and a value stored by another build or device that is not one of them. */
const options = computed(() =>
  [...new Set<number>([...CHOICES, rounds.value])]
    .sort((a, b) => a - b)
    .map((value) => ({ value: String(value), label: String(value) })),
)

async function loadAsync() {
  try {
    const stored = await getPrefAsync({ kind: 'vault' }, KEY)
    const value = stored === null ? DEFAULT_ROUNDS : Number(stored)
    rounds.value =
      Number.isInteger(value) && value >= 1 && value <= 200
        ? value
        : DEFAULT_ROUNDS
  } catch (cause) {
    toast.error(errString(cause))
  }
}

async function chooseAsync(value: string) {
  const previous = rounds.value
  rounds.value = Number(value)
  try {
    // action-exempt: how long the chat agent may work on one answer is the user's own choice; no
    // agent gets an action to raise its own limit.
    await setPrefAsync({ kind: 'vault' }, KEY, value)
  } catch (cause) {
    rounds.value = previous
    toast.error(errString(cause))
  }
}

onMounted(loadAsync)
onVaultTablesChanged(['preferences'], loadAsync)
</script>

<template>
  <SettingsRow
    :title="t('agentProfiles.toolRounds.label')"
    :description="t('agentProfiles.toolRounds.hint')"
    label-for="chat-tool-rounds"
  >
    <SettingsSelect
      id="chat-tool-rounds"
      :model-value="String(rounds)"
      class="w-28"
      :options="options"
      data-testid="chat-tool-rounds"
      @update:model-value="chooseAsync"
    />
  </SettingsRow>
</template>
