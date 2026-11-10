<script setup lang="ts">
/**
 * Every workspace with its number and window count, switch/create/delete,
 * and — for the current workspace's windows — a "move to" menu (spec
 * 015-workspace-shell, T040, FR-019/FR-020/FR-021/FR-022, plan research
 * R14). Deleting a workspace with open windows always confirms (FR-021),
 * even when none of them has a running reply — a plain window-count
 * reason alongside any real guard results, via the same
 * `wm/CloseConfirm.vue` dialog T038 built. The delete button grows to a
 * full touch target in compact mode (T050).
 */
import { computed } from 'vue'
import type { WmWindow } from '~/lib/wm/types'

const open = defineModel<boolean>('open', { default: false })

const wm = useWindowManagerStore()
const { t } = useI18n()

const rows = computed(() =>
  wm.workspaces.map((workspace) => ({
    workspace,
    windowCount: wm.windows.filter((w) => w.workspaceId === workspace.id)
      .length,
    hasAttention: wm.workspaceHasAttention(workspace.id),
  })),
)

const currentWorkspaceWindows = computed(() =>
  wm.windows.filter((w) => w.workspaceId === wm.activeWorkspaceId),
)

const otherWorkspaces = computed(() =>
  wm.workspaces.filter((w) => w.id !== wm.activeWorkspaceId),
)

/** Formats the zero-based storage position as a one-based workspace label. */
function numberLabel(position: number): string {
  return t('wm.workspaces.numbered', { number: position + 1 })
}

/** Resolves a window's title override before its translated app title. */
function windowTitle(window: WmWindow): string {
  const info = wm.windowDisplayInfo(window)
  if (!info) return ''
  return (
    info.titleOverride ??
    (info.titleKey ? t(info.titleKey, info.titleParams) : '')
  )
}

// Spec 020 FR-024: switching, creating, deleting and moving run catalog actions; deleting keeps
// the FR-021 confirmation (`requestDeleteWorkspace`, useWmTab.ts).
const switchWorkspace = useAction('wm.workspace.switch')
const createWorkspace = useAction('wm.workspace.create')
const deleteWorkspace = useAction('wm.workspace.delete')
const moveWindow = useAction('wm.window.moveToWorkspace')

/** Activates the selected workspace while leaving the overview open. */
function select(workspaceId: string) {
  void switchWorkspace({ workspaceId })
}

/** Creates a workspace (backend-assigned id) and switches to it. */
function create() {
  void createWorkspace()
}

/** Confirms open windows and running work before removing a workspace. */
function remove(workspaceId: string) {
  void deleteWorkspace({ workspaceId })
}
</script>

<template>
  <UiDrawerModal
    :open="open"
    :title="t('wm.workspaces.title')"
    @update:open="open = $event"
  >
    <template #content>
      <div class="flex flex-col gap-2 overflow-x-hidden p-4">
        <div
          v-for="row in rows"
          :key="row.workspace.id"
          class="flex items-center gap-2 rounded-lg border p-3"
          :class="
            row.workspace.id === wm.activeWorkspaceId
              ? 'border-foreground/40'
              : 'border-border'
          "
        >
          <button
            type="button"
            class="flex min-w-0 flex-1 items-center gap-2 text-left max-md:min-h-12 max-md:gap-3"
            @click="select(row.workspace.id)"
          >
            <span class="shrink-0 truncate font-medium">{{
              numberLabel(row.workspace.position)
            }}</span>
            <span class="min-w-0 truncate text-xs text-muted-foreground">{{
              t('wm.workspaces.windowCount', row.windowCount)
            }}</span>
            <span
              v-if="row.hasAttention"
              class="h-1.5 w-1.5 shrink-0 rounded-full bg-warning max-md:h-2.5 max-md:w-2.5"
              :aria-label="t('wm.attention')"
            />
          </button>
          <button
            v-if="wm.workspaces.length > 1"
            type="button"
            class="shrink-0 rounded text-muted-foreground hover:bg-accent hover:text-foreground"
            :class="wm.compact ? 'p-3.5' : 'p-1.5'"
            :aria-label="t('wm.workspaces.delete')"
            @click="remove(row.workspace.id)"
          >
            <Icon
              name="lucide:trash-2"
              class="h-4 w-4 max-md:h-5 max-md:w-5"
              :aria-hidden="true"
            />
          </button>
        </div>

        <UiButton variant="outline" class="justify-start gap-2" @click="create">
          <Icon
            name="lucide:plus"
            class="h-4 w-4 max-md:h-5 max-md:w-5"
            :aria-hidden="true"
          />
          {{ t('wm.workspaces.create') }}
        </UiButton>

        <template
          v-if="
            currentWorkspaceWindows.length > 0 && otherWorkspaces.length > 0
          "
        >
          <hr class="my-1 border-border" />
          <p class="px-1 text-xs font-medium text-muted-foreground">
            {{ t('wm.workspaces.moveWindow') }}
          </p>
          <div
            v-for="win in currentWorkspaceWindows"
            :key="win.id"
            class="flex items-center gap-2 px-1"
          >
            <span class="min-w-0 flex-1 truncate text-sm">{{
              windowTitle(win)
            }}</span>
            <ShadcnDropdownMenu>
              <ShadcnDropdownMenuTrigger as-child>
                <UiButton size="sm" variant="ghost">{{
                  t('wm.workspaces.moveTo')
                }}</UiButton>
              </ShadcnDropdownMenuTrigger>
              <ShadcnDropdownMenuContent align="end">
                <ShadcnDropdownMenuItem
                  v-for="target in otherWorkspaces"
                  :key="target.id"
                  @select="
                    moveWindow({ windowId: win.id, toWorkspaceId: target.id })
                  "
                >
                  {{ numberLabel(target.position) }}
                </ShadcnDropdownMenuItem>
              </ShadcnDropdownMenuContent>
            </ShadcnDropdownMenu>
          </div>
        </template>
      </div>
    </template>
  </UiDrawerModal>
</template>
