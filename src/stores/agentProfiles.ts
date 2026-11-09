import { computed, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { defineStore } from 'pinia'
import { onVaultTablesChanged } from '~/composables/useVaultData'
import { useMcpAgentsStore } from '~/stores/mcpAgents'
import { useWindowManagerStore } from '~/stores/windowManager'
import type { AgentProfileView } from '~/types/bindings/AgentProfileView'
import type { AgentSubjectView } from '~/types/bindings/AgentSubjectView'

/**
 * The agent profiles of the vault and the agents in them (spec 083, contracts/commands-and-actions.md):
 * the profiles with their member counts and every subject with the profile that applies to it and its
 * memberships, agents of every device. Reloads when Rust emits `mcp-agents-changed` (a write on this device) and when the
 * three tables or the agents change by sync, so the pages and the registration dialog show what
 * the vault holds. Adding to a profile and activating one go through guardrail actions; no agent
 * can call them.
 */
export const useAgentProfilesStore = defineStore('agentProfiles', () => {
  const profiles = ref<AgentProfileView[]>([])
  const subjects = ref<AgentSubjectView[]>([])
  const loaded = ref(false)
  const mcp = useMcpAgentsStore()
  const wm = useWindowManagerStore()

  /** Loads both lists; throws what the commands throw, for the page to show as a toast. */
  async function reloadAsync(): Promise<void> {
    const [nextProfiles, nextSubjects] = await Promise.all([
      invoke<AgentProfileView[]>('agent_profiles_list'),
      invoke<AgentSubjectView[]>('agent_subjects_list'),
    ])
    profiles.value = nextProfiles
    subjects.value = nextSubjects
    loaded.value = true
  }

  /**
   * The ids of the models the chat does not offer now (spec 083 FR-006b): while "Standard" is
   * active those taken out of it, else those that are not in the active profile.
   */
  const unavailableModelIds = computed(
    () =>
      new Set(
        subjects.value
          .filter(
            (subject) =>
              subject.kind === 'builtin' &&
              !subject.available &&
              subject.subject.startsWith('model:'),
          )
          .map((subject) => subject.subject.slice('model:'.length)),
      ),
  )

  /** The reload a change starts by itself: nobody to show an error to, so it is logged. */
  async function reloadQuietlyAsync(): Promise<void> {
    try {
      await reloadAsync()
    } catch (error) {
      console.error('[agent-profiles] reloading after a change failed', error)
    }
  }

  async function runUserAction(
    id: string,
    input: Record<string, unknown>,
  ): Promise<void> {
    const outcome = await wm.runAction(id, input, { kind: 'user' })
    if (outcome.ok) return
    throw outcome.error !== undefined
      ? outcome.error
      : new Error(outcome.message)
  }

  /**
   * Adds `subject` (`agent:<id>` or `model:<id>`) to `profileId` or removes it from it; its other
   * agent profiles stay. Throws what the action throws.
   */
  function setMemberAsync(
    subject: string,
    profileId: string,
    member: boolean,
  ): Promise<void> {
    return runUserAction('settings.agents.profiles.assign', {
      subject,
      profileId,
      member,
    })
  }

  /** Makes `profileId` the active agent profile of the vault; throws what the action throws. */
  function activateAsync(profileId: string): Promise<void> {
    return runUserAction('settings.agents.profiles.activate', { profileId })
  }

  watch(() => mcp.changes, reloadQuietlyAsync)
  onVaultTablesChanged(
    ['haex_agent_profile*', 'haex_mcp_agents', 'preferences'],
    reloadQuietlyAsync,
  )

  return {
    profiles,
    subjects,
    loaded,
    unavailableModelIds,
    reloadAsync,
    reloadQuietlyAsync,
    setMemberAsync,
    activateAsync,
  }
})
