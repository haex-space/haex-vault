<script setup lang="ts">
/**
 * The question before a change gives the agents of a profile more than they had (spec 083 FR-016,
 * US2 scenario 2): it names the agent profile and how many agents it applies to. For "Standard" it
 * also says that it holds every agent that registers later. Cancelling leaves the profile as it
 * was; the page puts the control back. The same question is asked before a profile becomes the
 * active one and gives members more (`activation`), then `count` is the agents that get more.
 */
defineProps<{
  /** The label of the profile. */
  name: string
  /** How many agents are in it, or get more when it becomes active. */
  count: number
  standard: boolean
  activation?: boolean
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
          {{
            activation
              ? t('agentProfiles.activation.title')
              : t('agentProfiles.widening.title')
          }}
        </ShadcnAlertDialogTitle>
        <ShadcnAlertDialogDescription v-if="activation">
          {{ t('agentProfiles.activation.body', { name }, count) }}
        </ShadcnAlertDialogDescription>
        <ShadcnAlertDialogDescription v-else>
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
          {{
            activation
              ? t('agentProfiles.activation.confirm')
              : t('agentProfiles.widening.confirm')
          }}
        </UiButton>
      </ShadcnAlertDialogFooter>
    </ShadcnAlertDialogContent>
  </ShadcnAlertDialog>
</template>
