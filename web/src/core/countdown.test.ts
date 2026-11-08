import assert from 'node:assert/strict'
import { test } from 'node:test'
import { countdown } from './countdown.ts'

const NOW = 1_800_000_000
const at = (days: number, hours: number, minutes: number) =>
  NOW + days * 86400 + hours * 3600 + minutes * 60

test('counts down in the largest two units', () => {
  assert.deepEqual(countdown(at(1, 2, 5), true, NOW), {
    long: 'in 1 day and 2 hours',
    short: 'in 1d 2h',
  })
  assert.deepEqual(countdown(at(0, 3, 12), true, NOW), {
    long: 'in 3 hours and 12 minutes',
    short: 'in 3h 12m',
  })
  assert.deepEqual(countdown(at(0, 0, 12), true, NOW), { long: 'in 12 minutes', short: 'in 12m' })
})

test('plurals follow the number', () => {
  assert.equal(countdown(at(2, 1, 0), true, NOW).long, 'in 2 days and 1 hour')
  assert.equal(countdown(at(0, 0, 1), true, NOW).long, 'in 1 minute')
})

test('a time that has passed is airing now', () => {
  assert.deepEqual(countdown(NOW, true, NOW), { long: 'Airing now', short: 'Now' })
  assert.deepEqual(countdown(NOW - 60, true, NOW), { long: 'Airing now', short: 'Now' })
})

test('a date with no time counts whole days', () => {
  // Stored as the end of the day in UTC.
  const endOfDay = (Math.floor(NOW / 86400) + 3) * 86400 + 86399
  const result = countdown(endOfDay, false, NOW)
  assert.match(result.long, /^in [234] days$/)
  assert.doesNotMatch(result.short, /h|m/)
  assert.deepEqual(countdown(NOW - 86400, false, NOW), { long: 'Airing today', short: 'Today' })
})
