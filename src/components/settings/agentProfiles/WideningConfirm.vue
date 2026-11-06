<script setup lang="ts">
/**
 * The question before a change gives the agents of a profile more than they had (spec 083 FR-016,
 * US2 scenario 2): it names the agent profile and how many agents it applies to. For "Standard" it
 * also says that it holds every agent that registers later. Cancelling leaves the profile as it
 * was; the page puts the control back.
 */
defineProps<{
  /** The label of the profile. */
  name: string
  /** How many agents are in it. */
  count: number
  standard: boolean
}>()

const emit = defineEmits<{ confirm: []; cancel: [] }>()

const { t } = useI18n()
</script>

<template>
  <ShadcnAlertDialog
    :open="true"
    @update:open="(open: boolean) => !open && emit('cancel')"
  >
    <ShadcnAlertDialogContent
      data-testid="agent-profile-widening"
      :data-count="count"
    >
      <ShadcnAlertDialogHeader>
        <ShadcnAlertDialogTitle>
          {{ t('agentProfiles.widening.title') }}
        </ShadcnAlertDialogTitle>
        <ShadcnAlertDialogDescription>
          {{ t('agentProfiles.widening.body', { name }, count) }}
          <template v-if="standard">
            {{ t('agentProfiles.widening.standard') }}
          </template>
        </ShadcnAlertDialogDescription>
      </ShadcnAlertDialogHeader>
      <ShadcnAlertDialogFooter>
        <ShadcnAlertDialogCancel data-testid="agent-profile-widening-cancel">
          {{ t('agentProfiles.widening.cancel') }}
        </ShadcnAlertDialogCancel>
        <UiButton
          type="button"
          data-testid="agent-profile-widening-confirm"
          @click="emit('confirm')"
        >
          {{ t('agentProfiles.widening.confirm') }}
        </UiButton>
      </ShadcnAlertDialogFooter>
    </ShadcnAlertDialogContent>
  </ShadcnAlertDialog>
</template>
