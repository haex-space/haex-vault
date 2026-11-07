// `pnpm check:mcp-actions` (spec 021-mcp-server): what the frontend pushes for external agents
// and the catalog rules the MCP server relies on. Rust refuses a whole push when one definition is
// invalid, so the full list has to pass the same checks the built-in agent's share passes; the
// guardrails of the MCP access must never reach any agent (FR-016).
import assert from 'node:assert/strict'
import { test } from 'node:test'
import { reactive } from 'vue'

import {
  TOOL_NAME_PATTERN,
  listAgentActions,
  listPushedActions,
  type TitleOf,
} from '../src/lib/actions/agentTools.ts'
import { ALL_ACTIONS } from '../src/lib/actions/catalog.ts'
import { validate } from '../src/lib/actions/schema.ts'
import { ACTION_SCOPE_IDS } from '../src/lib/actions/scopes.ts'
import {
  OPERATE_GROUPS,
  OPERATE_SCOPES,
  groupLevel,
  passwordsComplete,
  rightsCount,
  rightsDraftOf,
  rightsOf,
  siteOf,
  withGroupLevel,
  type PasswordRights,
  type BrowserRights,
  type RightsDraft,
} from '../src/lib/agents/rights.ts'
import {
  kindLabelKey,
  profileErrorCode,
  profileLabel,
} from '../src/lib/agents/profiles.ts'
import { loadScriptSetup } from './lib/script-setup-sandbox.ts'

const titleOf: TitleOf = (key, locale) => `${locale}:${key}`

/** `vue-sonner` for a page under test: each error toast lands in `errors`. */
function toastDouble(errors: string[]) {
  return { toast: { error: (message: string) => errors.push(message) } }
}

/** A draft of `operate`, with the other groups changed as given. */
function draft(
  operate: Record<string, 'read' | 'write'>,
  groups: Partial<Omit<RightsDraft, 'levels'>> = {},
): RightsDraft {
  return { ...rightsDraftOf({ operate }), ...groups }
}

test('every action any agent may call is pushed, with its scope, and no guardrail', () => {
  const pushed = listPushedActions(ALL_ACTIONS, titleOf)
  const expected = ALL_ACTIONS.filter(
    (a) => a.agentCallable && a.scope !== 'guardrails',
  ).map((a) => a.id)
  assert.deepEqual(pushed.map((d) => d.actionId).sort(), expected.sort())
  for (const def of pushed) {
    assert.notEqual(def.scope, 'guardrails', def.actionId)
    assert.ok(ACTION_SCOPE_IDS.includes(def.scope), def.actionId)
  }
})

test('the push carries the built-in agent share unchanged', () => {
  const pushed = listPushedActions(ALL_ACTIONS, titleOf)
  const builtin = listAgentActions(ALL_ACTIONS, titleOf)
  assert.deepEqual(
    pushed
      .filter((d) => d.builtinAgentCallable)
      .map((d) => d.actionId)
      .sort(),
    builtin.map((d) => d.actionId).sort(),
  )
  const externalOnly = pushed
    .filter((d) => !d.builtinAgentCallable)
    .map((d) => d.actionId)
  assert.deepEqual(externalOnly.sort(), [
    'chat.message.retry',
    'chat.message.send',
    'chat.reply.cancel',
    'passwords.secret.get',
    'settings.models.activate',
    'settings.models.deactivate',
  ])
})

test('pushed tool names fit the provider pattern and are unique', () => {
  const names = listPushedActions(ALL_ACTIONS, titleOf).map((d) => d.toolName)
  for (const name of names) assert.match(name, TOOL_NAME_PATTERN, name)
  assert.equal(new Set(names).size, names.length)
})

test('the actions of the MCP access are guardrails no agent can call (FR-016)', () => {
  const own = ALL_ACTIONS.filter((a) => a.id.startsWith('settings.agents.mcp.'))
  assert.ok(own.length > 0, 'the switch exists')
  for (const action of own) {
    assert.equal(action.scope, 'guardrails', action.id)
    assert.equal(action.agentCallable, false, action.id)
  }
})

test('the switch answers with the status Rust sends, the HTTP port included', () => {
  const action = ALL_ACTIONS.find((a) => a.id === 'settings.agents.mcp.enable')
  assert.ok(action, 'settings.agents.mcp.enable is in the catalog')
  const running = { enabled: true, state: 'running', httpPort: 41234 }
  assert.deepEqual(validate(action.result, running), { ok: true })
  const stopped = { enabled: false, state: 'off', httpPort: null }
  assert.deepEqual(validate(action.result, stopped), { ok: true })
})

