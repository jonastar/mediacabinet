#!/usr/bin/env node
// Generates the native players' API models from web/src/api/openapi.json, which
// `pnpm api:types` writes. The results are committed. With --check, changes
// nothing and fails if any is out of date.
//
// Only the models are generated. Each app writes the few calls it makes by hand,
// as players/web/src/api.ts does, since the operation IDs are Rust function names.
//
// The spec uses a small part of OpenAPI 3.1, and anything outside it fails here
// rather than producing a wrong type: objects, `allOf` to merge objects, `oneOf`
// with `null` for an optional reference, string enums, arrays, maps and free-form
// objects.

import { mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = join(dirname(fileURLToPath(import.meta.url)), '..')
const spec = JSON.parse(readFileSync(join(root, 'web/src/api/openapi.json'), 'utf8'))
const schemas = spec.components.schemas
const NOTICE =
  'Generated from web/src/api/openapi.json by scripts/generate-api-clients.mjs. Do not edit.'

const refName = (ref) => ref.replace('#/components/schemas/', '')

/** `episode_id` as `episodeId`. */
const camel = (name) => name.replace(/_([a-z0-9])/g, (_, letter) => letter.toUpperCase())

/** `not_acquired` as `NotAcquired`, for enum constants. */
const pascal = (name) => {
  const id = camel(name)
  return id.charAt(0).toUpperCase() + id.slice(1)
}

/** A schema's type without `null`, and whether `null` was allowed. */
function nonNull(schema) {
  if (schema.oneOf) {
    const rest = schema.oneOf.filter((s) => s.type !== 'null')
    if (rest.length !== 1 || rest.length === schema.oneOf.length) {
      throw new Error(`unsupported oneOf: ${JSON.stringify(schema)}`)
    }
    return {
      schema: { ...rest[0], description: schema.description ?? rest[0].description },
      nullable: true,
    }
  }
  if (Array.isArray(schema.type)) {
    const types = schema.type.filter((t) => t !== 'null')
    if (types.length !== 1) {
      throw new Error(`unsupported type list: ${JSON.stringify(schema.type)}`)
    }
    return { schema: { ...schema, type: types[0] }, nullable: types.length !== schema.type.length }
  }
  return { schema, nullable: false }
}

/** An object schema's properties and required names, with `allOf` merged in. */
function members(schema) {
  if (schema.allOf) {
    const merged = { properties: {}, required: [] }
    for (const part of schema.allOf) {
      const { properties, required } = members(part.$ref ? schemas[refName(part.$ref)] : part)
      Object.assign(merged.properties, properties)
      merged.required.push(...required)
    }
    return merged
  }
  return { properties: schema.properties ?? {}, required: schema.required ?? [] }
}

const isObject = (schema) => Boolean(schema.allOf || schema.properties)
const isEnum = (schema) => Boolean(schema.enum)

// Kotlin

const KOTLIN_KEYWORDS = new Set(
  'as break class continue do else false for fun if in interface is null object package return super this throw true try typealias typeof val var when while'.split(
    ' ',
  ),
)

const kotlinName = (name) => (KOTLIN_KEYWORDS.has(name) ? `\`${name}\`` : name)

function kotlinType(schema, where) {
  if (schema.$ref) {
    return refName(schema.$ref)
  }
  switch (schema.type) {
    case 'string':
      return 'String'
    case 'boolean':
      return 'Boolean'
    case 'integer':
      return schema.format === 'int32' ? 'Int' : 'Long'
    case 'number':
      return 'Double'
    case 'array':
      return `List<${kotlinType(itemType(tupleItems(schema, where), where), where)}>`
    case 'object':
      if (schema.properties) {
        throw new Error(`${where}: inline objects need a named schema`)
      }
      if (schema.additionalProperties && schema.additionalProperties !== true) {
        return `Map<String, ${kotlinType(itemType(schema.additionalProperties, where), where)}>`
      }
      return 'JsonObject'
    default:
      throw new Error(`${where}: unsupported schema ${JSON.stringify(schema)}`)
  }
}

/** An array's items; a tuple (`prefixItems`, as Rust's `(String, String)`) is a list of its one type. */
function tupleItems(schema, where) {
  if (!schema.prefixItems) {
    return schema.items
  }
  const types = new Set(schema.prefixItems.map((item) => JSON.stringify(item)))
  if (types.size !== 1) {
    throw new Error(`${where}: tuples of mixed types are unsupported`)
  }
  return schema.prefixItems[0]
}

/** Array items and map values: nullable ones keep the `?`. */
function itemType(schema, where) {
  const { schema: inner, nullable } = nonNull(schema)
  if (nullable) {
    return { $ref: `${kotlinType(inner, where)}?` }
  }
  return inner
}

function kdoc(description, indent) {
  if (!description) {
    return ''
  }
  const lines = description.replaceAll('*/', '*&#47;').trim().split('\n')
  if (lines.length === 1) {
    return `${indent}/** ${lines[0]} */\n`
  }
  return `${indent}/**\n${lines.map((l) => `${indent} *${l ? ` ${l}` : ''}`).join('\n')}\n${indent} */\n`
}

function kotlinClass(name, schema) {
  const { properties, required } = members(schema)
  const fields = Object.entries(properties).map(([prop, raw]) => {
    const { schema: inner, nullable } = nonNull(raw)
    const type = kotlinType(inner, `${name}.${prop}`)
    const id = camel(prop)
    const serial = id === prop ? '' : `@SerialName("${prop}") `
    // Anything that may be missing or null defaults to null, so a response
    // without it still decodes; a request leaves it out (explicitNulls = false).
    // Lists and booleans default to empty and false, as a server older than the
    // app may not send them yet; anything else missing is a real mismatch.
    const optional = nullable || !required.includes(prop)
    let declaration = type
    if (optional) {
      declaration = `${type}? = null`
    } else if (type.startsWith('List<')) {
      declaration = `${type} = emptyList()`
    } else if (type === 'Boolean') {
      declaration = `${type} = false`
    }
    return `${kdoc(raw.description ?? inner.description, '    ')}    ${serial}val ${kotlinName(id)}: ${declaration},`
  })
  const body = fields.length ? `(\n${fields.join('\n')}\n)` : '(val unused: Unit = Unit)'
  const kind = fields.length ? 'data class' : 'class'
  return `${kdoc(schema.description, '')}@Serializable\n${kind} ${name}${body}`
}

/**
 * A string, with the known values as constants. Not an enum class: a server can
 * add a value, and an older app has to keep decoding responses that use it.
 */
function kotlinEnum(name, schema) {
  const constants = schema.enum
    .map((value) => `        val ${pascal(value)} = ${name}("${value}")`)
    .join('\n')
  return `${kdoc(schema.description, '')}@Serializable\n@JvmInline\nvalue class ${name}(val value: String) {\n    companion object {\n${constants}\n    }\n}`
}

function kotlin() {
  const blocks = Object.keys(schemas)
    .sort()
    .map((name) => {
      const schema = schemas[name]
      if (isEnum(schema)) {
        return kotlinEnum(name, schema)
      }
      if (isObject(schema)) {
        return kotlinClass(name, schema)
      }
      throw new Error(`${name}: unsupported top-level schema`)
    })
  return `// ${NOTICE}

package app.mediacabinet.core.api

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonObject

${blocks.join('\n\n')}
`
}

const outputs = {
  'players/android/core/src/main/kotlin/app/mediacabinet/core/api/Models.kt': kotlin(),
}

const check = process.argv.includes('--check')
let stale = false
for (const [path, content] of Object.entries(outputs)) {
  const file = join(root, path)
  let current = null
  try {
    current = readFileSync(file, 'utf8')
  } catch {
    // Missing counts as out of date.
  }
  if (current === content) {
    continue
  }
  if (check) {
    console.error(`${path} is out of date: run 'pnpm api:types' in web/`)
    stale = true
  } else {
    mkdirSync(dirname(file), { recursive: true })
    writeFileSync(file, content)
    console.log(`wrote ${path}`)
  }
}
if (stale) {
  process.exit(1)
}
