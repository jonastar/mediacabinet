import assert from 'node:assert/strict'
import { test } from 'node:test'
import { episodeLabel, episodeLine } from './episode.ts'

test('a number follows the ordering it comes from', () => {
  assert.equal(episodeLabel({ style: 'season', season: 1, number: 3 }), 'S01E03')
  assert.equal(episodeLabel({ style: 'absolute', season: null, number: 3 }), '03')
  assert.equal(episodeLabel({ style: 'playlist', season: null, number: 3 }), 'E03')
})

test('an episode line leaves out a number it does not have', () => {
  const number = { style: 'season' as const, season: 1, number: 3 }
  assert.equal(episodeLine({ number, title: 'Rabbot' }), 'S01E03 · Rabbot')
  assert.equal(episodeLine({ number: null, title: 'Rabbot' }), 'Rabbot')
  assert.equal(
    episodeLine({ number: { style: 'season', season: 1, number: null }, title: 'Rabbot' }),
    'Rabbot',
  )
})
