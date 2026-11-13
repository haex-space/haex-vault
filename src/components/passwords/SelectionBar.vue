<script setup lang="ts">
/**
 * The bar in place of the breadcrumbs while rows are selected (spec 036, US3, FR-012, replacing the
 * toolbar of spec 034): close, a "select all" box with three states, the number, and the actions
 * Bearbeiten (one entry only), Ausschneiden, Kopieren (with the copy dialog of stage 3), Verschieben,
 * the tags (entries only) and Löschen. Below 24 rem the actions go into an overflow menu. Every mass
 * action names the number it touches and asks before it runs.
 */
import { toast } from 'vue-sonner'
import { displayTitle } from '~/lib/passwords/format'
import { selectAllState } from '~/lib/passwords/selection'
import { buildTree, flattenFolders } from '~/lib/passwords/tree'

const props = defineProps<{
  /** The rows of the list as it shows them, folders first. */
  visibleIds: string[]
}>()

const emit = defineEmits<{
  delete: []
}>()

const { t } = useI18n()
const fieldLabels = useFieldLabels()
const { errString } = useErrorString()
const router = useTabRouter()
const store = usePasswordsStore()
const selection = usePasswordsSelectionStore()
const { setTagsAsync } = usePasswords()
const actions = usePasswordsActions()

type Pending =
  | { kind: 'move'; groupId: string | null }
  | { kind: 'addTag'; name: string }
  | { kind: 'removeTag'; name: string }

const pending = ref<Pending | null>(null)
const busy = ref(false)

const folders = computed(() =>
  flattenFolders(buildTree(store.loginGroups, store.loginHeaders).roots),
)

const groupIds = computed(() => new Set(store.groups.map((group) => group.id)))
const entryIds = computed(() =>
  selection.ids.filter((id) => !groupIds.value.has(id)),
)
/** Tags only apply to a selection of entries alone. */
const onlyEntries = computed(
  () => entryIds.value.length === selection.ids.length,
)

const allState = computed(() => selectAllState(selection.ids, props.visibleIds))
function toggleAll() {
  if (allState.value === 'all') selection.clear()
  else selection.selectEvery(props.visibleIds)
}

/** The tags that at least one selected entry carries, for "remove tag". */
const selectedTags = computed(() => {
  const ids = new Set(entryIds.value)
  const names = new Map<string, string>()
  for (const header of store.loginHeaders) {
    if (!ids.has(header.id)) continue
    for (const tag of header.tags) names.set(tag.id, tag.name)
  }
  return [...names.values()].sort((a, b) => a.localeCompare(b))
})

type BarAction = {
  id: string
  icon: string
  label: string
  run: () => void
}

const barActions = computed<BarAction[]>(() => {
  const list: BarAction[] = []
  const only = entryIds.value[0]
  if (selection.count === 1 && onlyEntries.value && only) {
    list.push({
      id: 'edit',
      icon: 'lucide:pencil',
      label: t('passwords.selection.edit'),
      run: () => router.push({ path: `/entry/${only}`, query: { edit: '' } }),
    })
  }
  list.push({
    id: 'cut',
    icon: 'lucide:scissors',
    label: t('passwords.selection.cut'),
    run: () => {
      actions.cut(selection.ids)
      selection.clear()
    },
  })
  list.push({
    id: 'copy',
    icon: 'lucide:copy',
    label: t('passwords.selection.copy'),
    run: () => {
      actions.copy(selection.ids)
      selection.clear()
    },
  })
  list.push({
    id: 'move',
    icon: 'lucide:folder-input',
    label: t('passwords.selection.move'),
    run: () => ask({ kind: 'move', groupId: null }),
  })
  if (onlyEntries.value) {
    list.push({
      id: 'tag',
      icon: 'lucide:tag',
      label: t('passwords.selection.addTag'),
      run: () => ask({ kind: 'addTag', name: '' }),
    })
    if (selectedTags.value.length) {
      list.push({
        id: 'untag',
        icon: 'lucide:tag',
        label: t('passwords.selection.removeTag'),
        run: () =>
          ask({ kind: 'removeTag', name: selectedTags.value[0] ?? '' }),
      })
    }
  }
  list.push({
    id: 'delete',
    icon: 'lucide:trash-2',
    label: t('passwords.selection.delete'),
    run: () => emit('delete'),
  })
  return list
})

function ask(next: Pending) {
  pending.value = next
}

async function confirmAsync() {
  const action = pending.value
  if (!action) return
  busy.value = true
  const ids = [...selection.ids]
  try {
    if (action.kind === 'move') {
      if (await actions.moveIdsAsync(ids, action.groupId)) {
        pending.value = null
        selection.clear()
      }
      return
    } else if (action.kind === 'addTag') {
      if (!action.name.trim()) return
      await setTagsAsync(entryIds.value, [action.name.trim()], [])
    } else {
      if (!action.name) return
      await setTagsAsync(entryIds.value, [], [action.name])
    }
    await store.quietReloadAsync()
    pending.value = null
    selection.clear()
  } catch (cause) {
    toast.error(errString(cause))
  } finally {
    busy.value = false
  }
}

