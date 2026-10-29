// `pnpm check:agent-actions` (spec 032-model-operates-holzi): the action → tool mapping for the
// built-in agent under `src/lib/actions/agentTools.ts` and the invariants over the shipped catalog
// that keep the model's offer safe — tool names, the guardrail lock, the actions withheld from the
// built-in agent, the core offer, and that every read action runs for a built-in agent caller.
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'

import {
  CORE_AGENT_TOOLS,
  agentSafeMessages,
  describeAction,
  describeToolCall,
  TOOL_NAME_PATTERN,
  fromToolName,
  isBuiltinAgentAction,
  listAgentActions,
  toAgentActionDef,
  toOutcomeWire,
  toToolName,
  type TitleOf,
} from '../src/lib/actions/agentTools.ts'
import { ALL_ACTIONS } from '../src/lib/actions/catalog.ts'
import { forDevice } from '../src/lib/actions/browserActions.ts'
import type { ActionCaller, JsonSchema } from '../src/lib/actions/types.ts'
import { TOOLS_PATH, evalTools } from './export-eval-tools.ts'
import { TARGET_FIELD, catalogRunner, sample } from './lib/actions-harness.ts'

const BUILTIN: ActionCaller = { kind: 'builtinAgent' }
const EXTERNAL: ActionCaller = { kind: 'externalAgent', agentId: 'agent-1' }

/** Withheld from the built-in agent: they would re-enter the running chat turn (ADR-0006). */
const WITHHELD_FROM_BUILTIN = [
  'chat.message.send',
  'chat.message.retry',
  'chat.reply.cancel',
]

const titleOf: TitleOf = (key, locale) => `${locale}:${key}`

test('tool names match the provider pattern and are unique over the whole catalog', () => {
  const names = ALL_ACTIONS.map((a) => toToolName(a.id))
  for (const name of names) assert.match(name, TOOL_NAME_PATTERN, name)
  assert.equal(new Set(names).size, names.length)
})

test('toToolName is reversible over the catalog', () => {
  for (const action of ALL_ACTIONS)
    assert.equal(fromToolName(toToolName(action.id), ALL_ACTIONS), action)
  assert.equal(fromToolName('run_command', ALL_ACTIONS), undefined)
})

test('the built-in agent is offered no guardrail action (FR-003)', () => {
  const offered = listAgentActions(ALL_ACTIONS, titleOf)
  const guardrails = ALL_ACTIONS.filter((a) => a.scope === 'guardrails')
  assert.ok(guardrails.length > 0)
  for (const action of guardrails) {
    assert.ok(
      !offered.some((o) => o.actionId === action.id),
      `${action.id} is offered`,
    )
  }
  for (const def of offered) {
    const action = ALL_ACTIONS.find((a) => a.id === def.actionId)
    assert.ok(action && isBuiltinAgentAction(action), def.actionId)
  }
})

test('exactly the three self-triggering chat actions are withheld from the built-in agent', () => {
  const withheld = ALL_ACTIONS.filter(
    (a) => a.builtinAgentCallable === false,
  ).map((a) => a.id)
  assert.deepEqual(withheld.sort(), [...WITHHELD_FROM_BUILTIN].sort())
  const offered = listAgentActions(ALL_ACTIONS, titleOf).map((d) => d.actionId)
  for (const id of WITHHELD_FROM_BUILTIN) {
    assert.ok(!offered.includes(id), id)
    const action = ALL_ACTIONS.find((a) => a.id === id)
    assert.equal(action?.agentCallable, true, `${id} stays open to spec 021`)
  }
})

test('withheld actions are refused for the built-in agent and accepted for external agents', async () => {
  for (const id of WITHHELD_FROM_BUILTIN) {
    const action = ALL_ACTIONS.find((a) => a.id === id)
    assert.ok(action)
    const input = sample(action.input) as Record<string, unknown>
    const calls: string[] = []
    const refused = await catalogRunner(calls).runAction(id, input, BUILTIN)
    assert.equal(!refused.ok && refused.code, 'forbidden_for_agents', id)
    assert.equal(calls.length, 0, id)
    const accepted = await catalogRunner(calls).runAction(id, input, EXTERNAL)
    assert.equal(accepted.ok, true, id)
  }
})

