import { toast } from 'vue-sonner'

/**
 * Making an agent profile the active one (spec 083 FR-006a): the profile's rights then apply to the
 * agents that are members of it, and only its models are offered in the chat (FR-006b). It applies
 * at once, with no question: a profile with more rights gives its agents more, which is what
 * choosing it means (FR-016). Used by the list of profiles, the page of a profile and the chat;
 * the change itself is the guardrail action `settings.agents.profiles.activate`, which no agent
 * can call. A failure is a toast.
 */
export function useAgentProfileActivation() {
  const { errString } = useErrorString()
  const store = useAgentProfilesStore()
  let requested = 0
  let queued = Promise.resolve()

  function activateAsync(profileId: string) {
    const request = ++requested
    queued = queued.then(async () => {
      if (request !== requested) return
      if (store.profiles.find((profile) => profile.id === profileId)?.isActive)
        return
      await store.activateAsync(profileId)
    })
    return queued.catch((error: unknown) => {
      toast.error(errString(error))
    })
  }

  return { activateAsync }
}
