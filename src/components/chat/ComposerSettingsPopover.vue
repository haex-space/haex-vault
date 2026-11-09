<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from 'vue'
import type { EffortState } from '~/composables/useReasoningPreference'
import { filterModelGroups } from '~/lib/models/search'

export type ModelGroup = {
  providerId: string
  providerName: string
  models: { id: string; name: string }[]
}

const MODEL_NAME_MAX_LENGTH = 20
const props = defineProps<{
  modelId: string
  modelName?: string
  modelGroups: ModelGroup[]
  /** Options the displayed model offers, already labelled for display. Only
   * used while `effortState` is `selectable`. */
  effortChoices: { id: string; label: string }[]
  /** The effective option id; `null` is Auto. */
  effortLevel: string | null
  /** Trigger text for the current choice; empty when there is no active
   * choice to show (spec 012 FR-005). */
  effortLabel: string
  /** `hidden` renders nothing; `managed`/`unknown` render a disabled control
   * with a state label; `selectable` renders the options plus Auto. */
  effortState: EffortState
  disabled?: boolean
  modelDisabled?: boolean
}>()

const { t } = useI18n()
const wm = useWindowManagerStore()
/** A small screen gets a drawer (`ComposerSettingsDrawer`) instead of this popover. */
const compact = computed(() => wm.compact)

const emit = defineEmits<{
  'update:modelId': [modelId: string]
  'update:effortLevel': [effortLevel: string | null]
}>()

const root = ref<HTMLElement | null>(null)
const trigger = ref<HTMLButtonElement | null>(null)
const popover = ref<HTMLElement | null>(null)
const isOpen = ref(false)
const popoverStyle = ref<Record<string, string>>({})
const modelQuery = ref('')
const modelSearchInput = ref<HTMLInputElement | null>(null)
const filteredModelGroups = computed(() =>
  filterModelGroups(props.modelGroups, modelQuery.value),
)

let reclaimSearchFocus = false

/** Resets the search on every open of the model select (FR-006) and focuses it, so typing
 * filters right away instead of first hitting Reka Select's own jump-to-letter search (not on a
 * touch screen, see below). */
function onModelSelectOpenChange(open: boolean) {
  if (!open) return
  modelQuery.value = ''
  // Not on a touch screen: focusing the field opens the on-screen keyboard, which shrinks the
  // window, so the popover and the list jump around and the list can no longer be hit. There the
  // search field is tapped on purpose.
  if (hasTouchScreen()) return
  reclaimSearchFocus = true
  nextTick(() => modelSearchInput.value?.focus())
}

/** The main pointer is a finger: no hardware keyboard to type into a field that took focus by itself. */
function hasTouchScreen(): boolean {
  return window.matchMedia('(pointer: coarse)').matches
}

/** Reka focuses the selected option once the list is positioned — after the focus
 * above. The first such focus move, before the user has pressed a key or moved the
 * pointer in the list, is taken back to the search field. Deferred: Reka tries
 * `[selected option, listbox]` in turn and moves on to the listbox itself if focus
 * is not where it put it once `focus()` returns. */
function onModelListFocusin(event: FocusEvent) {
  const input = modelSearchInput.value
  if (!reclaimSearchFocus || !input || event.target === input) return
  reclaimSearchFocus = false
  queueMicrotask(() => input.focus())
}
function stopReclaimingSearchFocus() {
  reclaimSearchFocus = false
}

/** Also clears the search once the model select has closed (Reka's `closeAutoFocus`
 * fires after the close animation, so the list does not visibly refill while fading
 * out). A closed Reka Select still renders its items to know the selected label; a
 * leftover query that filtered the selected model out would leave the trigger showing
 * the placeholder instead of the active model. */
function onModelSelectClosed() {
  modelQuery.value = ''
}

const truncatedModelName = computed(() => {
  const name = props.modelName || t('chat.model.choose')
  const characters = Array.from(name)
  return characters.length > MODEL_NAME_MAX_LENGTH
    ? `${characters.slice(0, MODEL_NAME_MAX_LENGTH - 1).join('')}…`
    : name
})

function updateModel(value: unknown) {
  if (typeof value === 'string') emit('update:modelId', value)
}

/** Keeps typing in the model search field from also triggering Reka Select's own
 * jump-to-letter type-ahead (research 031-chat-model-search R2) — every other key
 * (arrows, Enter, Escape, Tab) still reaches the listbox to move or commit the
 * highlighted item. */
const MODEL_SEARCH_PASSTHROUGH_KEYS = new Set([
  'ArrowDown',
  'ArrowUp',
  'Enter',
  'Escape',
  'Tab',
])
function onModelSearchKeydown(event: KeyboardEvent) {
  if (!MODEL_SEARCH_PASSTHROUGH_KEYS.has(event.key)) event.stopPropagation()
}

