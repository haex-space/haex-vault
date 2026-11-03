// `pnpm check:chat-state`: the transcript follows a reply only while the reader is at its end.
import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  BOTTOM_TOLERANCE_PX,
  followsAfterScroll,
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

test('any move up stops following, even one that stays within the tolerance', () => {
  // A touchpad step of a few pixels: the next token must not pull the reader back down.
  assert.equal(followsAfterScroll(true, 1400, view(1396)), false)
  assert.equal(followsAfterScroll(true, 1400, view(900)), false)
})

test('reaching the end again follows again', () => {
  assert.equal(followsAfterScroll(false, 900, view(1400)), true)
  assert.equal(
    followsAfterScroll(false, 900, view(1400 - BOTTOM_TOLERANCE_PX)),
    true,
  )
})

test('a move down that stops short of the end keeps not following', () => {
  assert.equal(followsAfterScroll(false, 600, view(900)), false)
})

test('the late event of a jump to the end keeps following after more content arrived', () => {
  // Jumped to 1400 of 2000; before the scroll event fired the content grew by a long line.
  assert.equal(
    followsAfterScroll(true, 1000, {
      scrollHeight: 2100,
      scrollTop: 1400,
      clientHeight: 600,
    }),
    true,
  )
})

test('a view clamped to its end because its content shrank keeps following', () => {
  assert.equal(
    followsAfterScroll(true, 1400, {
      scrollHeight: 1800,
      scrollTop: 1200,
      clientHeight: 600,
    }),
    true,
  )
  // Content that now fits the view clamps to the top.
  assert.equal(
    followsAfterScroll(true, 1400, {
      scrollHeight: 300,
      scrollTop: 0,
      clientHeight: 600,
    }),
    true,
  )
})