test('the scope for one secret of a password entry exists (research R13)', () => {
  assert.ok(ACTION_SCOPE_IDS.includes('passwords.secret'))
})

test('passwords.secret.get reads one secret, natively, never for the built-in agent (T074, R13)', () => {
  const secret = ALL_ACTIONS.find((a) => a.id === 'passwords.secret.get')
  assert.ok(secret)
  assert.equal(secret.scope, 'passwords.secret')
  assert.equal(secret.effect, 'read')
  assert.equal(secret.builtinAgentCallable, false)
  assert.equal(secret.runner, 'native')
  assert.deepEqual(secret.input.required, ['entryId', 'field'])
  for (const [name, field] of Object.entries(secret.input.properties ?? {}))
    assert.notEqual(
      (field as Record<string, unknown>)['x-holzi-secret'],
      true,
      `${name} is an id or a field name, not a secret`,
    )
})

test('a website is written as Rust writes it, and no website is refused (spec 065)', () => {
  const cases: Array<[string, string | null]> = [
    ['https://Shop.Example.de/kasse?x=1', 'https://shop.example.de'],
    ['shop.example.de', 'https://shop.example.de'],
    ['https://shop.example.de:443', 'https://shop.example.de'],
    ['https://shop.example.de:8443/a', 'https://shop.example.de:8443'],
    ['http://intranet.example:80', 'http://intranet.example'],
    ['https://bücher.example', 'https://xn--bcher-kva.example'],
    ['file:///etc/passwd', null],
    ['javascript:alert(1)', null],
    ['https://user:pw@example.com', null],
    ['   ', null],
  ]
  for (const [typed, site] of cases) assert.equal(siteOf(typed), site, typed)
})

test('changing a scope keeps the browser rights, and an off group is left out (spec 065)', () => {
  const browser = {
    userBrowser: false,
    requests: {},
    sites: {
      'https://shop.example.de': {
        level: 'operate' as const,
        fillRemembered: false,
      },
    },
  }
  assert.deepEqual(rightsOf(draft({ 'wm.read': 'read' }, { browser })), {
    operate: { 'wm.read': 'read' },
    browser,
  })
  assert.deepEqual(rightsOf(draft({ 'wm.read': 'read' })), {
    operate: { 'wm.read': 'read' },
  })
})

test('granted extensions are kept with the other rights, and none is left out (spec 021 US4)', () => {
  const mail = '0b7d3c62-9f2e-4c1a-8d55-2a4b6e1f0c99'
  assert.deepEqual(
    rightsOf(draft({ 'wm.read': 'read' }, { extensions: [mail] })),
    {
      operate: { 'wm.read': 'read' },
      extensions: [mail],
    },
  )
  assert.deepEqual(rightsOf(draft({ 'wm.read': 'read' }, { extensions: [] })), {
    operate: { 'wm.read': 'read' },
  })
})

test('the group passwords is kept with the other rights, and off is left out (spec 021 US5)', () => {
  const passwords = { level: 'read' as const, tags: ['dev'] }
  assert.deepEqual(rightsOf(draft({}, { passwords })), {
    operate: {},
    passwords,
  })
  assert.deepEqual(rightsOf(draft({}, { passwords: null })), { operate: {} })
})

test('the group commands is kept with the other rights, and off is left out (spec 021 US5)', () => {
  assert.deepEqual(rightsOf(draft({}, { commands: true })), {
    operate: {},
    commands: true,
  })
  assert.deepEqual(rightsOf(draft({}, { commands: false })), { operate: {} })
})

