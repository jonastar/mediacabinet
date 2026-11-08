import assert from 'node:assert/strict'
import { test } from 'node:test'
import { englishLocale } from './locale.ts'

test('keeps an English locale the browser asks for', () => {
  assert.equal(englishLocale(['nb-NO', 'en-US', 'en']), 'en-US')
  assert.equal(englishLocale(['en']), 'en')
})

test('falls back to British English for other languages', () => {
  assert.equal(englishLocale(['nb-NO', 'nn', 'no']), 'en-GB')
  assert.equal(englishLocale([]), 'en-GB')
})
