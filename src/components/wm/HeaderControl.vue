<script setup lang="ts">
/**
 * One control of a window header, drawn from its description (spec 090-app-header-bar, FR-008,
 * FR-023, research R10). `form` says where it is drawn: `row` (in the header row), `icon` (the
 * icon alone, a collapse form) or `menu` (a row of the overflow menu); `width` is the size of a
 * `shrink` control that was made narrower. Every text is text (`{{ }}`, never HTML); a `lucide:`
 * icon is an Iconify icon, an extension's picture comes from the `icons` table. It only draws and
 * reports what the user did as a `WmHeaderEvent`; the app decides what that means.
 *
 * The test id is the description's `testId` (system apps) or `wm-header-<id>`. Every interactive
 * element stops `pointerdown`, so using a control never starts dragging the window.
 */
import { Search } from '@lucide/vue'
import type { HeaderSearchField } from '~/composables/useHeaderSearches'
import type { WmHeaderControl } from '~/types/bindings/WmHeaderControl'
import type { WmHeaderEvent } from '~/types/bindings/WmHeaderEvent'

const props = defineProps<{
  control: WmHeaderControl
  form: 'row' | 'icon' | 'menu'
  width?: number
  /** Drawn only to be measured (the header's hidden row): no test id, so a test finds one control. */
  ghost?: boolean
  icons?: Record<string, string>
  /** The field of a `search` control (kept by the header across descriptions). */
  search?: HeaderSearchField
}>()

const emit = defineEmits<{ event: [event: WmHeaderEvent] }>()

const fieldLabels = useFieldLabels()
const { t } = useI18n()

const inMenu = computed(() => props.form === 'menu')
const testId = computed(() =>
  props.ghost
    ? undefined
    : (props.control.testId ?? `wm-header-${props.control.id}`),
)
const label = computed(() => props.control.label ?? '')
const tooltip = computed(() => props.control.tooltip ?? label.value)
/** The visible text of a button, toggle or menu: none in the icon form. */
const text = computed(() => {
  const c = props.control
  const own =
    c.type === 'button' || c.type === 'toggle' || c.type === 'menu'
      ? c.text
      : undefined
  return props.form === 'icon' ? undefined : own
})
const widthStyle = computed(() =>
  props.width === undefined ? undefined : { width: `${props.width}px` },
)

/** The menu row a button, toggle or option takes in the overflow menu. */
const MENU_ROW =
  'flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-sm hover:bg-accent disabled:opacity-50'

/**
 * Closing a menu gives the focus back to its button. When the entry the user chose has already put
 * it somewhere else (an editor that opened, a dialog), that must stay: only a focus that is nowhere
 * (the page itself, or the closing menu) goes back to the button.
 */
function keepFocus(event: Event) {
  const active = document.activeElement
  if (active && active !== document.body && !active.closest('[role="menu"]'))
    event.preventDefault()
}

function send(event: WmHeaderEvent) {
  emit('event', event)
}

/** The group reports an empty value when the selected option is clicked again; a choice keeps one. */
function pick(id: string, value: unknown) {
  if (value) send({ id, kind: 'select', value: String(value) })
}

function onSearchKey(event: KeyboardEvent) {
  const field = props.search
  if (!field) return
  if (event.key === 'Enter') {
    event.preventDefault()
    field.enter()
  } else if (event.key === 'Escape') {
    // First Escape empties the field, the next one leaves it (FR-012).
    event.preventDefault()
    event.stopPropagation()
    if (field.text.value) field.type('')
    else (event.target as HTMLElement | null)?.blur()
  }
}
</script>

