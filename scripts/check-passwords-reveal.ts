// Part of `pnpm check:passwords`: the reveal control keeps a mouse hold alive when the value
// changes the surrounding responsive layout.
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'

import { loadScriptSetup } from './lib/script-setup-sandbox.ts'

type PointerTarget = {
  captured: boolean
  setPointerCapture: (pointerId: number) => void
  hasPointerCapture: (pointerId: number) => boolean
  releasePointerCapture: (pointerId: number) => void
}

type PointerEventType =
  'pointerdown' | 'pointerleave' | 'pointerup' | 'lostpointercapture'

type PointerEventLike = {
  pointerId: number
  pointerType: 'mouse'
  currentTarget: PointerTarget
  type: PointerEventType
}

function pointerEvent(
  target: PointerTarget,
  type: PointerEventType,
): PointerEventLike {
  return {
    pointerId: 1,
    pointerType: 'mouse',
    currentTarget: target,
    type,
  }
}

const revealed = 'a-password-long-enough-to-change-the-row-layout'

function loadMaskedValue() {
  return loadScriptSetup<{
    value: { value: string | null }
    onPointerDown: (event: PointerEventLike) => void
    onPointerEnd: (event: PointerEventLike) => void
  }>(
    'src/components/passwords/MaskedValue.vue',
    ['value', 'onPointerDown', 'onPointerEnd'],
    {
      props: {
        fetch: async () => revealed,
        identity: 'entry:password',
        present: true,
        label: 'Password',
        kind: 'password',
      },
      useErrorString: () => ({ errString: () => 'error' }),
      imports: { 'vue-sonner': { toast: { error: () => {} } } },
    },
  )
}

function captureTarget(): PointerTarget {
  return {
    captured: false,
    setPointerCapture() {
      this.captured = true
    },
    hasPointerCapture() {
      return this.captured
    },
    releasePointerCapture() {
      this.captured = false
    },
  }
}

test('a responsive reflow does not hide a mouse-held revealed value', async () => {
  const component = loadMaskedValue()
  const target = captureTarget()

  component.onPointerDown(pointerEvent(target, 'pointerdown'))
  await Promise.resolve()
  await Promise.resolve()
  assert.equal(component.value.value, revealed)

  component.onPointerEnd(pointerEvent(target, 'pointerleave'))
  assert.equal(component.value.value, revealed)

  component.onPointerEnd(pointerEvent(target, 'pointerup'))
  assert.equal(component.value.value, null)
})

test('a mouse hold that loses its pointer capture without a release hides the value', async () => {
  const component = loadMaskedValue()
  const target = captureTarget()

  component.onPointerDown(pointerEvent(target, 'pointerdown'))
  await Promise.resolve()
  await Promise.resolve()
  assert.equal(component.value.value, revealed)

  // The browser drops the capture (e.g. the window loses the pointer) and no pointerup follows.
  target.captured = false
  component.onPointerEnd(pointerEvent(target, 'lostpointercapture'))
  assert.equal(component.value.value, null)
})

test('the template ends a hold on a lost pointer capture', () => {
  const source = readFileSync(
    'src/components/passwords/MaskedValue.vue',
    'utf8',
  )
  assert.match(source, /@lostpointercapture="onPointerEnd"/)
})
