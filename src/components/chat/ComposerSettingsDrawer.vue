<script setup lang="ts">
/**
 * The composer's model and effort settings on a small screen, as a drawer instead of the popover
 * of a wide one. The popover's model select floats next to its trigger, which sits right above the
 * on-screen keyboard: once the keyboard opened for the search field the window shrank, the trigger
 * moved and the list jumped around under the finger. A drawer is fixed to the bottom edge and
 * knows the keyboard, and the list is a plain one, with nothing to position.
 */
import { computed, ref, watch } from 'vue'
import type { EffortState } from '~/composables/useReasoningPreference'
import type { ModelGroup } from '~/composables/useModelInventory'
import { filterModelGroups } from '~/lib/models/search'

const props = defineProps<{
  modelId: string
  modelGroups: ModelGroup[]
  effortChoices: { id: string; label: string }[]
  effortLevel: string | null
  effortState: EffortState
  disabled?: boolean
  modelDisabled?: boolean
}>()

const emit = defineEmits<{
  'update:modelId': [modelId: string]
  'update:effortLevel': [effortLevel: string | null]
}>()

const open = defineModel<boolean>('open', { default: false })
const { t } = useI18n()

const query = ref('')
const groups = computed<ModelGroup[]>(() =>
  filterModelGroups(props.modelGroups, query.value),
)

// A new search on every open, like the popover's (spec 031 FR-006).
watch(open, (isOpen) => {
  if (isOpen) query.value = ''
})

/** Picking a model is the drawer's job done: it closes, the composer shows the model. */
function pick(id: string) {
  emit('update:modelId', id)
  open.value = false
}
</script>

<template>
  <UiDrawerModal
    v-model:open="open"
    :title="t('chat.composer.settingsPopover.title')"
  >
    <template #content>
      <div data-testid="chat-settings-drawer">
        <UiInput
          v-model="query"
          type="search"
          :label="t('chat.composer.settingsPopover.modelSearch.placeholder')"
          :disabled="disabled || modelDisabled"
          data-testid="chat-model-search"
        />
        <p
          v-if="groups.length === 0"
          data-testid="chat-model-search-empty"
          class="px-1 py-3 text-sm text-muted-foreground"
        >
          {{ t('chat.composer.settingsPopover.modelSearch.noResults') }}
        </p>
        <div
          v-for="group in groups"
          :key="group.providerId"
          class="mt-3"
          role="group"
          :aria-label="group.providerName"
        >
          <div class="px-1 pb-1 text-xs font-medium text-muted-foreground">
            {{ group.providerName }}
          </div>
          <button
            v-for="model in group.models"
            :key="model.id"
            type="button"
            class="flex w-full items-center gap-3 rounded-md px-3 py-2 text-left text-base hover:bg-accent disabled:opacity-50"
            :class="model.id === modelId ? 'bg-accent' : ''"
            :disabled="disabled || modelDisabled || model.disabled"
            :aria-pressed="model.id === modelId"
            :data-value="model.id"
            @click="pick(model.id)"
          >
            <span class="min-w-0 flex-1 truncate">{{ model.name }}</span>
            <Icon
              v-if="model.id === modelId"
              name="lucide:check"
              class="size-5 shrink-0"
              :aria-hidden="true"
            />
          </button>
        </div>
        <ChatComposerProfileSelect
          id="chat-profile-drawer"
          class="mt-4"
          :disabled="disabled"
        />
        <ChatComposerEffortSelect
          id="effort-level-drawer"
          class="mt-4"
          :effort-choices="effortChoices"
          :effort-level="effortLevel"
          :effort-state="effortState"
          :disabled="disabled"
          @update:effort-level="emit('update:effortLevel', $event)"
        />
      </div>
    </template>
  </UiDrawerModal>
</template>
