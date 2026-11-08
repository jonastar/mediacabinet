import assert from 'node:assert/strict'
import { test } from 'node:test'
import { isStarted, minutesLeft, progress } from './progress.ts'

test('progress runs from 0 to 1, and is full once played', () => {
  assert.equal(progress(1200, 300, false), 0.25)
  assert.equal(progress(1200, 1500, false), 1)
  assert.equal(progress(1200, 0, false), 0)
  assert.equal(progress(null, 300, false), 0)
  assert.equal(progress(null, 0, true), 1)
})

test('minutes left round to whole minutes, never below one', () => {
  assert.equal(minutesLeft(1200, 300), 15)
  assert.equal(minutesLeft(1200, 1190), 1)
  assert.equal(minutesLeft(1200, 0), null)
  assert.equal(minutesLeft(null, 300), null)
})

test('only something begun and not finished counts as started', () => {
  assert.equal(isStarted({ played: false, position_seconds: 30 }), true)
  assert.equal(isStarted({ played: true, position_seconds: 30 }), false)
  assert.equal(isStarted({ played: false, position_seconds: 0 }), false)
  assert.equal(isStarted(null), false)
})
