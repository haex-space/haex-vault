import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { toast } from 'vue-sonner'
import { widens } from '~/lib/agents/profiles'
import type { AgentProfileDetail } from '~/types/bindings/AgentProfileDetail'
import type { Rights } from '~/types/bindings/Rights'

/**
 * Making an agent profile the active one (spec 083 FR-006a): the profile's rights then apply to the
 * agents that are members of it. When that gives any of them more than they have now, the change
 * waits for the user's answer and names how many agents that is. Used by the list of profiles and
 * the page of a profile; the change itself is the guardrail action
 * `settings.agents.profiles.activate`, which no agent can call.
 */
export function useAgentProfileActivation() {
  const { errString } = useErrorString()
  const store = useAgentProfilesStore()
  const pending = ref<{ profileId: string; count: number } | null>(null)
  let requestSequence = 0

  async function rightsOfAsync(id: string): Promise<Rights> {
    const detail = await invoke<AgentProfileDetail>('agent_profile_get', {
      args: { id },
    })
    return detail.rights
  }

  /** How many agents get more than they have now when `profileId` becomes active. */
  async function widenedAsync(profileId: string): Promise<number> {
    const target = store.profiles.find((profile) => profile.id === profileId)
    const affected = store.subjects.filter(
      (subject) =>
        target?.isStandard ||
        subject.profiles.some((profile) => profile.id === profileId),
    )
    const cache = new Map<string, Rights>()
    const rightsOf = async (id: string) => {
      const known = cache.get(id)
      if (known) return known
      const rights = await rightsOfAsync(id)
      cache.set(id, rights)
      return rights
    }
    const next = await rightsOf(profileId)
    let count = 0
    for (const subject of affected) {
      if (subject.profileId === profileId) continue
      if (widens(await rightsOf(subject.profileId), next)) count += 1
    }
    return count
  }

  async function runAsync(profileId: string) {
    try {
      await store.activateAsync(profileId)
    } catch (error) {
      toast.error(errString(error))
    }
  }

  /** Asks first when the change gives agents more; otherwise it applies at once. */
  async function requestAsync(profileId: string) {
    const request = ++requestSequence
    pending.value = null
    if (store.profiles.find((profile) => profile.id === profileId)?.isActive) {
      return
    }
    try {
      const count = await widenedAsync(profileId)
      if (request !== requestSequence) return
      if (count > 0) {
        pending.value = { profileId, count }
        return
      }
      await runAsync(profileId)
    } catch (error) {
      toast.error(errString(error))
    }
  }

  async function confirmAsync() {
    const answer = pending.value
    pending.value = null
    if (answer) await runAsync(answer.profileId)
  }

  function cancel() {
    requestSequence += 1
    pending.value = null
  }

  return { pending, requestAsync, confirmAsync, cancel }
}
