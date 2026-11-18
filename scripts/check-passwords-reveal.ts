// Part of `pnpm check:passwords`: the reveal control is a real toggle, the value stays shown
// until it is switched off again.
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'

import { loadScriptSetup } from './lib/script-setup-sandbox.ts'

const revealed = 'a-password-long-enough-to-change-the-row-layout'

function loadMaskedValue() {
  return loadScriptSetup<{
    value: { value: string | null }
    toggle: () => void
  }>('src/components/passwords/MaskedValue.vue', ['value', 'toggle'], {
    props: {
      fetch: async () => revealed,
      identity: 'entry:password',
      present: true,
      label: 'Password',
      kind: 'password',
    },
    useErrorString: () => ({ errString: () => 'error' }),
    imports: { 'vue-sonner': { toast: { error: () => {} } } },
  })
}

async function settle() {
  await Promise.resolve()
  await Promise.resolve()
}

test('one toggle shows the value and it stays until the next toggle hides it', async () => {
  const component = loadMaskedValue()

  component.toggle()
  await settle()
  assert.equal(component.value.value, revealed)

  // Nothing but the toggle hides it: no pointer release, no leaving the row.
  await settle()
  assert.equal(component.value.value, revealed)

  component.toggle()
  assert.equal(component.value.value, null)
})

test('the template toggles on click and holds no pointer handlers', () => {
  const source = readFileSync(
    'src/components/passwords/MaskedValue.vue',
    'utf8',
  )
  assert.match(source, /@click="toggle"/)
  assert.doesNotMatch(source, /@pointer|@lostpointercapture/)
})
