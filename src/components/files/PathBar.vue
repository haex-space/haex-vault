<script setup lang="ts">
/**
 * The path of one file browser panel (spec 044 FR-003, FR-004): up, the crumbs of the open folder
 * and reload. The pencil turns the crumbs into a text field with the whole path, as in GNOME's file
 * manager: to copy it, or to type another one (Enter goes there, Escape or leaving the field keeps
 * the folder). When the crumbs do not fit the room the bar has (a small screen, or a panel of two),
 * they give way to the open folder's name, which opens a menu of the folders above it, editing and
 * reload. In the toolbar while there is one panel, in the panel's own header with two
 * (spec 062).
 */
import { breadcrumbs } from '~/lib/files/state'

const props = defineProps<{
  path: string | null
  canGoUp: boolean
  /** The name the root shows in the path bar (a storage's name instead of `/`). */
  rootLabel?: string
}>()

const emit = defineEmits<{
  go: [path: string]
  up: []
  reload: []
}>()

const { t } = useI18n()

const crumbs = computed(() => (props.path ? breadcrumbs(props.path) : []))

/** What a crumb shows: the root as the storage's name when it has one. */
function crumbName(crumb: { name: string }, index: number): string {
  return index === 0 && props.rootLabel ? props.rootLabel : crumb.name
}
/** The open folder's name, for the small screen's one button. */
const currentName = computed(() => {
  const last = crumbs.value.length - 1
  const crumb = crumbs.value[last]
  return crumb ? crumbName(crumb, last) : ''
})

const editing = ref(false)
const draft = ref('')
const field = useTemplateRef<HTMLElement>('field')

// The crumbs collapse when they do not fit, whatever the screen: a hidden copy of them at their
// natural width is compared with the room the bar has besides its buttons (up, pencil, reload and
// the gaps between them).
const BUTTONS_WIDTH = 3 * 32 + 3 * 4
const root = useTemplateRef<HTMLElement>('root')
const ghost = useTemplateRef<HTMLElement>('ghost')
const { width: rootWidth } = useElementSize(root, undefined, {
  box: 'border-box',
})
const { width: crumbsWidth } = useElementSize(ghost, undefined, {
  box: 'border-box',
})
const compact = computed(
  () =>
    rootWidth.value > 0 && crumbsWidth.value + BUTTONS_WIDTH > rootWidth.value,
)

async function edit() {
  draft.value = props.path ?? ''
  editing.value = true
  await nextTick()
  const input = field.value?.querySelector('input')
  input?.focus()
  input?.select()
}

function commit() {
  const target = draft.value.trim()
  editing.value = false
  if (target && target !== props.path) emit('go', target)
}

async function copy() {
  await navigator.clipboard.writeText(draft.value).catch(() => {})
}
</script>

