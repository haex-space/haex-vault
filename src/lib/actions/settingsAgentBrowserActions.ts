// The settings actions of the agent browser (spec 065, data-model.md "Katalogaktionen"): the
// browser chosen on this device, the tracker lists, the cookies and removing the browser's profile.
// The rights of the agents are in their agent profiles (spec 083). Guardrails: they decide what an agent may reach on the web, which is the
// user's choice only, so no agent can call them. The handlers are in
// `stores/settingsActionHandlers.ts`. Pure, relative `.ts` imports only.
import type { ActionDefinition } from './types.ts'

/** The fields every guardrail of the agent browser shares. */
const GUARDRAIL = {
  /** The commands answer nothing. */
  result: { type: 'object', nullable: true },
  target: 'none',
  scope: 'guardrails',
  agentCallable: false,
  binding: 'global',
} as const

/** What names one cookie of the profile (Rust `CookieRef`). */
const COOKIE = {
  type: 'object',
  properties: {
    name: { type: 'string' },
    domain: { type: 'string' },
    path: { type: 'string' },
  },
  required: ['name', 'domain', 'path'],
} as const

export const AGENT_BROWSER_ACTIONS: readonly ActionDefinition[] = [
  {
    id: 'settings.agentBrowser.path.set',
    titleKey: 'actions.settings.agentBrowser.pathSet',
    description:
      'Choose the browser program the agent browser starts on this device; null searches for one again.',
    input: {
      type: 'object',
      properties: { path: { type: 'string', nullable: true } },
      required: ['path'],
    },
    effect: 'write',
    ...GUARDRAIL,
  },
  {
    id: 'settings.agentBrowser.trackers.set',
    titleKey: 'actions.settings.agentBrowser.trackersSet',
    description:
      'Set the tracker blocking of the agent browser on this device: blocking as a whole, the default filter lists switched off, and own lists by their https address. Answers the state of the lists.',
    input: {
      type: 'object',
      properties: {
        settings: {
          type: 'object',
          properties: {
            enabled: { type: 'boolean' },
            off: { type: 'array', items: { type: 'string' } },
            custom: {
              type: 'array',
              items: {
                type: 'object',
                properties: {
                  id: { type: 'string' },
                  name: { type: 'string' },
                  url: { type: 'string' },
                  enabled: { type: 'boolean' },
                },
                required: ['url'],
              },
            },
          },
          required: ['enabled'],
        },
      },
      required: ['settings'],
    },
    effect: 'write',
    ...GUARDRAIL,
    result: { type: 'object' },
  },
  {
    id: 'settings.agentBrowser.consent.set',
    titleKey: 'actions.settings.agentBrowser.consentSet',
    description:
      'Set whether the agent browser declines the optional cookies of cookie banners (rejectOptional, on by default). It never accepts anything. Applies to the tabs the agent opens from then on. Answers the choice.',
    input: {
      type: 'object',
      properties: {
        settings: {
          type: 'object',
          properties: { rejectOptional: { type: 'boolean' } },
          required: ['rejectOptional'],
        },
      },
      required: ['settings'],
    },
    effect: 'write',
    ...GUARDRAIL,
    result: { type: 'object' },
  },
  {
    id: 'settings.agentBrowser.dns.set',
    titleKey: 'actions.settings.agentBrowser.dnsSet',
    description:
      'Set how the agent browser looks up website names on this device: mode (off, automatic, secure), provider (quad9, cloudflare, google, mullvad, custom) and, for custom, an https address. Applies at the next start of the browser. Answers the choice and the providers.',
    input: {
      type: 'object',
      properties: {
        settings: {
          type: 'object',
          properties: {
            mode: { type: 'string', enum: ['off', 'automatic', 'secure'] },
            provider: {
              type: 'string',
              enum: ['quad9', 'cloudflare', 'google', 'mullvad', 'custom'],
            },
            custom: { type: 'string' },
          },
          required: ['mode', 'provider'],
        },
      },
      required: ['settings'],
    },
    effect: 'write',
    ...GUARDRAIL,
    result: { type: 'object' },
  },
  {
    id: 'settings.agentBrowser.profile.cookie.set',
    titleKey: 'actions.settings.agentBrowser.cookieSet',
    description:
      'Change the value and the expiry of one cookie in the profile of the agent browser (expires: seconds since 1970, null for a cookie of the session).',
    input: {
      type: 'object',
      properties: {
        cookie: COOKIE,
        value: { type: 'string' },
        expires: { type: 'number', nullable: true },
      },
      required: ['cookie', 'value'],
    },
    effect: 'write',
    ...GUARDRAIL,
  },
  {
    id: 'settings.agentBrowser.profile.cookie.delete',
    titleKey: 'actions.settings.agentBrowser.cookieDelete',
    description:
      'Delete one cookie ({ cookie: { name, domain, path } }) or every cookie of a website ({ site }) from the profile of the agent browser. Answers how many went.',
    input: {
      type: 'object',
      properties: {
        target: {
          type: 'object',
          description:
            'Either { cookie: { name, domain, path } } for one cookie or { site: "example.com" } for a whole website.',
        },
      },
      required: ['target'],
    },
    effect: 'destructive',
    ...GUARDRAIL,
    result: { type: 'number' },
  },
  {
    id: 'settings.agentBrowser.profile.clear',
    titleKey: 'actions.settings.agentBrowser.profileClear',
    description:
      'End the agent browser and remove its profile: every login of the agent browser on websites is gone.',
    input: { type: 'object' },
    effect: 'destructive',
    ...GUARDRAIL,
  },
]
