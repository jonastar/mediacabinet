#!/usr/bin/env node
// Generates each platform's design values from design/tokens.json. The results
// are committed. With --check, changes nothing and fails if any is out of date.

import { mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = join(dirname(fileURLToPath(import.meta.url)), '..')
const tokens = JSON.parse(readFileSync(join(root, 'design/tokens.json'), 'utf8'))
const NOTICE = 'Generated from design/tokens.json by scripts/generate-tokens.mjs. Do not edit.'

/** `state-available-text` as `stateAvailableText`; `4` as `space4`, for languages that need a name. */
const camel = (name) => {
  const id = name.replace(/-([a-z0-9])/g, (_, letter) => letter.toUpperCase())
  return /^\d/.test(id) ? `space${id}` : id
}

const hex = (color) => color.slice(1).toUpperCase()
const entries = (group) => Object.entries(group)
const typeSizes = (form) => entries(tokens.type).map(([name, sizes]) => [name, sizes[form]])

function css() {
  const lines = [
    ...entries(tokens.color).map(([name, value]) => `--${name}: ${value};`),
    '',
    ...entries(tokens.radius).map(([name, value]) => `--radius-${name}: ${value}px;`),
    ...entries(tokens.space).map(([name, value]) => `--space-${name}: ${value}px;`),
    ...typeSizes('web').map(([name, value]) => `--type-${name}: ${value}px;`),
    ...entries(tokens.duration).map(([name, value]) => `--duration-${name}: ${value}ms;`),
  ]
  const body = lines.map((line) => (line ? `  ${line}` : '')).join('\n')
  return `/* ${NOTICE} */\n:root {\n${body}\n}\n`
}

/** Fixed values, since older TVs have no CSS variables. Sizes are at 1920×1080. */
function tv() {
  const object = (group, value = (v) => v) =>
    `{\n${group.map(([name, v]) => `  ${camel(name)}: ${JSON.stringify(value(v))},`).join('\n')}\n}`
  const groups = {
    color: object(entries(tokens.color)),
    radius: object(entries(tokens.radius)),
    space: object(entries(tokens.space)),
    type: object(typeSizes('tv')),
    duration: object(entries(tokens.duration)),
    layout: object(entries(tokens.tv)),
  }
  const exports = entries(groups)
    .map(([name, value]) => `export const ${name} = ${value}`)
    .join('\n\n')
  return `// ${NOTICE}\n\n${exports}\n`
}

function swift() {
  const number = (type) => (group) =>
    group.map(([name, v]) => `        static let ${camel(name)}: ${type} = ${v}`).join('\n')
  const block = (name, body) => `    enum ${name} {\n${body}\n    }`
  const colors = entries(tokens.color)
    .map(([name, v]) => `        static let ${camel(name)}: UInt32 = 0x${hex(v)}`)
    .join('\n')
  const blocks = [
    block('Color', colors),
    block('Radius', number('Double')(entries(tokens.radius))),
    block('Space', number('Double')(entries(tokens.space))),
    block('PhoneType', number('Double')(typeSizes('phone'))),
    block('TVType', number('Double')(typeSizes('tv'))),
    block('DurationMs', number('Double')(entries(tokens.duration))),
    block('TVLayout', number('Double')(entries(tokens.tv))),
  ]
  return `// ${NOTICE}\n// Colours are 0xRRGGBB. TV sizes are at 1920×1080, one-to-one on tvOS.\n\npublic enum Tokens {\n${blocks.join('\n\n').replaceAll('static let', 'public static let').replaceAll('    enum', '    public enum')}\n}\n`
}

function kotlin() {
  const number = (group) =>
    group.map(([name, v]) => `        const val ${camel(name)} = ${v}`).join('\n')
  const block = (name, body) => `    object ${name} {\n${body}\n    }`
  const colors = entries(tokens.color)
    .map(([name, v]) => `        const val ${camel(name)} = 0xFF${hex(v)}`)
    .join('\n')
  const blocks = [
    block('Color', colors),
    block('Radius', number(entries(tokens.radius))),
    block('Space', number(entries(tokens.space))),
    block('PhoneType', number(typeSizes('phone'))),
    block('TvType', number(typeSizes('tv'))),
    block('DurationMs', number(entries(tokens.duration))),
    block('TvLayout', number(entries(tokens.tv))),
  ]
  return `// ${NOTICE}\n// Colours are 0xAARRGGBB, as Compose's Color() takes them. TV sizes are at\n// 1920×1080: halve them on Android TV, which lays out at 960×540 dp.\n\npackage app.mediacabinet.core\n\nobject Tokens {\n${blocks.join('\n\n')}\n}\n`
}

const outputs = {
  'web/src/tokens.css': css(),
  'players/web/src/tokens.ts': tv(),
  'players/apple/Core/Sources/Core/Tokens.swift': swift(),
  'players/android/core/src/main/kotlin/app/mediacabinet/core/Tokens.kt': kotlin(),
}

const check = process.argv.includes('--check')
let stale = false
for (const [path, content] of Object.entries(outputs)) {
  const file = join(root, path)
  if (check) {
    let current = null
    try {
      current = readFileSync(file, 'utf8')
    } catch {
      // Missing counts as out of date.
    }
    if (current !== content) {
      console.error(`${path} is out of date`)
      stale = true
    }
  } else {
    mkdirSync(dirname(file), { recursive: true })
    writeFileSync(file, content)
  }
}
if (stale) {
  console.error("Run 'pnpm tokens' in web/.")
  process.exit(1)
}