/** Typing while focus sits on an option (the user arrowed down, or Reka's own focus on
 * the selected item was not taken back) moves focus back to the search field, so the key edits the query
 * there instead of running Reka's jump-to-letter search. Space is left alone: on an
 * option it selects, as in any listbox. */
function onModelListKeydownCapture(event: KeyboardEvent) {
  reclaimSearchFocus = false
  const input = modelSearchInput.value
  if (!input || event.target === input) return
  if (event.ctrlKey || event.altKey || event.metaKey) return
  const edits =
    (event.key.length === 1 && event.key !== ' ') || event.key === 'Backspace'
  if (!edits) return
  event.stopPropagation()
  input.focus()
}

function positionPopover() {
  if (!isOpen.value || !trigger.value || !popover.value) return

  const triggerRect = trigger.value.getBoundingClientRect()
  const popoverWidth = Math.min(352, window.innerWidth - 32)
  const left = Math.min(
    Math.max(16, triggerRect.left),
    Math.max(16, window.innerWidth - popoverWidth - 16),
  )

  popoverStyle.value = {
    left: `${left}px`,
    bottom: `${Math.max(16, window.innerHeight - triggerRect.top + 8)}px`,
    width: `${popoverWidth}px`,
  }
}

function openPopover() {
  if (props.disabled) return
  modelQuery.value = ''
  isOpen.value = true
  nextTick(() => requestAnimationFrame(positionPopover))
}

function closePopover(focusTrigger = false) {
  isOpen.value = false
  popoverStyle.value = {}
  if (focusTrigger) nextTick(() => trigger.value?.focus())
}

function togglePopover() {
  if (isOpen.value) closePopover()
  else openPopover()
}

/**
 * Reka's Select (and any other Popper-based reka component) teleports its
 * floating content to `document.body` on its own — a sibling of this
 * popover's own `<Teleport>`, not a descendant of `popover`/`root`.
 * Without this check, pointerdown on a dropdown item counted as
 * "outside," closing (and unmounting) this whole panel before the
 * Select's own click could commit the new value — the pick never reached
 * `updateModel`.
 */
function isInsideRekaPopperContent(target: Node): boolean {
  return (
    target instanceof Element &&
    target.closest('[data-reka-popper-content-wrapper]') !== null
  )
}

function closeOnOutsideClick(event: PointerEvent) {
  if (
    isOpen.value &&
    !compact.value &&
    event.target instanceof Node &&
    !root.value?.contains(event.target) &&
    !popover.value?.contains(event.target) &&
    !isInsideRekaPopperContent(event.target)
  ) {
    closePopover()
  }
}

function closeOnEscape(event: KeyboardEvent) {
  if (event.key === 'Escape' && isOpen.value && !compact.value) {
    closePopover(true)
  }
}

function repositionPopover() {
  if (isOpen.value && !compact.value) positionPopover()
}

watch(compact, (isCompact) => {
  if (!isCompact && isOpen.value)
    nextTick(() => requestAnimationFrame(positionPopover))
})

onMounted(() => {
  document.addEventListener('pointerdown', closeOnOutsideClick)
  document.addEventListener('keydown', closeOnEscape)
  window.addEventListener('resize', repositionPopover)
  window.addEventListener('scroll', repositionPopover, true)
})

onBeforeUnmount(() => {
  document.removeEventListener('pointerdown', closeOnOutsideClick)
  document.removeEventListener('keydown', closeOnEscape)
  window.removeEventListener('resize', repositionPopover)
  window.removeEventListener('scroll', repositionPopover, true)
})
</script>

