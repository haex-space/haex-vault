<script setup lang="ts">
/**
 * Merging one tag into another (spec 100, US2): the user picks the target among the other tags and
 * sees the merge as a picture: both tags that become one on the left (the old tag and the one it
 * goes into, each with how many entries it has per app), an arrow, and on the right the name of the
 * result. The name field starts with the name of the target and can be changed, so the merged tag
 * may get a new name in the same step. What the merge means for both apps and that it cannot be undone is behind
 * the info button, not in the way. When an agent profile's password rights name the source or the
 * target tag, a line says so and stays visible (FR-010): rights go by tag name and are not
 * adjusted. A wrong name shows under its field, any other failure is a toast; a success is the
 * changed list.
 */
import { toast } from 'vue-sonner'
import type { MergedTag } from '~/lib/passwords/remote'
import { tagUsageOf } from '~/lib/passwords/scope'

const props = defineProps<{ source: MergedTag }>()
const open = defineModel<boolean>('open', { required: true })
const emit = defineEmits<{ merged: [targetId: string] }>()

const { t } = useI18n()
const { errString } = useErrorString()
const store = usePasswordsStore()
const { mergeTagsAsync, tagAgentProfilesAsync } = usePasswords()

const targetId = ref<string | null>(null)
const name = ref('')
const nameError = ref<string | null>(null)
const profiles = ref<string[]>([])
const profilesLoading = ref(false)
const profilesFailed = ref(false)
const busy = ref(false)
let profileLookupGeneration = 0

const target = computed(
  () => store.displayTags.find((tag) => tag.id === targetId.value) ?? null,
)
const sourceUsage = computed(() => tagUsageOf(props.source.ids, store.headers))

// Every opening starts without a target.
watch(open, (isOpen) => {
  if (!isOpen) return
  targetId.value = null
  profiles.value = []
  profilesFailed.value = false
})

// The name of the result starts as the name of the target, and a refusal of a name goes away once
// the name changes.
watch(target, (chosen) => {
  name.value = chosen?.name ?? ''
  nameError.value = null
})
watch(name, () => {
  nameError.value = null
})

// The profiles that name either tag, asked once a target is chosen. A failed question is a toast
// and leaves the merge possible without the line.
watch(targetId, async (id) => {
  const generation = ++profileLookupGeneration
  profiles.value = []
  profilesFailed.value = false
  profilesLoading.value = Boolean(id)
  if (!id) return
  try {
    const [ofSource, ofTarget] = await Promise.all([
      tagAgentProfilesAsync(props.source.id),
      tagAgentProfilesAsync(id),
    ])
    if (generation === profileLookupGeneration && targetId.value === id)
      profiles.value = [...new Set([...ofSource, ...ofTarget])].sort()
  } catch (cause) {
    if (generation === profileLookupGeneration && targetId.value === id) {
      profilesFailed.value = true
      toast.error(errString(cause))
    }
  } finally {
    if (generation === profileLookupGeneration) profilesLoading.value = false
  }
})

