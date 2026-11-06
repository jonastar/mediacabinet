import assert from 'node:assert/strict'
import { test } from 'node:test'
import { clock, language } from './labels.ts'

test('languages have names, and unknown or missing ones are handled', () => {
  assert.equal(language('eng'), 'English')
  assert.equal(language('und'), null)
  assert.equal(language(null), null)
  assert.equal(language('zzz'), 'zzz')
})

test('times are written with hours only when there are some', () => {
  assert.equal(clock(245), '4:05')
  assert.equal(clock(3723), '1:02:03')
})