<template>
  <!-- button -->
  <template v-if="control.type === 'button'">
    <button
      v-if="inMenu"
      type="button"
      :class="MENU_ROW"
      :disabled="control.disabled"
      :data-testid="testId"
      @click="send({ id: control.id, kind: 'click' })"
    >
      <WmHeaderIcon
        :name="control.icon"
        :icons="icons"
        :monochrome="control.monochrome"
      />
      {{ control.text ?? label }}
    </button>
    <UiButton
      v-else
      variant="ghost"
      :size="text ? 'default' : 'icon'"
      class="shrink-0"
      :disabled="control.disabled"
      :aria-label="label"
      :tooltip="tooltip"
      :data-testid="testId"
      @pointerdown.stop
      @click="send({ id: control.id, kind: 'click' })"
    >
      <WmHeaderIcon
        :name="control.icon"
        :icons="icons"
        :monochrome="control.monochrome"
      />
      <span v-if="text">{{ text }}</span>
    </UiButton>
  </template>

  <!-- toggle -->
  <template v-else-if="control.type === 'toggle'">
    <button
      v-if="inMenu"
      type="button"
      :class="MENU_ROW"
      role="switch"
      :aria-checked="control.pressed"
      :disabled="control.disabled"
      :data-testid="testId"
      @click="send({ id: control.id, kind: 'toggle', value: !control.pressed })"
    >
      <WmHeaderIcon
        :name="control.icon"
        :icons="icons"
        :monochrome="control.monochrome"
      />
      <span class="min-w-0 flex-1 truncate">{{ control.text ?? label }}</span>
      <Icon
        v-if="control.pressed"
        name="lucide:check"
        class="size-4"
        :aria-hidden="true"
      />
    </button>
    <UiButton
      v-else
      variant="ghost"
      :size="text ? 'default' : 'icon'"
      class="shrink-0"
      :disabled="control.disabled"
      :aria-label="label"
      :aria-pressed="control.pressed"
      :tooltip="tooltip"
      :data-testid="testId"
      @pointerdown.stop
      @click="send({ id: control.id, kind: 'toggle', value: !control.pressed })"
    >
      <WmHeaderIcon
        :name="control.icon"
        :icons="icons"
        :monochrome="control.monochrome"
      />
      <span v-if="text">{{ text }}</span>
    </UiButton>
  </template>

  <!-- search -->
  <div
    v-else-if="control.type === 'search' && search"
    :class="['min-w-0', inMenu ? 'w-full' : width === undefined ? 'w-72' : '']"
    :style="inMenu ? undefined : widthStyle"
    @pointerdown.stop
  >
    <UiInput
      :model-value="search.text.value"
      type="search"
      :placeholder="control.placeholder"
      :aria-label="label"
      :labels="fieldLabels.input.value"
      :prepend-icon="Search"
      :disabled="control.disabled"
      clearable
      class="[&_input::-webkit-search-cancel-button]:hidden"
      :data-testid="testId"
      @update:model-value="search.type(String($event ?? ''))"
      @focus="search.focus()"
      @blur="search.blur()"
      @keydown="onSearchKey"
    />
  </div>

  <!-- menu -->
  <template v-else-if="control.type === 'menu'">
    <div v-if="inMenu" role="group" :aria-label="label" :data-testid="testId">
      <p
        class="flex items-center gap-1.5 px-2 pt-1 text-xs text-muted-foreground"
      >
        {{ control.text ?? label }}
        <WmHeaderDot v-if="control.active" :label="t('wm.header.active')" />
      </p>
      <template
        v-for="item in control.items"
        :key="item.type === 'item' ? item.id : undefined"
      >
        <button
          v-if="item.type === 'item'"
          type="button"
          :class="MENU_ROW"
          :role="item.checked === undefined ? undefined : 'checkbox'"
          :aria-checked="item.checked"
          :disabled="control.disabled || item.disabled"
          :data-testid="testId ? `${testId}-${item.id}` : undefined"
          @click="send({ id: control.id, kind: 'click', value: item.id })"
        >
          <WmHeaderIcon
            :name="item.icon"
            :icons="icons"
            :monochrome="control.monochrome"
          />
          <span class="min-w-0 flex-1 truncate">{{ item.label }}</span>
          <Icon
            v-if="item.checked"
            name="lucide:check"
            class="size-4"
            :aria-hidden="true"
          />
        </button>
        <hr v-else class="my-1 border-border" />
      </template>
    </div>
    <ShadcnDropdownMenu v-else>
      <ShadcnDropdownMenuTrigger as-child>
        <UiButton
          variant="ghost"
          :size="text ? 'default' : 'icon'"
          class="relative shrink-0"
          :disabled="control.disabled"
          :aria-label="
            control.active ? `${label} (${t('wm.header.active')})` : label
          "
          :tooltip="tooltip"
          :data-testid="testId"
          @pointerdown.stop
        >
          <WmHeaderIcon
            :name="control.icon"
            :icons="icons"
            :monochrome="control.monochrome"
          />
          <span v-if="text">{{ text }}</span>
          <WmHeaderDot v-if="control.active" corner />
        </UiButton>
      </ShadcnDropdownMenuTrigger>
      <ShadcnDropdownMenuContent
        align="end"
        class="max-h-80 overflow-y-auto"
        @close-auto-focus="keepFocus"
      >
        <template
          v-for="item in control.items"
          :key="item.type === 'item' ? item.id : undefined"
        >
          <ShadcnDropdownMenuItem
            v-if="item.type === 'item'"
            class="gap-2"
            :role="item.checked === undefined ? undefined : 'menuitemcheckbox'"
            :aria-checked="item.checked"
            :disabled="item.disabled"
            :data-testid="`${testId}-${item.id}`"
            @select="send({ id: control.id, kind: 'click', value: item.id })"
          >
            <WmHeaderIcon
              :name="item.icon"
              :icons="icons"
              :monochrome="control.monochrome"
            />
            <span class="min-w-0 flex-1 truncate">{{ item.label }}</span>
            <Icon
              v-if="item.checked"
              name="lucide:check"
              class="size-4"
              :aria-hidden="true"
            />
          </ShadcnDropdownMenuItem>
          <ShadcnDropdownMenuSeparator v-else />
        </template>
      </ShadcnDropdownMenuContent>
    </ShadcnDropdownMenu>
  </template>

  <!-- choice -->
  <div
    v-else-if="control.type === 'choice' && inMenu"
    role="group"
    :aria-label="label"
    class="flex flex-col"
    :data-testid="testId"
  >
    <button
      v-for="option in control.options"
      :key="option.id"
      type="button"
      :class="MENU_ROW"
      :aria-pressed="option.id === control.selected"
      :disabled="control.disabled"
      :data-testid="`${testId}-${option.id}`"
      @click="send({ id: control.id, kind: 'select', value: option.id })"
    >
      <WmHeaderIcon
        :name="option.icon"
        :icons="icons"
        :monochrome="control.monochrome"
      />
      <span class="min-w-0 flex-1 truncate">{{ option.label }}</span>
      <Icon
        v-if="option.id === control.selected"
        name="lucide:check"
        class="size-4"
        :aria-hidden="true"
      />
    </button>
  </div>
  <ShadcnToggleGroup
    v-else-if="control.type === 'choice'"
    type="single"
    size="sm"
    :model-value="control.selected"
    :aria-label="label"
    :disabled="control.disabled"
    class="shrink-0 gap-0 rounded-md border border-border"
    :data-testid="testId"
    @pointerdown.stop
    @update:model-value="pick(control.id, $event)"
  >
    <ShadcnToggleGroupItem
      v-for="option in control.options"
      :key="option.id"
      :value="option.id"
      :aria-label="option.label"
      :title="option.label"
      :data-testid="`${testId}-${option.id}`"
    >
      <WmHeaderIcon
        :name="option.icon"
        :icons="icons"
        :monochrome="control.monochrome"
      />
      <span v-if="!option.icon" class="min-w-0 truncate">{{
        option.label
      }}</span>
    </ShadcnToggleGroupItem>
  </ShadcnToggleGroup>

  <!-- breadcrumb -->
  <WmHeaderBreadcrumb
    v-else-if="control.type === 'breadcrumb'"
    :id="control.id"
    :label="label"
    :segments="control.segments"
    :editable="control.editable"
    :edit-value="control.editValue"
    :width="width"
    :menu="inMenu"
    :disabled="control.disabled"
    :ghost="ghost"
    :test-id="testId"
    @event="send"
  />

  <!-- title -->
  <span
    v-else-if="control.type === 'title'"
    class="min-w-0 truncate px-1 text-sm font-medium"
    :style="widthStyle"
    :data-testid="testId"
  >
    {{ control.text }}
  </span>

  <!-- separator -->
  <template v-else-if="control.type === 'separator'">
    <hr v-if="inMenu" class="my-1 border-border" />
    <span
      v-else
      class="mx-1 h-5 w-px shrink-0 bg-border"
      role="separator"
      aria-orientation="vertical"
    />
  </template>
</template>
