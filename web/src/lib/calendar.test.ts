import assert from 'node:assert/strict'
import { test } from 'node:test'
import { addMonths, airDay, firstWeekday, monthGrid, monthParam, parseMonth } from './calendar.ts'

const TODAY = new Date(2026, 10, 8)

test('reads and writes months', () => {
  assert.deepEqual(parseMonth('2027-01', TODAY), { year: 2027, month: 0 })
  assert.equal(monthParam({ year: 2027, month: 0 }), '2027-01')
})

test('falls back to the current month', () => {
  for (const bad of [null, '', '2026-13', '2026-00', 'soon']) {
    assert.deepEqual(parseMonth(bad, TODAY), { year: 2026, month: 10 })
  }
})

test('moves across years', () => {
  assert.deepEqual(addMonths({ year: 2026, month: 11 }, 1), { year: 2027, month: 0 })
  assert.deepEqual(addMonths({ year: 2026, month: 0 }, -1), { year: 2025, month: 11 })
})

test('lays out whole weeks from the first weekday', () => {
  // November 2026 starts on a Sunday and ends on a Monday.
  const monday = monthGrid({ year: 2026, month: 10 }, 1)
  assert.equal(monday.length, 6)
  assert.deepEqual(monday[0][0], new Date(2026, 9, 26))
  assert.deepEqual(monday[0][6], new Date(2026, 10, 1))
  assert.deepEqual(monday[5][6], new Date(2026, 11, 6))

  const sunday = monthGrid({ year: 2026, month: 10 }, 0)
  assert.equal(sunday.length, 5)
  assert.deepEqual(sunday[0][0], new Date(2026, 10, 1))
  assert.ok(sunday.every((w) => w.length === 7))
})

test('a February that starts on the first weekday fits four weeks', () => {
  assert.equal(monthGrid({ year: 2027, month: 1 }, 1).length, 4)
})

test('knows where weeks start', () => {
  assert.equal(firstWeekday('en-GB'), 1)
  assert.equal(firstWeekday('en-US'), 0)
})

test('dates without a time keep their date', () => {
  const endOfDayUtc = Date.UTC(2026, 10, 14, 23, 59, 59) / 1000
  assert.deepEqual(airDay(endOfDayUtc, false), new Date(2026, 10, 14))
})