test('the profile page can save every rights group through its action (spec 083 T033)', () => {
  const action = ALL_ACTIONS.find(
    (a) => a.id === 'settings.agents.profiles.rights.set',
  )
  assert.ok(action, 'settings.agents.profiles.rights.set is in the catalog')
  const rights = rightsOf(
    draft(
      { 'wm.read': 'read' },
      {
        browser: {
          userBrowser: true,
          all: 'read',
          sites: {
            'https://example.org': { level: 'read', fillRemembered: false },
            'https://blocked.example.org': {
              level: 'blocked',
              fillRemembered: false,
            },
          },
          requests: { 'https://tracker.example.org': 'deny' },
        },
        passwords: { level: 'readWrite', tags: ['dev'], defaultTag: 'dev' },
        extensions: ['0b7d3c62-9f2e-4c1a-8d55-2a4b6e1f0c99'],
        commands: true,
      },
    ),
  )
  assert.deepEqual(
    validate(action.input, {
      id: '1f6a2b3c-0d4e-4f5a-8b6c-7d8e9f0a1b2c',
      rights,
    }),
    { ok: true },
  )
  // Every group off, the least a profile can hold.
  assert.deepEqual(
    validate(action.input, {
      id: '1f6a2b3c-0d4e-4f5a-8b6c-7d8e9f0a1b2c',
      rights: rightsOf(draft({})),
    }),
    { ok: true },
  )
})

test('an incomplete password tag selection revokes access instead of retaining all entries', () => {
  for (const tags of [[], [' ', '\t']]) {
    const passwords = { level: 'read' as const, tags }
    assert.equal(passwordsComplete(passwords), false)
    assert.deepEqual(rightsOf(draft({ 'wm.read': 'read' }, { passwords })), {
      operate: { 'wm.read': 'read' },
    })
  }
  for (const tags of [null, ['dev']]) {
    const passwords = { level: 'read' as const, tags }
    assert.equal(passwordsComplete(passwords), true)
    assert.deepEqual(rightsOf(draft({}, { passwords })).passwords, passwords)
  }
})

test('every scope of "holzi bedienen" is in exactly one row of the rights list', () => {
  const grouped = OPERATE_GROUPS.flatMap((group) => [...group.scopes])
  assert.deepEqual([...grouped].sort(), [...OPERATE_SCOPES].sort())
})

test('a row sets every scope of its topic and shows the widest of them', () => {
  const chat = OPERATE_GROUPS.find((group) => group.id === 'chat')!
  const none = rightsDraftOf(null).levels
  const written = withGroupLevel(none, chat, 'write')
  assert.equal(written['chat.read'], 'write')
  assert.equal(written['chat.write'], 'write')
  assert.equal(written['wm.read'], 'none', 'other topics stay as they are')
  assert.equal(groupLevel(written, chat), 'write')
  // Rights from before the rows can differ inside a topic; the row shows what is granted most.
  const mixed = { ...none, 'chat.read': 'read', 'chat.write': 'write' } as const
  assert.equal(groupLevel(mixed, chat), 'write')
  assert.deepEqual(rightsOf({ ...draft({}), levels: written }).operate, {
    'chat.read': 'write',
    'chat.write': 'write',
  })
  assert.deepEqual(
    rightsCount({
      operate: {
        'chat.read': 'read',
        'chat.write': 'read',
        'wm.layout': 'write',
      },
    }),
    { read: 1, write: 1 },
  )
})

const vaultTags = {
  usePasswordsStore: () => ({
    hasLoadedOnce: true,
    displayTags: [{ name: 'shop' }, { name: 'dev' }],
  }),
}

/** The password row with the given stored rights, and every update it emits. */
function passwordRow(modelValue: PasswordRights | null) {
  const updates: Array<PasswordRights | null> = []
  const row = loadScriptSetup<{
    setLevel: (level: string) => void
    setTags: (tags: string[]) => void
    setDefaultTag: (tag: string) => void
    tagChoices: { value: string[] }
  }>(
    'src/components/agents/PasswordRightsFields.vue',
    ['setLevel', 'setTags', 'setDefaultTag', 'tagChoices'],
    {
      props: { modelValue },
      ...vaultTags,
      emit: (_event, value) => updates.push(value as PasswordRights | null),
    },
  )
  return { row, updates }
}

/** The browser row with the given stored rights, and every update it emits. */
function browserRow(modelValue: BrowserRights | null) {
  const updates: Array<BrowserRights | null> = []
  const row = loadScriptSetup<{
    setChoice: (choice: string) => void
    setLevel: (site: string, level: string) => void
    add: () => void
    draft: { value: string }
  }>(
    'src/components/agents/BrowserRightsFields.vue',
    ['setChoice', 'setLevel', 'add', 'draft'],
    {
      props: { modelValue },
      imports: { '~/lib/agents/rights': { siteOf } },
      useFieldLabels: () => ({ input: { value: {} } }),
      emit: (_event, value) => updates.push(value as BrowserRights | null),
    },
  )
  return { row, updates }
}

