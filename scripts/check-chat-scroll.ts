// `pnpm check:chat-state`: the transcript follows a reply only while the reader is at its end.
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  BOTTOM_TOLERANCE_PX,
  isAtBottom,
} from '../src/lib/chat/scrollFollow.ts'

const view = (scrollTop: number) => ({
  scrollHeight: 2000,
  scrollTop,
  clientHeight: 600,
})

test('the end of the content counts as the bottom', () => {
  assert.equal(isAtBottom(view(1400)), true)
})

test('a few pixels short of the end still count, a rounding error must not lose the end', () => {
  assert.equal(isAtBottom(view(1400 - BOTTOM_TOLERANCE_PX)), true)
  assert.equal(isAtBottom(view(1399.5)), true)
})

test('scrolling up beyond the tolerance leaves the bottom', () => {
  assert.equal(isAtBottom(view(1400 - BOTTOM_TOLERANCE_PX - 1)), false)
  assert.equal(isAtBottom(view(0)), false)
})

test('content that fits the view is at its end', () => {
  assert.equal(
    isAtBottom({ scrollHeight: 300, scrollTop: 0, clientHeight: 600 }),
    true,
  )
})