<template>
  <div ref="root" class="relative flex min-w-0 flex-1 items-center gap-1">
    <UiButton
      variant="ghost"
      size="icon"
      class="shrink-0"
      :aria-label="t('files.up')"
      :tooltip="t('files.up')"
      :disabled="!canGoUp"
      data-testid="files-up"
      @click="emit('up')"
    >
      <Icon name="lucide:arrow-up" class="size-4" />
    </UiButton>

    <div v-if="editing" ref="field" class="min-w-0 flex-1">
      <UiInput
        v-model="draft"
        :aria-label="t('files.pathBar')"
        data-testid="files-path-input"
        @keydown.enter.prevent="commit"
        @keydown.esc.stop.prevent="editing = false"
        @blur="editing = false"
      >
        <template #append>
          <ShadcnInputGroupAddon align="inline-end">
            <button
              type="button"
              class="text-muted-foreground hover:text-foreground"
              :aria-label="t('files.path.copy')"
              data-testid="files-path-copy"
              @mousedown.prevent
              @click="copy"
            >
              <Icon name="lucide:copy" class="size-4" />
            </button>
          </ShadcnInputGroupAddon>
        </template>
      </UiInput>
    </div>
    <UiButton
      v-else-if="!compact"
      variant="ghost"
      size="icon"
      class="shrink-0"
      :aria-label="t('files.path.edit')"
      :tooltip="t('files.path.edit')"
      :disabled="!path"
      data-testid="files-path-edit"
      @click="edit"
    >
      <Icon name="lucide:pencil" class="size-4" />
    </UiButton>
    <!-- On a small screen the crumbs do not fit: the folder's name stands for them, and a tap
      offers the folders above it, editing the path and reloading (they are buttons of their own on a
      wide one). -->
    <ShadcnDropdownMenu v-if="!editing && compact">
      <ShadcnDropdownMenuTrigger as-child>
        <UiButton
          variant="ghost"
          class="min-w-0 flex-1 justify-start gap-1 px-2 text-base font-medium"
          :disabled="!path"
          :aria-label="t('files.pathBar')"
          data-testid="files-path-menu"
        >
          <span class="min-w-0 truncate">{{ currentName }}</span>
          <Icon name="lucide:chevron-down" class="size-4 shrink-0" />
        </UiButton>
      </ShadcnDropdownMenuTrigger>
      <ShadcnDropdownMenuContent align="start" @close-auto-focus.prevent>
        <ShadcnDropdownMenuItem
          v-for="(crumb, index) in crumbs"
          :key="crumb.path"
          class="gap-2 py-2.5 text-base"
          :style="{ paddingLeft: `${0.5 + index * 0.75}rem` }"
          :aria-current="index === crumbs.length - 1 ? 'page' : undefined"
          :data-testid="`files-path-menu-crumb-${index}`"
          @select="emit('go', crumb.path)"
        >
          <Icon
            :name="
              index === crumbs.length - 1
                ? 'lucide:folder-open'
                : 'lucide:folder'
            "
            class="size-5 shrink-0"
          />
          <span class="min-w-0 truncate">{{ crumbName(crumb, index) }}</span>
        </ShadcnDropdownMenuItem>
        <ShadcnDropdownMenuSeparator />
        <ShadcnDropdownMenuItem
          class="gap-2 py-2.5 text-base"
          data-testid="files-path-menu-edit"
          @select="edit"
        >
          <Icon name="lucide:pencil" class="size-5" />
          {{ t('files.path.edit') }}
        </ShadcnDropdownMenuItem>
        <ShadcnDropdownMenuItem
          class="gap-2 py-2.5 text-base"
          data-testid="files-path-menu-reload"
          @select="emit('reload')"
        >
          <Icon name="lucide:refresh-cw" class="size-5" />
          {{ t('files.reload') }}
        </ShadcnDropdownMenuItem>
      </ShadcnDropdownMenuContent>
    </ShadcnDropdownMenu>
    <nav
      v-if="!editing && !compact"
      class="min-w-0 flex-1 overflow-x-auto [scrollbar-width:none] [&::-webkit-scrollbar]:hidden"
      :aria-label="t('files.pathBar')"
      data-testid="files-path-bar"
    >
      <ShadcnBreadcrumbList class="flex-nowrap">
        <template v-for="(crumb, index) in crumbs" :key="crumb.path">
          <ShadcnBreadcrumbSeparator v-if="index > 0" />
          <ShadcnBreadcrumbItem class="shrink-0">
            <ShadcnBreadcrumbLink
              as="button"
              type="button"
              class="rounded px-1"
              :aria-current="index === crumbs.length - 1 ? 'page' : undefined"
              :data-testid="`files-crumb-${index}`"
              @click="emit('go', crumb.path)"
            >
              {{ crumbName(crumb, index) }}
            </ShadcnBreadcrumbLink>
          </ShadcnBreadcrumbItem>
        </template>
      </ShadcnBreadcrumbList>
    </nav>

    <UiButton
      v-if="!compact"
      variant="ghost"
      size="icon"
      class="shrink-0"
      :aria-label="t('files.reload')"
      :tooltip="t('files.reload')"
      data-testid="files-reload"
      @click="emit('reload')"
    >
      <Icon name="lucide:refresh-cw" class="size-4" />
    </UiButton>

    <!-- The crumbs at their natural width, only to be measured. -->
    <div
      ref="ghost"
      class="pointer-events-none invisible absolute left-0 top-0 w-max"
      aria-hidden="true"
      inert
    >
      <ShadcnBreadcrumbList class="flex-nowrap">
        <template v-for="(crumb, index) in crumbs" :key="crumb.path">
          <ShadcnBreadcrumbSeparator v-if="index > 0" />
          <ShadcnBreadcrumbItem class="shrink-0">
            <span class="rounded px-1">{{ crumbName(crumb, index) }}</span>
          </ShadcnBreadcrumbItem>
        </template>
      </ShadcnBreadcrumbList>
    </div>
  </div>
</template>