/** What the settings action of a profile takes: every field the row sends is known. */
function profileTakes(rights: BrowserRights | null) {
  const action = ALL_ACTIONS.find(
    (candidate) => candidate.id === 'settings.agents.profiles.rights.set',
  )
  assert.ok(action)
  return validate(action.input, {
    id: 'a-profile',
    rights: { operate: {}, browser: rights },
  })
}

test('the browser row has the levels of every other row, and what it sends the action takes (spec 065)', () => {
  const { row, updates } = browserRow(null)
  row.setChoice('write')
  assert.deepEqual(updates.at(-1), {
    userBrowser: false,
    sites: {},
    requests: {},
    all: 'operate',
  })
  assert.deepEqual(profileTakes(updates.at(-1) ?? null), { ok: true })
  row.setChoice('read')
  assert.equal(updates.at(-1)?.all, 'read')
  row.setChoice('ask')
  assert.equal(updates.at(-1)?.all, null)
  assert.deepEqual(profileTakes(updates.at(-1) ?? null), { ok: true })
  row.setChoice('none')
  assert.equal(updates.at(-1), null)
  assert.deepEqual(profileTakes(null), { ok: true })
})

test('a website typed in the browser block is blocked, and its level changes in its row (FR-047)', () => {
  const { row, updates } = browserRow({
    userBrowser: false,
    sites: {},
    requests: {},
    all: 'operate',
  })
  row.draft.value = 'bank.example'
  row.add()
  const blocked = updates.at(-1)
  assert.deepEqual(blocked?.sites, {
    'https://bank.example': { level: 'blocked', fillRemembered: false },
  })
  assert.deepEqual(profileTakes(blocked ?? null), { ok: true })
  assert.equal(blocked?.all, 'operate', 'the level for all stays')
  const { row: listed, updates: listedUpdates } = browserRow(blocked ?? null)
  listed.setLevel('https://bank.example', 'read')
  assert.equal(
    listedUpdates.at(-1)?.sites['https://bank.example']?.level,
    'read',
  )
})

test('the password row has the levels of every other row (spec 021 US5)', () => {
  const { row, updates } = passwordRow(null)
  row.setLevel('read')
  assert.deepEqual(updates.at(-1), { level: 'read', tags: null })
  row.setLevel('write')
  assert.deepEqual(updates.at(-1), { level: 'readWrite', tags: null })
  const { row: on, updates: onUpdates } = passwordRow({
    level: 'readWrite',
    tags: ['dev'],
    defaultTag: 'dev',
  })
  on.setLevel('none')
  assert.equal(onUpdates.at(-1), null)
  on.setLevel('read')
  assert.deepEqual(onUpdates.at(-1), { level: 'read', tags: ['dev'] })
})

test('one dropdown names the entries: all, or those with the chosen tags (065 FR-036)', () => {
  const { row, updates } = passwordRow({ level: 'read', tags: null })
  assert.deepEqual(
    row.tagChoices.value,
    ['dev', 'shop'],
    'the tags of the vault',
  )
  row.setTags(['shop'])
  assert.deepEqual(updates.at(-1), { level: 'read', tags: ['shop'] })
  row.setTags(['shop', 'dev'])
  assert.deepEqual(updates.at(-1), { level: 'read', tags: ['shop', 'dev'] })
  const { row: writing, updates: writes } = passwordRow({
    level: 'readWrite',
    tags: null,
  })
  // Within tags new entries get one of them, so the rights are complete.
  writing.setTags(['dev'])
  assert.deepEqual(writes.at(-1), {
    level: 'readWrite',
    tags: ['dev'],
    defaultTag: 'dev',
  })
  assert.equal(passwordsComplete(writes.at(-1) ?? null), true)
  writing.setTags([])
  assert.deepEqual(writes.at(-1), {
    level: 'readWrite',
    tags: null,
    defaultTag: null,
  })
  // Reading becomes reading and changing within the tags.
  const { row: tagged, updates: tagUpdates } = passwordRow({
    level: 'read',
    tags: ['shop', 'dev'],
  })
  tagged.setLevel('write')
  assert.deepEqual(tagUpdates.at(-1), {
    level: 'readWrite',
    tags: ['shop', 'dev'],
    defaultTag: 'shop',
  })
})