test('toAgentActionDef passes the action through unchanged and adds localized titles', () => {
  const action = ALL_ACTIONS.find((a) => a.id === 'wm.app.open')
  assert.ok(action)
  const def = toAgentActionDef(action, titleOf)
  assert.equal(def.toolName, 'wm_app_open')
  assert.equal(def.actionId, 'wm.app.open')
  assert.equal(def.description, action.description)
  assert.equal(def.inputSchema, action.input)
  assert.equal(def.effect, action.effect)
  assert.deepEqual(def.titles, {
    de: `de:${action.titleKey}`,
    en: `en:${action.titleKey}`,
  })
})

test('the core offer names existing, built-in-callable actions and leaves room for find_actions and ask_user', () => {
  assert.ok(
    CORE_AGENT_TOOLS.length <= 9,
    'core plus find_actions and ask_user is at most 11',
  )
  assert.equal(new Set(CORE_AGENT_TOOLS).size, CORE_AGENT_TOOLS.length)
  for (const id of CORE_AGENT_TOOLS) {
    const action = ALL_ACTIONS.find((a) => a.id === id)
    assert.ok(action, `${id} is not in the catalog`)
    assert.ok(isBuiltinAgentAction(action), id)
  }
  const defs = listAgentActions(ALL_ACTIONS, titleOf)
  const core = defs.filter((d) => d.core).map((d) => d.actionId)
  assert.deepEqual(core.sort(), [...CORE_AGENT_TOOLS].sort())
})

test('every built-in-callable read action runs for a built-in agent naming its target', async () => {
  // Native actions run in Rust (ADR 0011); `cargo test files::agent` covers them.
  const reads = ALL_ACTIONS.filter(
    (a) => a.effect === 'read' && isBuiltinAgentAction(a) && !a.runner,
  )
  assert.ok(reads.length > 0)
  for (const action of reads) {
    const input = sample(action.input) as Record<string, unknown>
    if (action.target !== 'none') input[TARGET_FIELD[action.target]] = 'target'
    const outcome = await catalogRunner([]).runAction(action.id, input, BUILTIN)
    assert.equal(outcome.ok, true, action.id)
  }
})

/** The ids of the Rust executors of the file, file share and contact share actions
 * (`FILE_ACTION_IDS`, `SHARE_ACTION_IDS`). */
function nativeExecutorIds(): string[] {
  const ids = (file: string, name: string) => {
    const source = readFileSync(
      new URL(`../src-tauri/src/${file}`, import.meta.url),
      'utf8',
    )
    const list = new RegExp(`${name}: &\\[&str\\] = &\\[([^\\]]*)\\]`).exec(
      source,
    )?.[1]
    assert.ok(list, `${name} not found`)
    return [...list.matchAll(/"([^"]+)"/g)].map((match) => match[1] ?? '')
  }
  return [
    ...ids('files/agent/exec.rs', 'FILE_ACTION_IDS'),
    ...ids('files/agent/shares.rs', 'SHARE_ACTION_IDS'),
    ...ids('contacts/agent_share.rs', 'SHARE_ACTION_IDS'),
    ...ids('agent_browser/executor.rs', 'BROWSER_ACTION_IDS'),
  ]
}

test('every native action has its Rust executor and every executor its action (ADR 0011)', () => {
  const native = ALL_ACTIONS.filter((a) => a.runner === 'native')
  assert.deepEqual(native.map((a) => a.id).sort(), nativeExecutorIds().sort())
  for (const action of native) {
    // No window is involved: nothing to wait for, nothing to target.
    assert.equal(action.binding, 'global', action.id)
    assert.equal(action.target, 'none', action.id)
    assert.ok(isBuiltinAgentAction(action), action.id)
    assert.ok(!CORE_AGENT_TOOLS.includes(action.id), action.id)
    assert.equal(toAgentActionDef(action, titleOf).runner, 'native', action.id)
  }
  const show = ALL_ACTIONS.find((a) => a.id === 'files.show')
  assert.ok(show && !show.runner, 'files.show opens a window and runs there')
  assert.equal(
    toAgentActionDef(show, titleOf).runner,
    undefined,
    'a window action carries no runner',
  )
})

