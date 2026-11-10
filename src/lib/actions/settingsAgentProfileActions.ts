// The settings actions of agent profiles (spec 083, contracts/commands-and-actions.md): creating,
// renaming and deleting a profile, setting its rights, and putting an agent into one. Guardrails:
// they decide what every agent in a profile may do, which is the user's choice only, so no agent
// can call them (spec 021 FR-016). Listed in `catalog.ts`; the handlers are in
// `stores/settingsActionHandlers.ts`. Pure, relative `.ts` imports only.
import type { ActionDefinition } from './types.ts'

/** The fields every guardrail on a profile shares. */
const GUARDRAIL = {
  /** The commands answer nothing. */
  result: { type: 'object', nullable: true },
  target: 'none',
  scope: 'guardrails',
  agentCallable: false,
  binding: 'global',
} as const

const PROFILE_ID = {
  type: 'string',
  description: 'The id of an agent profile; "Standard" has a fixed id.',
} as const

/** The rights of a profile, as the window edits them (Rust `Rights`, spec 021 FR-008 and 065). */
export const RIGHTS_SCHEMA = {
  type: 'object',
  properties: {
    operate: {
      type: 'object',
      description:
        'Per scope of "holzi bedienen" (wm.read, wm.navigation, wm.layout, chat.read, chat.write, settings.read, settings.device, settings.models): "read" or "write".',
    },
    browser: {
      type: 'object',
      nullable: true,
      description:
        'The rights group "browser": { userBrowser, sites: { "<https://host>": { level: "read" | "operate" | "blocked", fillRemembered } }, requests: { "<https://host>": "allow" | "deny" }, all: "read" | "operate" | null (the level for every website not listed) }.',
    },
    passwords: {
      type: 'object',
      nullable: true,
      description:
        'The rights group "passwords": { level: "read" | "readWrite", tags: null for every entry or the tag names, defaultTag }.',
    },
    extensions: {
      type: 'array',
      items: { type: 'string' },
      description:
        'The ids of the installed extensions whose tools the agents see.',
    },
    allExtensions: {
      type: 'boolean',
      description:
        '"Alle Erweiterungen": the tools of every extension, also one installed later.',
    },
    commands: {
      type: 'boolean',
      description: 'The rights group "Befehle ausführen".',
    },
    delegate: {
      type: 'object',
      description:
        'The rights group "Abo-Agenten": how the own tools of a subscription agent (Claude Code, Codex) run when its model is in the profile: { autonomy: "standard" | "ungated" | "gated_permissive", deny: ["workspace_escape" | "network_access" | "credential_paths"] } (the deny rules matter for "gated_permissive"). A profile without the group asks for every call.',
    },
    contacts: {
      type: 'object',
      description:
        'The rights group "Kontakte": what an agent reaches in the address book: { all: "read" | "readWrite" | "denied" (every contact), groups: { "<contact group id>": "read" | "readWrite" | "denied" } }. Without an entry the agent is asked at its first access; "denied" for a group hides its members from a grant for all.',
    },
    files: {
      type: 'object',
      description:
        'The rights group "Dateien": { device: true for the files of this device (off without it), storages: { "<storage id>": "read" | "readWrite" | "denied" }, folders: { "<storage id>/<folder id>": { level: "read" | "readWrite" | "denied", cloud: true when cloud models reach it too } }, fileShares: true to manage file shares, syncRules: true to manage sync rules }. A storage or an encrypted folder without an entry asks the user at the first access.',
    },
  },
  required: ['operate'],
} as const

export const AGENT_PROFILE_ACTIONS: readonly ActionDefinition[] = [
  {
    id: 'settings.agents.profiles.create',
    titleKey: 'actions.settings.agents.profiles.create',
    description:
      'Create an agent profile with a name. It gives nothing until its rights are set; another profile may have the same name. User only.',
    input: {
      type: 'object',
      properties: { name: { type: 'string' } },
      required: ['name'],
    },
    ...GUARDRAIL,
    effect: 'write',
  },
  {
    id: 'settings.agents.profiles.rename',
    titleKey: 'actions.settings.agents.profiles.rename',
    description:
      'Rename an agent profile; "Standard" keeps its name. User only.',
    input: {
      type: 'object',
      properties: { id: PROFILE_ID, name: { type: 'string' } },
      required: ['id', 'name'],
    },
    ...GUARDRAIL,
    effect: 'write',
  },
  {
    id: 'settings.agents.profiles.delete',
    titleKey: 'actions.settings.agents.profiles.delete',
    description:
      'Delete an agent profile and its memberships; "Standard" becomes the active profile if it was. "Standard" cannot be deleted. User only.',
    input: {
      type: 'object',
      properties: { id: PROFILE_ID },
      required: ['id'],
    },
    ...GUARDRAIL,
    effect: 'destructive',
  },
  {
    id: 'settings.agents.profiles.rights.set',
    titleKey: 'actions.settings.agents.profiles.rights.set',
    description:
      'Set what every agent in an agent profile may do, in every kind of access at once: "holzi bedienen" per scope and the rights groups "browser", "passwords", "extensions" and "commands". A scope left out is nothing; a group left out is off. User only.',
    input: {
      type: 'object',
      properties: { id: PROFILE_ID, rights: RIGHTS_SCHEMA },
      required: ['id', 'rights'],
    },
    ...GUARDRAIL,
    effect: 'write',
  },
  {
    id: 'settings.agents.profiles.assign',
    titleKey: 'actions.settings.agents.profiles.assign',
    description:
      'Add an agent or a model to an agent profile, or remove it from one; its other agent profiles stay. Every agent is in "Standard" and stays; a model can be taken out of "Standard" (the "Standard" id, member false) and put back, and the chat offers only the models of the active profile. The subject is "agent:<id>" or "model:<id>". User only.',
    input: {
      type: 'object',
      properties: {
        subject: { type: 'string' },
        profileId: PROFILE_ID,
        member: {
          type: 'boolean',
          description:
            'true adds the subject to the profile, false removes it.',
        },
      },
      required: ['subject', 'profileId', 'member'],
    },
    ...GUARDRAIL,
    effect: 'write',
  },
  {
    id: 'settings.agents.profiles.activate',
    titleKey: 'actions.settings.agents.profiles.activate',
    description:
      'Make an agent profile the active one for the vault: the agents that are members of it have its rights, all others those of "Standard", and the chat offers only its models. User only.',
    input: {
      type: 'object',
      properties: { profileId: PROFILE_ID },
      required: ['profileId'],
    },
    ...GUARDRAIL,
    effect: 'write',
  },
]
