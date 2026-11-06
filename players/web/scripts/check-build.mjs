// A passing build doesn't show the app runs on old TVs: the build leaves CSS as
// written, and can keep JavaScript they can't parse. This reads what came out.

import { readdirSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
import { parse, tokenizer, tokTypes } from 'acorn'

const dist = new URL('../dist/', import.meta.url).pathname

/** Features of CSS, with what's the oldest engine they work on (docs/clients.md). */
const CSS_RULES = [
  [/var\(/, 'var() needs Chromium 49: use token()'],
  [/display:\s*(inline-)?grid/, 'grid needs Chromium 57: use flexbox'],
  [/(^|[;{])\s*((row|column)-)?gap\s*:/, 'flexbox gap needs Chromium 84: use margins'],
  [/aspect-ratio/, 'aspect-ratio needs Chromium 88: use fixed sizes'],
  [/:has\(/, ':has() needs Chromium 105'],
  [/:focus-visible/, ':focus-visible needs Chromium 86: use the .focused class'],
  [/color-mix\(/, 'color-mix() needs Chromium 111'],
  [/#[0-9a-f]{8}\b|#[0-9a-f]{4}\b/i, '#RRGGBBAA needs Chromium 62: use rgba() with commas'],
  [/rgba?\([^)]*\//, 'rgb(0 0 0 / 50%) needs Chromium 65: use rgba() with commas'],
  [/(clamp|min|max)\(/, 'clamp(), min() and max() need Chromium 79'],
  [/(^|[;{])\s*inset\s*:/, 'inset needs Chromium 87: use top, right, bottom and left'],
  [/font-variation-settings|wght\b.*woff/, 'variable fonts need Chromium 62: use static files'],
]

function files(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) =>
    entry.isDirectory() ? files(join(dir, entry.name)) : [join(dir, entry.name)],
  )
}

const problems = []

function checkCss(name, css) {
  for (const [rule, why] of CSS_RULES) {
    const found = css.match(rule)
    if (found) {
      problems.push(`${name}: ${why} (found "${found[0].trim()}")`)
    }
  }
}

/** Legacy chunks carry their CSS as a string, so that's where it's read from. */
function cssInside(code) {
  const found = []
  for (const token of tokenizer(code, { ecmaVersion: 5 })) {
    if (token.type === tokTypes.string && /font-family\s*:|@font-face/.test(token.value)) {
      found.push(token.value)
    }
  }
  return found
}

let cssChecked = false
for (const file of files(dist)) {
  const name = file.slice(dist.length)
  if (!file.endsWith('.js')) {
    continue
  }
  const code = readFileSync(file, 'utf8')
  try {
    parse(code, { ecmaVersion: 5, allowHashBang: true })
  } catch (err) {
    problems.push(`${name}: isn't ES5: ${err.message}`)
    continue
  }
  for (const css of cssInside(code)) {
    cssChecked = true
    checkCss(name, css)
  }
}
if (/\scrossorigin\b/.test(readFileSync(join(dist, 'index.html'), 'utf8'))) {
  problems.push("index.html: crossorigin scripts won't load from a file, as packaged apps do")
}
if (!cssChecked) {
  problems.push('no CSS was found in the JavaScript, so none was checked')
}

if (problems.length > 0) {
  console.error(`The build won't run on old TVs:\n${problems.map((p) => `  ${p}`).join('\n')}`)
  process.exit(1)
}
console.log('Build checked: CSS and JavaScript fit old TVs.')
