<script setup lang="ts">
/**
 * The breadcrumbs above the list (spec 036, US3, FR-010, FR-020): "Alle Einträge", the ancestors
 * and the open folder as the big title of the page; in the trash, a tag view and the search the
 * name of the view. Every part but the last is a link and takes a drop of entries and folders,
 * which move there (a folder into itself is refused). A long path keeps the root and the last parts
 * and puts the middle ones into a menu. For an open entry (FR-010a) the default slot is the last
 * part, the entry's title, so a part of the path that is no target is plain text.
 */
import {
  breadcrumb,
  collapseCrumbs,
  type BreadcrumbPlace,
  type Crumb,
} from '~/lib/passwords/breadcrumb'
import { FOLDER_MIME, ITEMS_MIME, parseItemsPayload } from '~/lib/passwords/dnd'
import { displayTitle } from '~/lib/passwords/format'

const props = defineProps<{
  place: BreadcrumbPlace
}>()

defineSlots<{ default?: () => unknown }>()
const slots = useSlots()

const { t } = useI18n()
const router = useTabRouter()
const store = usePasswordsStore()
const { moveIdsAsync } = usePasswordsActions()

const nav = useTemplateRef<HTMLElement>('nav')
const { width } = useElementSize(nav)
/** Below 28 rem the row shows at most three parts (root, "…", the open folder). */
const max = computed(() => (width.value > 0 && width.value < 448 ? 3 : 5))

/** The slot's title takes one of the places on the row. */
const parts = computed(() =>
  collapseCrumbs(
    breadcrumb(store.groups, props.place),
    slots.default ? max.value - 1 : max.value,
  ),
)

type Shown = { kind: 'crumb'; crumb: Crumb } | { kind: 'more' }
const shown = computed<Shown[]>(() => {
  const [first, ...rest] = parts.value.visible
  if (!first) return []
  return [
    { kind: 'crumb', crumb: first },
    ...(parts.value.hidden.length > 0 ? [{ kind: 'more' as const }] : []),
    ...rest.map((crumb) => ({ kind: 'crumb' as const, crumb })),
  ]
})

function keyOf(crumb: Crumb): string {
  if (crumb.kind === 'root') return 'root'
  if (crumb.kind === 'folder') return crumb.id
  return `view-${crumb.view}`
}

function label(crumb: Crumb): string {
  if (crumb.kind === 'root') return t('passwords.sidebar.all')
  if (crumb.kind === 'folder')
    return displayTitle(crumb.name) ?? t('passwords.folders.unknown')
  if (crumb.view === 'trash') return t('passwords.trash.title')
  if (crumb.view === 'search') return t('passwords.breadcrumb.search')
  return crumb.name ?? ''
}

function go(crumb: Crumb) {
  if (crumb.kind === 'root') router.push('/')
  else if (crumb.kind === 'folder') router.push(`/folder/${crumb.id}`)
}

const dropping = ref<string | null>(null)

function accepts(event: DragEvent): boolean {
  const types = event.dataTransfer?.types ?? []
  return types.includes(ITEMS_MIME) || types.includes(FOLDER_MIME)
}

function onDragOver(event: DragEvent, crumb: Crumb) {
  if (!crumb.target || !accepts(event)) return
  event.preventDefault()
  dropping.value = keyOf(crumb)
}

async function onDrop(event: DragEvent, crumb: Crumb) {
  dropping.value = null
  if (!crumb.target) return
  const folder = event.dataTransfer?.getData(FOLDER_MIME)
  const ids = folder
    ? [folder]
    : parseItemsPayload(event.dataTransfer?.getData(ITEMS_MIME))
  if (ids.length === 0) return
  event.preventDefault()
  await moveIdsAsync(ids, crumb.kind === 'folder' ? crumb.id : null)
}
</script>

<template>
  <nav
    ref="nav"
    class="min-w-0"
    :aria-label="t('passwords.breadcrumb.label')"
    data-testid="passwords-breadcrumbs"
  >
    <ShadcnBreadcrumbList class="flex-nowrap items-baseline">
      <template
        v-for="(part, index) in shown"
        :key="part.kind === 'more' ? 'more' : keyOf(part.crumb)"
      >
        <ShadcnBreadcrumbSeparator v-if="index > 0" class="self-center" />
        <ShadcnBreadcrumbItem v-if="part.kind === 'more'" class="shrink-0">
          <ShadcnDropdownMenu>
            <ShadcnDropdownMenuTrigger as-child>
              <button
                type="button"
                class="flex items-center rounded hover:text-foreground"
                :aria-label="t('passwords.breadcrumb.more')"
                data-testid="passwords-crumb-more"
              >
                <ShadcnBreadcrumbEllipsis class="size-6" />
              </button>
            </ShadcnDropdownMenuTrigger>
            <ShadcnDropdownMenuContent align="start">
              <ShadcnDropdownMenuItem
                v-for="hidden in parts.hidden"
                :key="keyOf(hidden)"
                :data-testid="`passwords-crumb-${keyOf(hidden)}`"
                @select="go(hidden)"
              >
                {{ label(hidden) }}
              </ShadcnDropdownMenuItem>
            </ShadcnDropdownMenuContent>
          </ShadcnDropdownMenu>
        </ShadcnBreadcrumbItem>
        <ShadcnBreadcrumbItem
          v-else-if="part.crumb.target"
          class="max-w-48 min-w-0 shrink"
        >
          <ShadcnBreadcrumbLink
            as="button"
            type="button"
            class="flex min-w-0 items-baseline gap-1.5 rounded px-1 text-base"
            :class="dropping === keyOf(part.crumb) ? 'ring-2 ring-primary' : ''"
            :data-testid="`passwords-crumb-${keyOf(part.crumb)}`"
            @click="go(part.crumb)"
            @dragover="onDragOver($event, part.crumb)"
            @dragleave="dropping = null"
            @drop="onDrop($event, part.crumb)"
          >
            <Icon
              v-if="part.crumb.kind === 'root'"
              name="lucide:key-round"
              class="size-4 shrink-0 self-center"
            />
            <span class="truncate">{{ label(part.crumb) }}</span>
          </ShadcnBreadcrumbLink>
        </ShadcnBreadcrumbItem>
        <ShadcnBreadcrumbItem
          v-else-if="place.kind === 'entry'"
          class="min-w-0 shrink"
        >
          <span
            class="truncate px-1 text-base text-foreground"
            aria-current="page"
          >
            {{ label(part.crumb) }}
          </span>
        </ShadcnBreadcrumbItem>
        <ShadcnBreadcrumbItem v-else class="min-w-0 flex-1">
          <h1
            class="truncate px-1 text-2xl font-bold text-foreground"
            aria-current="page"
            data-testid="passwords-title"
          >
            {{ label(part.crumb) }}
          </h1>
        </ShadcnBreadcrumbItem>
      </template>
      <template v-if="slots.default">
        <ShadcnBreadcrumbSeparator class="self-center" />
        <ShadcnBreadcrumbItem class="min-w-0 flex-1">
          <slot />
        </ShadcnBreadcrumbItem>
      </template>
    </ShadcnBreadcrumbList>
  </nav>
</template>