/** The select cannot hold an empty value, so "top level" gets a placeholder value. */
const TOP_LEVEL = '__top__'
const targetOptions = computed(() => [
  { value: TOP_LEVEL, label: t('passwords.selection.topLevel') },
  ...folders.value.map((folder) => ({
    value: folder.id,
    label: `${'– '.repeat(folder.depth)}${displayTitle(folder.name) ?? t('passwords.folders.unknown')}`,
  })),
])
const tagOptions = computed(() =>
  selectedTags.value.map((name) => ({ value: name, label: name })),
)
const targetValue = computed({
  get: () =>
    pending.value?.kind === 'move'
      ? (pending.value.groupId ?? TOP_LEVEL)
      : TOP_LEVEL,
  set: (value: string | null | undefined) => {
    if (pending.value?.kind === 'move')
      pending.value.groupId = !value || value === TOP_LEVEL ? null : value
  },
})
</script>

<template>
  <div
    class="@container/bar flex min-w-0 flex-1 items-center gap-1 rounded-xl bg-muted px-1.5 py-1"
    role="toolbar"
    :aria-label="t('passwords.selection.label')"
    data-testid="passwords-selection"
  >
    <UiButton
      variant="ghost"
      size="icon-sm"
      :aria-label="t('passwords.selection.clear')"
      :tooltip="t('passwords.selection.clear')"
      data-testid="passwords-selection-clear"
      @click="selection.clear()"
    >
      <Icon name="lucide:x" class="size-4" />
    </UiButton>
    <ShadcnCheckbox
      class="mx-1.5"
      :model-value="
        allState === 'all'
          ? true
          : allState === 'some'
            ? 'indeterminate'
            : false
      "
      :aria-label="t('passwords.selection.selectAll')"
      data-testid="passwords-selection-all"
      @update:model-value="toggleAll"
    >
      <Icon
        :name="allState === 'some' ? 'lucide:minus' : 'lucide:check'"
        class="size-3.5"
      />
    </ShadcnCheckbox>
    <span
      class="mr-auto truncate text-sm font-medium"
      data-testid="passwords-selection-count"
    >
      {{
        t(
          'passwords.selection.count',
          { count: selection.count },
          selection.count,
        )
      }}
    </span>
    <div class="hidden items-center gap-0.5 @sm/bar:flex">
      <UiButton
        v-for="action in barActions"
        :key="action.id"
        variant="ghost"
        size="icon-sm"
        :aria-label="action.label"
        :tooltip="action.label"
        :data-testid="`passwords-selection-${action.id}`"
        @click="action.run"
      >
        <Icon :name="action.icon" class="size-4" />
      </UiButton>
    </div>
    <ShadcnDropdownMenu>
      <ShadcnDropdownMenuTrigger as-child>
        <UiButton
          variant="ghost"
          size="icon-sm"
          class="@sm/bar:hidden"
          :aria-label="t('passwords.selection.more')"
          data-testid="passwords-selection-more"
        >
          <Icon name="lucide:ellipsis-vertical" class="size-4" />
        </UiButton>
      </ShadcnDropdownMenuTrigger>
      <ShadcnDropdownMenuContent align="end">
        <ShadcnDropdownMenuItem
          v-for="action in barActions"
          :key="action.id"
          :data-testid="`passwords-selection-more-${action.id}`"
          @select="action.run"
        >
          <Icon :name="action.icon" class="size-4" />
          {{ action.label }}
        </ShadcnDropdownMenuItem>
      </ShadcnDropdownMenuContent>
    </ShadcnDropdownMenu>
  </div>

  <ShadcnAlertDialog
    :open="pending !== null"
    @update:open="(value: boolean) => !value && (pending = null)"
  >
    <ShadcnAlertDialogContent v-if="pending">
      <ShadcnAlertDialogHeader>
        <ShadcnAlertDialogTitle>
          {{
            t(
              `passwords.selection.confirm.${pending.kind}`,
              { count: selection.count },
              selection.count,
            )
          }}
        </ShadcnAlertDialogTitle>
        <ShadcnAlertDialogDescription as-child>
          <div class="flex flex-col gap-2">
            <UiSelect
              v-if="pending.kind === 'move'"
              v-model="targetValue"
              :options="targetOptions"
              :aria-label="t('passwords.selection.target')"
              data-testid="passwords-selection-target"
            />
            <UiInput
              v-else-if="pending.kind === 'addTag'"
              v-model="pending.name"
              :labels="fieldLabels.input.value"
              :placeholder="t('passwords.editor.tagPlaceholder')"
              data-testid="passwords-selection-tagname"
            />
            <UiSelect
              v-else
              :model-value="pending.name"
              :options="tagOptions"
              :aria-label="t('passwords.fields.tags')"
              @update:model-value="pending.name = $event ?? ''"
            />
          </div>
        </ShadcnAlertDialogDescription>
      </ShadcnAlertDialogHeader>
      <ShadcnAlertDialogFooter>
        <ShadcnAlertDialogCancel>{{
          t('passwords.cancel')
        }}</ShadcnAlertDialogCancel>
        <UiButton
          :loading="busy"
          data-testid="passwords-selection-confirm"
          @click="confirmAsync"
        >
          {{ t('passwords.selection.run') }}
        </UiButton>
      </ShadcnAlertDialogFooter>
    </ShadcnAlertDialogContent>
  </ShadcnAlertDialog>
</template>