async function confirmAsync() {
  const chosen = target.value
  if (!chosen || busy.value || profilesLoading.value || profilesFailed.value)
    return
  busy.value = true
  try {
    await mergeTagsAsync(props.source.ids, chosen.ids, name.value)
    await store.quietReloadAsync()
    open.value = false
    emit('merged', chosen.id)
  } catch (cause) {
    // A refused name belongs to the name field; any other failure is a toast.
    const reason = (cause as { reason?: string }).reason
    if (reason === 'exists' || reason === 'empty' || reason === 'too_long')
      nameError.value = t(`passwords.tags.errors.${reason}`)
    else toast.error(errString(cause))
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <ShadcnAlertDialog
    :open="open"
    @update:open="(value: boolean) => (open = value)"
  >
    <ShadcnAlertDialogContent>
      <ShadcnAlertDialogHeader>
        <div class="flex items-center justify-between gap-2">
          <ShadcnAlertDialogTitle>{{
            t('passwords.tags.mergeTitle')
          }}</ShadcnAlertDialogTitle>
          <ShadcnPopover>
            <ShadcnPopoverTrigger as-child>
              <UiButtonInfo
                :label="t('passwords.tags.mergeInfo')"
                size="icon-sm"
                class="text-muted-foreground hover:bg-background hover:text-foreground focus-visible:text-foreground"
                data-testid="passwords-tag-merge-info"
              />
            </ShadcnPopoverTrigger>
            <ShadcnPopoverContent
              align="end"
              class="w-80 max-w-[calc(100vw-2rem)] text-sm"
              data-testid="passwords-tag-merge-info-text"
            >
              {{
                t('passwords.tags.mergeBody', {
                  source: source.name,
                  name: name.trim() || target?.name || '',
                  logins: sourceUsage.login,
                  passes: sourceUsage.pass,
                })
              }}
            </ShadcnPopoverContent>
          </ShadcnPopover>
        </div>
        <ShadcnAlertDialogDescription class="sr-only">{{
          t('passwords.tags.mergeDescription')
        }}</ShadcnAlertDialogDescription>
      </ShadcnAlertDialogHeader>
      <div
        class="grid gap-3 text-sm"
        :class="
          target
            ? 'sm:grid-cols-[minmax(0,1fr)_auto_minmax(0,1fr)] sm:items-center'
            : 'grid-cols-1'
        "
        data-testid="passwords-tag-merge-flow"
      >
        <!-- Both tags that become one, one above the other: the old one, and the one it goes into. -->
        <div class="flex min-w-0 flex-col gap-1.5">
          <div
            class="flex h-9 min-w-0 items-center gap-2 rounded-md border bg-muted/40 px-3"
            data-testid="passwords-tag-merge-from"
          >
            <span
              class="size-2.5 shrink-0 rounded-full border border-foreground/20"
              :style="
                source.color ? { backgroundColor: source.color } : undefined
              "
              aria-hidden="true"
            />
            <span class="min-w-0 flex-1 truncate font-medium">{{
              source.name
            }}</span>
            <PasswordsTagUsage :usage="sourceUsage" />
          </div>
          <Icon
            name="lucide:plus"
            class="size-4 shrink-0 self-center text-muted-foreground"
            aria-hidden="true"
          />
          <PasswordsTagPicker
            v-model="targetId"
            :tags="store.displayTags"
            :exclude="source.id"
            :placeholder="t('passwords.tags.mergeTarget')"
            :aria-label="t('passwords.tags.mergeTarget')"
            show-usage
            data-testid="passwords-tag-merge-target"
          />
        </div>
        <template v-if="target">
          <Icon
            name="lucide:arrow-right"
            class="size-4 shrink-0 self-center justify-self-center text-muted-foreground max-sm:rotate-90"
            aria-hidden="true"
          />
          <div class="min-w-0" data-testid="passwords-tag-merge-body">
            <UiInput
              v-model="name"
              :aria-label="t('passwords.tags.mergeName')"
              data-testid="passwords-tag-merge-name"
              @keydown.enter.prevent="confirmAsync"
            />
            <p
              v-if="nameError"
              class="mt-1 text-sm text-destructive"
              role="alert"
            >
              {{ nameError }}
            </p>
          </div>
        </template>
      </div>
      <p v-if="target && profiles.length > 0" class="text-sm text-warning">
        {{
          t('passwords.tags.mergeAgents', {
            source: source.name,
            target: target.name,
            profiles: profiles.join(', '),
          })
        }}
      </p>
      <p
        v-else-if="target && profilesLoading"
        class="text-sm text-muted-foreground"
        role="status"
      >
        {{ t('passwords.tags.mergeAgentsLoading') }}
      </p>
      <p
        v-else-if="target && profilesFailed"
        class="text-sm text-destructive"
        role="alert"
      >
        {{ t('passwords.tags.mergeAgentsUnavailable') }}
      </p>
      <ShadcnAlertDialogFooter>
        <ShadcnAlertDialogCancel>{{
          t('passwords.cancel')
        }}</ShadcnAlertDialogCancel>
        <UiButton
          variant="destructive"
          :disabled="!target || busy || profilesLoading || profilesFailed"
          data-testid="passwords-tag-merge-confirm"
          @click="confirmAsync"
        >
          {{ t('passwords.tags.mergeConfirm') }}
        </UiButton>
      </ShadcnAlertDialogFooter>
    </ShadcnAlertDialogContent>
  </ShadcnAlertDialog>
</template>