test('the tag for new entries is one of the chosen tags and follows the choice', () => {
  const { row, updates } = passwordRow({
    level: 'readWrite',
    tags: ['dev', 'shop'],
    defaultTag: 'shop',
  })
  row.setDefaultTag('dev')
  assert.deepEqual(updates.at(-1), {
    level: 'readWrite',
    tags: ['dev', 'shop'],
    defaultTag: 'dev',
  })
  // Dropping the tag new entries get moves it to another chosen tag.
  const { row: dropping, updates: dropped } = passwordRow({
    level: 'readWrite',
    tags: ['dev', 'shop'],
    defaultTag: 'shop',
  })
  dropping.setTags(['dev'])
  assert.deepEqual(dropped.at(-1), {
    level: 'readWrite',
    tags: ['dev'],
    defaultTag: 'dev',
  })
  assert.equal(passwordsComplete(dropped.at(-1) ?? null), true)
  // With all entries new entries get no tag, also one an earlier editor stored.
  const { row: all, updates: allUpdates } = passwordRow({
    level: 'readWrite',
    tags: null,
    defaultTag: 'shop',
  })
  all.setLevel('write')
  assert.deepEqual(allUpdates.at(-1), {
    level: 'readWrite',
    tags: null,
    defaultTag: null,
  })
})

test('the registration answers with the chosen agent profile and stays open when it is gone (spec 083 US4)', async () => {
  const answers: Array<{ profileId: string; decision: string }> = []
  const toasts: string[] = []
  const STANDARD = '61705f70-726f-6669-6c65-5f7374640083'
  const STRICT = '0b7d3c62-9f2e-4c1a-8d55-2a4b6e1f0c99'
  let gone = false
  const fetched: string[] = []
  const profiles = {
    profiles: [
      { id: STANDARD, name: 'Standard', isStandard: true, memberCount: 0 },
    ],
    reloadAsync: async () => {},
  }
  const dialog = loadScriptSetup<{
    profileId: { value: string }
    draft: { value: RightsDraft }
    choices: { value: Array<{ id: string }> }
    selectAsync: (id: string) => Promise<void>
    answerAsync: (register: boolean) => Promise<void>
  }>(
    'src/components/agents/RegistrationDialog.vue',
    ['profileId', 'draft', 'choices', 'selectAsync', 'answerAsync'],
    {
      imports: {
        '~/lib/agents/profiles': {
          STANDARD_PROFILE_ID: STANDARD,
          profileErrorCode,
          profileLabel,
        },
        '~/lib/agents/rights': { rightsDraftOf },
        '@tauri-apps/api/core': {
          invoke: async (
            _command: string,
            { args }: { args: { id: string } },
          ) => {
            fetched.push(args.id)
            if (args.id === STRICT && gone) {
              throw { kind: 'InvalidInput', reason: 'profile_missing' }
            }
            // The strict profile is the active one, so an agent that joins it has its rights.
            return {
              id: args.id,
              isStandard: args.id === STANDARD,
              isActive: args.id === STRICT,
              rights:
                args.id === STRICT
                  ? { operate: { 'wm.read': 'write' } }
                  : { operate: { 'wm.read': 'read' } },
            }
          },
        },
        'vue-sonner': toastDouble(toasts),
      },
      useErrorString: () => ({ errString: String }),
      useMcpAgentsStore: () => ({
        shown: {
          requestId: 'registration',
          question: {
            rights: { operate: { 'wm.read': 'read' } },
            profiles: [
              { id: STANDARD, name: 'Standard' },
              { id: STRICT, name: 'Strikt' },
            ],
            profileId: STANDARD,
            mode: 'manual',
            delegate: false,
          },
        },
        answerAsync: async (_id: string, answer: (typeof answers)[number]) => {
          if (answer.profileId === STRICT && gone) {
            throw { kind: 'InvalidInput', reason: 'profile_missing' }
          }
          answers.push(answer)
        },
      }),
      useAgentProfilesStore: () => profiles,
    },
  )
  assert.equal(
    dialog.profileId.value,
    STANDARD,
    'a new agent starts in Standard',
  )
  assert.equal(groupLevel(dialog.draft.value.levels, OPERATE_GROUPS[0]), 'read')
  await dialog.selectAsync(STRICT)
  assert.equal(
    groupLevel(dialog.draft.value.levels, OPERATE_GROUPS[0]),
    'write',
    'the rights of the chosen profile are shown',
  )
  await dialog.answerAsync(true)
  assert.deepEqual(answers, [
    {
      decision: 'register',
      remember: false,
      profileId: STRICT,
      mode: 'manual',
    },
  ])
  // The profile is deleted while the dialog is open: the answer is refused, the choices are
  // reloaded, "Standard" is selected again and nothing was answered.
  gone = true
  await dialog.answerAsync(true)
  assert.equal(answers.length, 1, 'the question stays open')
  assert.equal(toasts.length, 1, 'the refusal is a toast')
  assert.deepEqual(
    dialog.choices.value.map((choice) => choice.id),
    [STANDARD],
  )
  assert.equal(dialog.profileId.value, STANDARD)
  await dialog.answerAsync(false)
  assert.equal(answers.length, 2, 'declining is always possible')
  assert.equal(answers[1]?.decision, 'deny')
})

