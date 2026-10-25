// `pnpm export:eval-tools` (spec 032 T042): writes the snapshot of the tools the built-in agent
// is offered to `src-tauri/src/chat/eval/tools.json`. The evaluation of a model's tool use
// (`chat/eval/`) runs in Rust without a webview, so it reads this snapshot instead of the
// catalog; `check:agent-actions` fails when the snapshot differs from what this script builds.
import { readFileSync, writeFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import * as prettier from 'prettier'

import {
  listAgentActions,
  type AgentActionDef,
  type TitleOf,
} from '../src/lib/actions/agentTools.ts'
import { ALL_ACTIONS } from '../src/lib/actions/catalog.ts'

export const TOOLS_PATH = fileURLToPath(
  new URL('../src-tauri/src/chat/eval/tools.json', import.meta.url),
)

function locale(name: 'de' | 'en'): unknown {
  const path = new URL(`../src/i18n/locales/${name}.json`, import.meta.url)
  return JSON.parse(readFileSync(path, 'utf8'))
}

const MESSAGES = { de: locale('de'), en: locale('en') }

/** Resolves a nested key such as `actions.wm.tab.close` in the locale files. */
const titleOf: TitleOf = (key, name) => {
  let node: unknown = MESSAGES[name]
  for (const part of key.split('.')) {
    if (typeof node !== 'object' || node === null) return key
    node = (node as Record<string, unknown>)[part]
  }
  return typeof node === 'string' ? node : key
}

/** What the snapshot has to contain: the same list the frontend pushes with `set_agent_actions`. */
export function evalTools(): AgentActionDef[] {
  return listAgentActions(ALL_ACTIONS, titleOf)
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const formatted = await prettier.format(JSON.stringify(evalTools()), {
    parser: 'json',
  })
  writeFileSync(TOOLS_PATH, formatted)
  console.log(`wrote ${TOOLS_PATH}`)
}