test('contacts.share is destructive, asks for itself and needs the permission to read contacts (spec 060 FR-029)', () => {
  const share = ALL_ACTIONS.find((a) => a.id === 'contacts.share')
  assert.ok(share)
  assert.equal(share.effect, 'destructive')
  assert.equal(share.scope, 'contacts.read')
  assert.equal(share.runner, 'native')
  assert.equal(share.confirmsItself, true)
  assert.equal(toAgentActionDef(share, titleOf).confirmsItself, true)
  // The action says that the user confirms every call, so a model does not ask for the link twice.
  assert.match(share.description, /every call/)
  // Only the actions with a dialog of their own skip the chat's approval: this one and the browser
  // tools that change something (spec 065).
  const asking = ALL_ACTIONS.filter((a) => a.confirmsItself).map((a) => a.id)
  assert.deepEqual(
    asking.filter((id) => !id.startsWith('browser.')),
    ['contacts.share'],
  )
  for (const other of ALL_ACTIONS.filter((a) => !asking.includes(a.id)))
    assert.equal(
      toAgentActionDef(other, titleOf).confirmsItself,
      undefined,
      other.id,
    )
})

test('wm.state.get tells the model which ids serve as action targets (FR-004)', () => {
  const action = ALL_ACTIONS.find((a) => a.id === 'wm.state.get')
  assert.ok(action && isBuiltinAgentAction(action))
  for (const field of ['workspaceId', 'windowId', 'tabId'])
    assert.ok(action.description.includes(field), field)
})

test('toOutcomeWire never carries the raw error of a failed handler (FR-006)', () => {
  const secret = new Error('cannot read /home/someone/vault.db')
  const failed = toOutcomeWire({
    ok: false,
    code: 'failed',
    message: secret.message,
    error: secret,
  })
  assert.deepEqual(Object.keys(failed).sort(), ['code', 'message', 'ok'])
  const invalid = toOutcomeWire({
    ok: false,
    code: 'invalid_input',
    message: 'bad',
    field: 'text',
  })
  assert.deepEqual(invalid, {
    ok: false,
    code: 'invalid_input',
    field: 'text',
    message: 'bad',
  })
  assert.deepEqual(toOutcomeWire({ ok: true, result: { done: true } }), {
    ok: true,
    result: { done: true },
  })
})

test('toOutcomeWire passes the candidates of needs_choice on to Rust', () => {
  const options = [
    { value: 'extension.mail', label: 'haex-mail' },
    {
      value: 'extension.files',
      label: 'haex-files',
      unavailable: 'Wird übertragen',
    },
  ]
  assert.deepEqual(
    toOutcomeWire({
      ok: false,
      code: 'needs_choice',
      message: 'no app matches haex unambiguously',
      field: 'appId',
      options,
    }),
    {
      ok: false,
      code: 'needs_choice',
      field: 'appId',
      message: 'no app matches haex unambiguously',
      options,
    },
  )
})

test('every guardrail action is refused for the built-in agent before its handler runs (FR-003, SC-003)', async () => {
  const guardrails = ALL_ACTIONS.filter((a) => a.scope === 'guardrails')
  assert.ok(guardrails.length > 0)
  for (const action of guardrails) {
    const input = sample(action.input) as Record<string, unknown>
    if (action.target !== 'none') input[TARGET_FIELD[action.target]] = 'target'
    for (const caller of [BUILTIN, EXTERNAL]) {
      const calls: string[] = []
      const outcome = await catalogRunner(calls).runAction(
        action.id,
        input,
        caller,
      )
      assert.equal(
        !outcome.ok && outcome.code,
        'forbidden_for_agents',
        action.id,
      )
      assert.equal(calls.length, 0, `${action.id} ran`)
    }
  }
})

