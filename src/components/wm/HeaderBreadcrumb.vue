<script setup lang="ts">
/**
 * The `breadcrumb` control of a window header (spec 090-app-header-bar, FR-008, research R10),
 * after the path bar of the file browser (`files/PathBar.vue`): the segments as buttons with
 * separators; a `shrink`-ed one (`width` given) shows only the open segment's name, which opens a
 * menu of the segments above it and, when `editable`, "Edit path"; `editable` also adds a pencil
 * that turns the crumbs into a text field with `editValue` (Enter sends `submit` with the typed
 * path, Escape or leaving the field keeps the folder). Segments report `select` with their id.
 * Drawn as rows of the overflow menu (`menu`), the segments are rows and `editable` adds an "Edit
 * path" row that turns into the same text field. `disabled` leaves nothing to press.
 */
import type { WmHeaderEvent } from '~/types/bindings/WmHeaderEvent'

const props = defineProps<{
  id: string
  label: string
  segments: readonly { id: string; label: string }[]
  editable?: boolean
  editValue?: string
  /** Set when the control was made narrower: only the open segment's name shows. */
  width?: number
  /** Drawn as rows of the overflow menu. */
  menu?: boolean
  disabled?: boolean
  testId?: string
}>()

const emit = defineEmits<{ event: [event: WmHeaderEvent] }>()

const { t } = useI18n()

const last = computed(() => props.segments[props.segments.length - 1])
const above = computed(() => props.segments.slice(0, -1))

const editing = ref(false)
const draft = ref('')
const field = useTemplateRef<HTMLElement>('field')

async function edit() {
  draft.value = props.editValue ?? props.segments.map((s) => s.label).join('/')
  editing.value = true
  await nextTick()
  const input = field.value?.querySelector('input')
  input?.focus()
  input?.select()
}

function commit() {
  const path = draft.value.trim()
  editing.value = false
  if (path && path !== props.editValue)
    emit('event', { id: props.id, kind: 'submit', value: path })
}

function select(segmentId: string) {
  emit('event', { id: props.id, kind: 'select', value: segmentId })
}
</script>

<template>
  <div
    v-if="menu"
    ref="field"
    role="group"
    :aria-label="label"
    :data-testid="testId"
  >
    <UiInput
      v-if="editing"
      v-model="draft"
      class="min-w-0 flex-1"
      :aria-label="label"
      :data-testid="`${testId}-input`"
      @keydown.enter.prevent="commit"
      @keydown.esc.stop.prevent="editing = false"
    />
    <template v-else>
      <button
        v-for="segment in segments"
        :key="segment.id"
        type="button"
        class="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-sm hover:bg-accent disabled:opacity-50"
        :disabled="disabled"
        :aria-current="segment.id === last?.id ? 'page' : undefined"
        @click="select(segment.id)"
      >
        <Icon
          name="lucide:folder"
          class="size-4 shrink-0"
          :aria-hidden="true"
        />
        <span class="min-w-0 truncate">{{ segment.label }}</span>
      </button>
      <button
        v-if="editable"
        type="button"
        class="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-sm hover:bg-accent disabled:opacity-50"
        :disabled="disabled"
        :data-testid="`${testId}-edit`"
        @click="edit"
      >
        <Icon
          name="lucide:pencil"
          class="size-4 shrink-0"
          :aria-hidden="true"
        />
        {{ t('wm.header.editPath') }}
      </button>
    </template>
  </div>

  <div
    v-else
    ref="field"
    class="flex min-w-0 items-center gap-1"
    :style="width === undefined ? undefined : { width: `${width}px` }"
    @pointerdown.stop
  >
    <UiInput
      v-if="editing"
      v-model="draft"
      class="min-w-0 flex-1"
      :aria-label="label"
      :data-testid="`${testId}-input`"
      @keydown.enter.prevent="commit"
      @keydown.esc.stop.prevent="editing = false"
      @blur="editing = false"
    />
    <template v-else-if="width !== undefined">
      <ShadcnDropdownMenu>
        <ShadcnDropdownMenuTrigger as-child>
          <UiButton
            variant="ghost"
            class="min-w-0 flex-1 justify-start gap-1 px-2 font-medium"
            :disabled="disabled"
            :aria-label="label"
            :data-testid="testId"
          >
            <span class="min-w-0 truncate">{{ last?.label }}</span>
            <Icon
              name="lucide:chevron-down"
              class="size-4 shrink-0"
              :aria-hidden="true"
            />
          </UiButton>
        </ShadcnDropdownMenuTrigger>
        <ShadcnDropdownMenuContent align="start" @close-auto-focus.prevent>
          <ShadcnDropdownMenuItem
            v-for="segment in above"
            :key="segment.id"
            class="gap-2"
            @select="select(segment.id)"
          >
            <Icon name="lucide:folder" class="size-4" :aria-hidden="true" />
            <span class="min-w-0 truncate">{{ segment.label }}</span>
          </ShadcnDropdownMenuItem>
          <ShadcnDropdownMenuSeparator v-if="editable && above.length > 0" />
          <ShadcnDropdownMenuItem v-if="editable" class="gap-2" @select="edit">
            <Icon name="lucide:pencil" class="size-4" :aria-hidden="true" />
            {{ t('wm.header.editPath') }}
          </ShadcnDropdownMenuItem>
        </ShadcnDropdownMenuContent>
      </ShadcnDropdownMenu>
    </template>
    <template v-else>
      <nav
        class="min-w-0 flex-1 overflow-x-auto"
        :aria-label="label"
        :data-testid="testId"
      >
        <ShadcnBreadcrumbList class="flex-nowrap">
          <template v-for="(segment, index) in segments" :key="segment.id">
            <ShadcnBreadcrumbSeparator v-if="index > 0" />
            <ShadcnBreadcrumbItem class="shrink-0">
              <ShadcnBreadcrumbLink
                as="button"
                type="button"
                class="rounded px-1 disabled:opacity-50"
                :disabled="disabled"
                :aria-current="segment.id === last?.id ? 'page' : undefined"
                :data-testid="`${testId}-${segment.id}`"
                @click="select(segment.id)"
              >
                {{ segment.label }}
              </ShadcnBreadcrumbLink>
            </ShadcnBreadcrumbItem>
          </template>
        </ShadcnBreadcrumbList>
      </nav>
      <UiButton
        v-if="editable"
        variant="ghost"
        size="icon"
        class="shrink-0"
        :disabled="disabled"
        :aria-label="t('wm.header.editPath')"
        :tooltip="t('wm.header.editPath')"
        :data-testid="`${testId}-edit`"
        @click="edit"
      >
        <Icon name="lucide:pencil" class="size-4" :aria-hidden="true" />
      </UiButton>
    </template>
  </div>
</template>
