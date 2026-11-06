import { ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { defineStore } from 'pinia'
import { onVaultTablesChanged } from '~/composables/useVaultData'
import { useMcpAgentsStore } from '~/stores/mcpAgents'
import { useWindowManagerStore } from '~/stores/windowManager'
import type { AgentProfileView } from '~/types/bindings/AgentProfileView'
import type { AgentSubjectView } from '~/types/bindings/AgentSubjectView'

/**
 * The agent profiles of the vault and the agents in them (spec 083, contracts/commands-and-actions.md):
 * the profiles with their member counts and every subject with the profile it is in, agents of
 * every device. Reloads when Rust emits `mcp-agents-changed` (a write on this device) and when the
 * three tables or the agents change by sync, so the pages and the registration dialog show what
 * the vault holds. Assigning goes through the guardrail action; no agent can call it.
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

  /** The reload a change starts by itself: nobody to show an error to, so it is logged. */
  async function reloadQuietlyAsync(): Promise<void> {
    try {
      await reloadAsync()
    } catch (error) {
      console.error('[agent-profiles] reloading after a change failed', error)
    }
  }

  /** Puts `subject` (`agent:<id>` or `model:<id>`) into `profileId`; throws what the action throws. */
  async function assignAsync(
    subject: string,
    profileId: string,
  ): Promise<void> {
    const outcome = await wm.runAction(
      'settings.agents.profiles.assign',
      { subject, profileId },
      { kind: 'user' },
    )
    if (outcome.ok) return
    throw outcome.error !== undefined
      ? outcome.error
      : new Error(outcome.message)
  }

  watch(() => mcp.changes, reloadQuietlyAsync)
  onVaultTablesChanged(
    ['haex_agent_profile*', 'haex_mcp_agents'],
    reloadQuietlyAsync,
  )

  return { profiles, subjects, loaded, reloadAsync, assignAsync }
})