const SECRET_NAME =
  /secret|private|password|passphrase|token|apiKey|credential/i

/** Names allowed to match, each with the reason it is no secret. Keep this short. */
const SECRET_NAME_ALLOWED: Record<string, string> = {
  tokenizerRepo:
    'a Hugging Face repository id of a tokenizer, not a credential',
}

function propertyNames(schema: JsonSchema): string[] {
  return [
    ...Object.entries(schema.properties ?? {}).flatMap(([name, child]) => [
      name,
      ...propertyNames(child),
    ]),
    ...(schema.items ? propertyNames(schema.items) : []),
  ]
}

test('no input or result schema of an agent-callable action names a secret (FR-010, SC-004)', () => {
  for (const action of ALL_ACTIONS.filter(isBuiltinAgentAction)) {
    for (const [kind, schema] of [
      ['input', action.input],
      ['result', action.result],
    ] as const) {
      for (const name of propertyNames(schema)) {
        if (name in SECRET_NAME_ALLOWED) continue
        assert.doesNotMatch(
          name,
          SECRET_NAME,
          `${action.id} ${kind} has a field named ${name}`,
        )
      }
    }
  }
})

test('settings.devices.identity exposes the public key and nothing else', () => {
  const action = ALL_ACTIONS.find((a) => a.id === 'settings.devices.identity')
  assert.ok(action && isBuiltinAgentAction(action))
  assert.deepEqual(Object.keys(action.result.properties ?? {}).sort(), [
    'hex',
    'npub',
  ])
})

test('chat.messages.list shows an agent no text, neither as content nor inside another field (SC-004)', () => {
  const secret = 'sk-ant-api03-this-is-a-secret'
  const rows = [
    {
      id: 'm1',
      role: 'user',
      content: `my key is ${secret}`,
      createdAt: 1,
      finishReason: null,
      toolInput: JSON.stringify({ key: secret }),
    },
    {
      id: 'm2',
      role: 'assistant',
      content: 'ok',
      createdAt: 2,
      finishReason: 'complete',
      toolInput: null,
    },
  ]
  const projected = agentSafeMessages(rows)
  assert.deepEqual(projected, {
    messages: [
      { id: 'm1', role: 'user', createdAt: 1, status: null },
      { id: 'm2', role: 'assistant', createdAt: 2, status: 'complete' },
    ],
  })
  assert.ok(!JSON.stringify(projected).includes(secret))
  assert.ok(!JSON.stringify(projected).includes('content'))
})

test('describeAction names the action, its target and its inputs for the approval dialog (FR-008)', () => {
  const close = ALL_ACTIONS.find((a) => a.id === 'wm.tab.close')
  assert.ok(close)
  const resolve = (kind: string, id: string) =>
    kind === 'tab' && id === 't1' ? 'Settings' : undefined
  const known = describeAction(close, { tabId: 't1' }, resolve)
  assert.equal(known.titleKey, close.titleKey)
  assert.deepEqual(known.target, { kind: 'tab', label: 'Settings' })
  assert.deepEqual(known.inputs, [])
  // A target that does not exist keeps its id as the label rather than hiding it.
  assert.deepEqual(describeAction(close, { tabId: 'gone' }, resolve).target, {
    kind: 'tab',
    label: 'gone',
  })
  // No target given: none is shown.
  assert.equal(describeAction(close, {}, resolve).target, undefined)
  // Other inputs become fields; long values are cut.
  const rename = ALL_ACTIONS.find((a) => a.id === 'chat.conversation.rename')
  assert.ok(rename)
  const long = describeAction(
    rename,
    { threadId: 'x', title: 'a'.repeat(300) },
    resolve,
  )
  assert.equal(long.inputs.length, 2)
  assert.equal(long.inputs[1]?.[1].length, 201)
})