test('a switch adds the agent to a profile or removes it, and a failure is a toast (spec 083 FR-006)', async () => {
  const calls: unknown[][] = []
  const toasts: string[] = []
  let fail = false
  const memberships = loadScriptSetup<{
    isMember: (profileId: string) => boolean
    setAsync: (profileId: string, member: boolean) => Promise<void>
  }>(
    'src/components/settings/agentProfiles/Memberships.vue',
    ['isMember', 'setAsync'],
    {
      props: { subject: 'agent:agent' },
      imports: {
        '~/lib/agents/profiles': { profileLabel },
        'vue-sonner': toastDouble(toasts),
      },
      useErrorString: () => ({ errString: String }),
      useAgentProfilesStore: () => ({
        profiles: [],
        subjects: [
          {
            subject: 'agent:agent',
            profileId: 'standard',
            profileName: 'Standard',
            profiles: [{ id: 'strict', name: 'Strikt' }],
          },
        ],
        setMemberAsync: async (...args: unknown[]) => {
          if (fail) throw new Error('add failed')
          calls.push(args)
        },
      }),
    },
  )
  assert.equal(memberships.isMember('strict'), true)
  assert.equal(memberships.isMember('open'), false)
  await memberships.setAsync('open', true)
  await memberships.setAsync('strict', false)
  assert.deepEqual(calls, [
    ['agent:agent', 'open', true],
    ['agent:agent', 'strict', false],
  ])
  fail = true
  await memberships.setAsync('open', true)
  assert.deepEqual(toasts, ['Error: add failed'], 'the failure is a toast')
})

test('revoking an agent shows no error toast for the reload its change event starts', async () => {
  const route = reactive({ params: { agentId: 'agent' } })
  const toasts: string[] = []
  const pushed: string[] = []
  let gone = false
  let failRevoke = false
  let changeReload: Promise<void> = Promise.resolve()
  const detail = loadScriptSetup<{
    revokeAsync: () => Promise<void>
    reloadAsync: () => Promise<void>
  }>(
    'src/components/settings/agents/AgentDetailView.vue',
    ['revokeAsync', 'reloadAsync'],
    {
      imports: {
        '~/lib/agents/profiles': { kindLabelKey, profileLabel },
        '~/lib/agents/rights': { rightsDraftOf },
        '@tauri-apps/api/core': {
          invoke: async () => {
            if (gone) throw new Error('no such agent')
            return { id: route.params.agentId, rights: { operate: {} } }
          },
        },
        'vue-sonner': toastDouble(toasts),
      },
      useErrorString: () => ({ errString: String }),
      useTabRouter: () => ({
        route,
        push: async (path: string) => {
          pushed.push(path)
        },
      }),
      useMcpAgentsStore: () => ({ changes: 0 }),
      useAgentProfilesStore: () => ({
        profiles: [],
        reloadAsync: async () => {},
      }),
      useActionOrThrow: (id: string) => async () => {
        if (id !== 'settings.agents.mcp.revoke') return
        if (failRevoke) throw new Error('revoke failed')
        gone = true
        // Rust sends `mcp-agents-changed` before the command answers.
        changeReload = detail.reloadAsync()
      },
    },
  )
  await detail.mount()
  failRevoke = true
  await detail.revokeAsync()
  assert.deepEqual(
    toasts,
    ['Error: revoke failed'],
    'a failed revocation is a toast',
  )
  assert.deepEqual(pushed, [])
  failRevoke = false
  await detail.revokeAsync()
  await changeReload
  assert.deepEqual(pushed, ['/ai/agents'])
  assert.deepEqual(
    toasts,
    ['Error: revoke failed'],
    'the gone agent is no new toast',
  )
})