<template>
  <div ref="root" class="shrink-0">
    <button
      ref="trigger"
      type="button"
      data-testid="chat-settings-trigger"
      class="flex max-w-[min(17rem,calc(100vw-7rem))] cursor-pointer items-center gap-1.5 rounded-lg px-2 py-1.5 text-xs text-muted-foreground outline-none transition-colors hover:bg-muted/60 hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring disabled:pointer-events-none"
      :class="{ 'pointer-events-none opacity-50': disabled }"
      :aria-label="
        [
          `${t('chat.composer.settingsButton')}: ${modelName ?? t('chat.model.choose')}`,
          effortLabel,
        ]
          .filter(Boolean)
          .join(', ')
      "
      :title="modelName || t('chat.model.choose')"
      :aria-expanded="isOpen"
      aria-haspopup="dialog"
      :disabled="disabled"
      @click="togglePopover"
    >
      <!-- Too narrow a composer (`composer` container in Composer.vue)
           shows an icon instead of the model and effort text. -->
      <Icon
        name="lucide:sparkles"
        class="h-3.5 w-3.5 shrink-0 @lg/composer:hidden"
        :aria-hidden="true"
      />
      <span
        class="hidden max-w-[8rem] truncate @lg/composer:inline @xl/composer:max-w-[11rem] @2xl/composer:max-w-[15rem]"
        >{{ truncatedModelName }}</span
      >
      <template v-if="effortLabel">
        <span class="hidden @lg/composer:inline" aria-hidden="true">·</span>
        <span class="hidden shrink-0 @lg/composer:inline">{{
          effortLabel
        }}</span>
      </template>
      <Icon
        name="lucide:chevron-down"
        class="h-3.5 w-3.5 shrink-0"
        :aria-hidden="true"
      />
    </button>

    <ChatComposerSettingsDrawer
      v-if="compact"
      v-model:open="isOpen"
      :model-id="modelId"
      :model-groups="modelGroups"
      :effort-choices="effortChoices"
      :effort-level="effortLevel"
      :effort-state="effortState"
      :disabled="disabled"
      :model-disabled="modelDisabled"
      @update:model-id="emit('update:modelId', $event)"
      @update:effort-level="emit('update:effortLevel', $event)"
    />
    <Teleport v-else to="body">
      <div
        v-if="isOpen"
        id="chat-settings-popover"
        ref="popover"
        class="fixed z-50 max-h-[calc(100vh-2rem)] rounded-xl border border-border bg-popover p-3 text-popover-foreground shadow-lg"
        :style="popoverStyle"
        role="dialog"
        :aria-label="t('chat.composer.settingsPopover.title')"
      >
        <div class="mb-3 text-sm font-medium">
          {{ t('chat.composer.settingsPopover.title') }}
        </div>

        <label
          for="chat-model-popover"
          class="mb-1 block text-xs font-medium text-muted-foreground"
        >
          {{ t('chat.composer.settingsPopover.modelLabel') }}
        </label>
        <ShadcnSelect
          :model-value="modelId || undefined"
          :disabled="disabled || modelDisabled"
          @update:model-value="updateModel"
          @update:open="onModelSelectOpenChange"
        >
          <ShadcnSelectTrigger
            id="chat-model-popover"
            :aria-label="t('chat.composer.settingsPopover.modelLabel')"
            class="mb-4 h-9 w-full bg-background text-sm"
          >
            <ShadcnSelectValue :placeholder="t('chat.model.choose')" />
          </ShadcnSelectTrigger>
          <ShadcnSelectContent
            class="w-[min(20rem,calc(100vw-2rem))] max-h-[60vh] !overflow-y-auto"
            @close-auto-focus="onModelSelectClosed"
          >
            <div
              @keydown.capture="onModelListKeydownCapture"
              @pointermove.capture="stopReclaimingSearchFocus"
              @focusin="onModelListFocusin"
            >
              <input
                ref="modelSearchInput"
                v-model="modelQuery"
                type="text"
                data-testid="chat-model-search"
                :placeholder="
                  t('chat.composer.settingsPopover.modelSearch.placeholder')
                "
                :aria-label="
                  t('chat.composer.settingsPopover.modelSearch.placeholder')
                "
                class="mb-2 h-8 w-full rounded-md border border-input bg-background px-2 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring"
                @pointerdown.stop
                @keydown="onModelSearchKeydown"
              />
              <div
                v-if="filteredModelGroups.length === 0"
                data-testid="chat-model-search-empty"
                class="px-2 py-1.5 text-sm text-muted-foreground"
              >
                {{ t('chat.composer.settingsPopover.modelSearch.noResults') }}
              </div>
              <ShadcnSelectGroup
                v-for="group in filteredModelGroups"
                :key="group.providerId"
              >
                <ShadcnSelectLabel>{{ group.providerName }}</ShadcnSelectLabel>
                <ShadcnSelectItem
                  v-for="model in group.models"
                  :key="model.id"
                  :value="model.id"
                  :data-value="model.id"
                >
                  {{ model.name }}
                </ShadcnSelectItem>
              </ShadcnSelectGroup>
            </div>
          </ShadcnSelectContent>
        </ShadcnSelect>

        <ChatComposerProfileSelect
          id="chat-profile-popover"
          :disabled="disabled"
        />

        <ChatComposerEffortSelect
          id="effort-level-popover"
          :effort-choices="effortChoices"
          :effort-level="effortLevel"
          :effort-state="effortState"
          :disabled="disabled"
          @update:effort-level="emit('update:effortLevel', $event)"
        />
      </div>
    </Teleport>
  </div>
</template>