test('describeToolCall falls back for a name that is no offered action', () => {
  const resolve = () => undefined
  assert.equal(
    describeToolCall('run_command', {}, ALL_ACTIONS, resolve),
    undefined,
  )
  assert.equal(
    describeToolCall('find_actions', {}, ALL_ACTIONS, resolve),
    undefined,
  )
  assert.equal(
    describeToolCall('chat_message_send', { text: 'x' }, ALL_ACTIONS, resolve),
    undefined,
  )
  assert.ok(describeToolCall('settings_get', null, ALL_ACTIONS, resolve))
})

const EVAL_DIR = new URL('../src-tauri/src/chat/eval/', import.meta.url)

type EvalExpect = 'none' | { tool: string; args?: Record<string, unknown> }[]
type EvalSentence = {
  id: string
  lang: string
  kind: string
  text: string
  expect: EvalExpect
  selfTest?: boolean
}

function evalSet(): { version: number; sentences: EvalSentence[] } {
  return JSON.parse(readFileSync(new URL('eval_set.json', EVAL_DIR), 'utf8'))
}

type EvalExtension = { name: string; title: string; tools: { name: string }[] }

/** The model-facing names `x_<slug>_<tool>` of the extensions (spec 018 R8, R14). */
function extensionToolNames(): string[] {
  const { extensions } = JSON.parse(
    readFileSync(new URL('extension_tools.json', EVAL_DIR), 'utf8'),
  ) as { extensions: EvalExtension[] }
  const slug = (name: string) =>
    name.replace(/[^a-z0-9]/g, '').slice(0, 20) || 'ext'
  return extensions.flatMap((e) =>
    e.tools.map((tool) => `x_${slug(e.name)}_${tool.name}`),
  )
}

test('the tool snapshot of the evaluation is the catalog as it is (run pnpm export:eval-tools)', () => {
  const snapshot = JSON.parse(readFileSync(TOOLS_PATH, 'utf8'))
  assert.deepEqual(snapshot, JSON.parse(JSON.stringify(evalTools())))
})

test('every sentence of the evaluation set is well formed and names tools of the snapshot', () => {
  const { version, sentences } = evalSet()
  const tools = evalTools()
  assert.equal(version, 5)
  assert.equal(new Set(sentences.map((x) => x.id)).size, sentences.length)
  const extensionTools = new Set(extensionToolNames())
  const runtimeIds = new Set([...Object.values(TARGET_FIELD), 'threadId'])
  for (const sentence of sentences) {
    assert.ok(['de', 'en'].includes(sentence.lang), sentence.id)
    assert.ok(
      ['read', 'change', 'smalltalk', 'clarify', 'extension'].includes(
        sentence.kind,
      ),
      sentence.id,
    )
    assert.ok(sentence.text.trim().length > 0, sentence.id)
    if (sentence.expect === 'none') {
      assert.equal(sentence.kind, 'smalltalk', sentence.id)
      continue
    }
    assert.notEqual(sentence.kind, 'smalltalk', sentence.id)
    assert.ok(sentence.expect.length > 0, sentence.id)
    for (const call of sentence.expect) {
      // `ask_user` is a built-in tool of the chat turn, not an action of the snapshot (spec 046).
      assert.ok(
        call.tool === 'ask_user' ||
          (sentence.kind === 'extension'
            ? extensionTools.has(call.tool)
            : tools.some((tool) => tool.toolName === call.tool)),
        `${sentence.id}: ${call.tool} is not in the snapshot or the extension tools`,
      )
      for (const field of Object.keys(call.args ?? {}))
        assert.ok(
          !runtimeIds.has(field),
          `${sentence.id}: ${field} is a runtime id`,
        )
    }
  }
})

test('the evaluation set covers both languages, smalltalk and tools outside the core offer', () => {
  const { sentences } = evalSet()
  const tools = evalTools()
  const count = (pick: (x: EvalSentence) => boolean) =>
    sentences.filter(pick).length
  assert.ok(count((x) => x.kind === 'smalltalk') >= 6)
  const outsideCore = count(
    (x) =>
      x.expect !== 'none' &&
      x.expect.some((c) => !tools.find((t) => t.toolName === c.tool)?.core),
  )
  assert.ok(outsideCore >= 8, `${outsideCore} sentences need the search`)
  const selfTest = sentences.filter((x) => x.selfTest)
  assert.ok(selfTest.length >= 5 && selfTest.length <= 6, `${selfTest.length}`)
  assert.deepEqual([...new Set(selfTest.map((x) => x.lang))].sort(), [
    'de',
    'en',
  ])
  assert.ok(selfTest.some((x) => x.kind === 'smalltalk'))
})

