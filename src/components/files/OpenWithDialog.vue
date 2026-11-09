<script setup lang="ts">
import { invoke } from '@tauri-apps/api/core'
import type { OpenApp } from '@bindings/OpenApp'

/**
 * "Öffnen mit": the apps of this device for one file, the best matches first. Picking one opens
 * the file with it.
 */
const props = defineProps<{ apps: OpenApp[] }>()
const emit = defineEmits<{ select: [id: string] }>()
const open = defineModel<boolean>('open', { required: true })

const { t } = useI18n()
const filter = ref('')
const icons = reactive<Record<string, string | null>>({})
const requested = new Set<string>()

const shown = computed(() => {
  const needle = filter.value.trim().toLowerCase()
  return needle
    ? props.apps.filter((app) => app.name.toLowerCase().includes(needle))
    : props.apps
})
const recommended = computed(() => shown.value.filter((app) => app.recommended))
const others = computed(() => shown.value.filter((app) => !app.recommended))

const MAX_ICON_REQUESTS = 4
const queue: string[] = []
let running = 0

async function loadIconAsync(id: string) {
  icons[id] = await invoke<string | null>('files_open_app_icon', { id }).catch(
    () => null,
  )
}

function pumpIcons() {
  while (running < MAX_ICON_REQUESTS) {
    const id = queue.shift()
    if (id === undefined) return
    running++
    void loadIconAsync(id).finally(() => {
      running--
      pumpIcons()
    })
  }
}

function requestIcon(id: string) {
  if (requested.has(id)) return
  requested.add(id)
  queue.push(id)
  pumpIcons()
}

watch(
  [open, () => props.apps],
  () => {
    if (open.value) for (const app of props.apps) requestIcon(app.id)
  },
  { immediate: true },
)
watch(open, (value) => {
  if (value) filter.value = ''
})

function chooseFirst() {
  const [first] = shown.value
  if (first) choose(first.id)
}

function choose(id: string) {
  open.value = false
  emit('select', id)
}
</script>

<template>
  <ShadcnAlertDialog
    :open="open"
    @update:open="(value: boolean) => (open = value)"
  >
    <ShadcnAlertDialogContent data-testid="files-open-with">
      <ShadcnAlertDialogHeader>
        <ShadcnAlertDialogTitle>{{
          t('files.actions.openWithChoose')
        }}</ShadcnAlertDialogTitle>
      </ShadcnAlertDialogHeader>
      <UiInput
        v-model="filter"
        type="search"
        autofocus
        :placeholder="t('files.openWith.filter')"
        :aria-label="t('files.openWith.filter')"
        data-testid="files-open-with-filter"
        @keydown.enter.prevent="chooseFirst"
      >
        <template #prepend>
          <ShadcnInputGroupAddon align="inline-start">
            <Icon name="lucide:search" class="size-4" />
          </ShadcnInputGroupAddon>
        </template>
      </UiInput>
      <p
        v-if="!shown.length"
        class="text-muted-foreground py-4 text-center text-sm"
      >
        {{ t('files.openWith.noMatch') }}
      </p>
      <div class="max-h-[50vh] space-y-3 overflow-y-auto">
        <section v-if="recommended.length" class="space-y-1">
          <h3 class="text-xs font-medium text-muted-foreground">
            {{ t('files.openWith.recommended') }}
          </h3>
          <button
            v-for="app in recommended"
            :key="app.id"
            type="button"
            class="hover:bg-accent flex w-full items-center gap-3 rounded-md px-3 py-2 text-left text-sm"
            data-testid="files-open-with-app"
            @click="choose(app.id)"
          >
            <img
              v-if="icons[app.id]"
              :src="icons[app.id]!"
              alt=""
              class="size-6 shrink-0 object-contain"
            />
            <Icon
              v-else
              name="lucide:app-window"
              class="text-muted-foreground size-6 shrink-0"
            />
            <span class="min-w-0 truncate">{{ app.name }}</span>
          </button>
        </section>
        <section v-if="others.length" class="space-y-1">
          <h3 class="text-xs font-medium text-muted-foreground">
            {{
              recommended.length
                ? t('files.openWith.others')
                : t('files.openWith.all')
            }}
          </h3>
          <button
            v-for="app in others"
            :key="app.id"
            type="button"
            class="hover:bg-accent flex w-full items-center gap-3 rounded-md px-3 py-2 text-left text-sm"
            data-testid="files-open-with-app"
            @click="choose(app.id)"
          >
            <img
              v-if="icons[app.id]"
              :src="icons[app.id]!"
              alt=""
              class="size-6 shrink-0 object-contain"
            />
            <Icon
              v-else
              name="lucide:app-window"
              class="text-muted-foreground size-6 shrink-0"
            />
            <span class="min-w-0 truncate">{{ app.name }}</span>
          </button>
        </section>
      </div>
      <ShadcnAlertDialogFooter>
        <ShadcnAlertDialogCancel>{{
          t('files.cancel')
        }}</ShadcnAlertDialogCancel>
      </ShadcnAlertDialogFooter>
    </ShadcnAlertDialogContent>
  </ShadcnAlertDialog>
</template>
