<script setup lang="ts">
/**
 * "Belongs to: …" at the top of a pass (spec 092, FR-014, FR-015, FR-013): the entry the pass is
 * linked to, as a link to it; "(in trash)" while that entry is in the trash; "(not available)" for
 * one that is not in the vault (it may still arrive by sync); and a notice for a loop that two
 * devices made by linking in opposite directions. The link can be cleared from each of the last two
 * states and from an open one. The state is read from the headers the window holds, so a change by
 * sync shows at once.
 */
import { toast } from 'vue-sonner'
import type { ItemDetail } from '@bindings/ItemDetail'
import { displayTitle } from '~/lib/passwords/format'
import { parentState } from '~/lib/passwords/links'
import { trashGroupIds } from '~/lib/passwords/tree'

const props = defineProps<{
  detail: ItemDetail
  /** The pass is in the trash: it can be looked at, not changed. */
  readonly?: boolean
}>()
const emit = defineEmits<{ changed: [] }>()

const { t } = useI18n()
const { errString } = useErrorString()
const router = useTabRouter()
const store = usePasswordsStore()
const { setParentAsync } = usePasses()

const state = computed(() =>
  parentState(
    // The store's header carries the link as of now; the loaded detail may be older.
    store.headersById.get(props.detail.id) ?? props.detail,
    store.headers,
    trashGroupIds(store.groups),
  ),
)
const parentTitle = computed(() =>
  state.value.state === 'open' || state.value.state === 'in_trash'
    ? (displayTitle(state.value.parent.title) ?? t('passwords.untitled'))
    : '',
)
const clearing = ref(false)

async function clearAsync() {
  if (!props.detail.updatedAt) return
  clearing.value = true
  try {
    await setParentAsync(props.detail.id, props.detail.updatedAt, null)
    await store.quietReloadAsync()
    emit('changed')
  } catch (cause) {
    toast.error(errString(cause))
  } finally {
    clearing.value = false
  }
}

function openParent() {
  if (state.value.state === 'open' || state.value.state === 'in_trash')
    router.push(`/entry/${state.value.parent.id}`)
}
</script>

<template>
  <div
    v-if="state.state !== 'none'"
    class="flex flex-wrap items-center gap-2 rounded-xl bg-muted px-4 py-3 text-sm"
    :role="state.state === 'loop' ? 'alert' : 'status'"
    data-testid="passwords-pass-belongs-to"
    :data-state="state.state"
  >
    <Icon
      :name="state.state === 'loop' ? 'lucide:triangle-alert' : 'lucide:link'"
      class="size-4 shrink-0"
    />
    <span v-if="state.state === 'loop'" class="min-w-0 flex-1">
      {{ t('passwords.pass.links.loop') }}
    </span>
    <span v-else class="min-w-0 flex-1">
      {{ t('passwords.pass.links.belongsTo') }}
      <button
        v-if="state.state === 'open' || state.state === 'in_trash'"
        type="button"
        class="font-medium underline-offset-2 hover:underline"
        data-testid="passwords-pass-parent-link"
        @click="openParent"
      >
        {{ parentTitle }}
      </button>
      <span
        v-else
        class="text-muted-foreground"
        data-testid="passwords-pass-parent-missing"
        >{{ t('passwords.pass.links.notAvailable') }}</span
      >
      <span
        v-if="state.state === 'in_trash'"
        class="text-muted-foreground"
        data-testid="passwords-pass-parent-in-trash"
      >
        {{ t('passwords.pass.links.inTrash') }}
      </span>
    </span>
    <UiButton
      v-if="!readonly && state.state !== 'open' && state.state !== 'in_trash'"
      size="sm"
      variant="outline"
      :loading="clearing"
      data-testid="passwords-pass-clear-link"
      @click="clearAsync"
    >
      {{ t('passwords.pass.links.clear') }}
    </UiButton>
  </div>
</template>