test('the extensions are thirty and have the declared tool counts', () => {
  const { extensions } = JSON.parse(
    readFileSync(new URL('extension_tools.json', EVAL_DIR), 'utf8'),
  ) as { extensions: EvalExtension[] }
  assert.equal(extensions.length, 30)
  for (const extension of extensions) {
    if (extension.name === 'mail') {
      assert.equal(extension.tools.length, 15, extension.name)
    } else {
      assert.ok(
        extension.tools.length >= 2 && extension.tools.length <= 4,
        extension.name,
      )
    }
  }
  const names = extensionToolNames()
  assert.equal(new Set(names).size, names.length)
  assert.ok(names.includes('x_mail_list_mailboxes'))
  const extensionSentences = evalSet().sentences.filter(
    (x) => x.kind === 'extension',
  )
  assert.ok(extensionSentences.length >= 10)
})

test('the browser bridge settings are guardrails no agent can call (spec 049 FR-013)', () => {
  for (const id of [
    'settings.browserBridge.set',
    'settings.browserBridge.client.update',
    'settings.browserBridge.client.revoke',
    'settings.browserBridge.client.unblock',
  ]) {
    const action = ALL_ACTIONS.find((a) => a.id === id)
    assert.equal(action?.scope, 'guardrails', id)
    assert.equal(action?.agentCallable, false, id)
  }
})

test('agents manage file shares in Rust, under a permission of their own (spec 056 FR-045)', () => {
  const shares = ALL_ACTIONS.filter((a) => a.id.startsWith('files.shares.'))
  assert.equal(shares.length, 6)
  for (const action of shares) {
    assert.equal(action.scope, 'files.shares', action.id)
    assert.equal(action.agentCallable, true, action.id)
    assert.equal(action.runner, 'native', action.id)
  }
})

test('every browser tool runs in Rust and asks itself when it changes a page (spec 065 R8)', () => {
  for (const action of ALL_ACTIONS.filter((a) => a.scope === 'browser')) {
    assert.equal(action.runner, 'native', action.id)
    assert.ok(action.agentCallable, action.id)
    if (action.effect !== 'read')
      assert.equal(action.confirmsItself, true, action.id)
  }
})

test('the browser tools that ask themselves are those the executor asks for (spec 065 R8)', () => {
  const source = readFileSync(
    new URL('../src-tauri/src/agent_browser/executor.rs', import.meta.url),
    'utf8',
  )
  const changing = /CHANGING: &\[&str\] = &\[([^\]]*)\]/.exec(source)?.[1] ?? ''
  const ids = [...changing.matchAll(/"([^"]+)"/g)].map((match) => match[1])
  assert.deepEqual(
    ALL_ACTIONS.filter((a) => a.scope === 'browser' && a.confirmsItself)
      .map((a) => a.id)
      .sort(),
    ids.sort(),
  )
})

test('a device without an agent browser offers no browser tools (spec 065 R13)', () => {
  const without = forDevice(ALL_ACTIONS, false)
  assert.ok(!without.some((a) => a.scope === 'browser'))
  assert.equal(
    without.length,
    ALL_ACTIONS.filter((a) => a.scope !== 'browser').length,
  )
  assert.equal(forDevice(ALL_ACTIONS, true).length, ALL_ACTIONS.length)
})

test('the settings of the agent browser are guardrails (spec 065)', () => {
  for (const action of ALL_ACTIONS.filter((a) =>
    a.id.startsWith('settings.agentBrowser.'),
  )) {
    assert.equal(action.scope, 'guardrails', action.id)
    assert.equal(action.agentCallable, false, action.id)
  }
})
