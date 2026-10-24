// Validator for the JSON-schema subset actions use (spec 020-tab-navigation, T012, research R8):
// `object` (strict when `properties` is declared; otherwise any object), `properties`, `required`, `string`, `number`,
// `integer`, `boolean`, `array` with `items`, `enum`, `description`. No dependency; anything
// outside the subset is rejected by `isSchemaInSubset` so the catalog cannot drift into it.
import type { JsonSchema } from './types.ts'

export type ValidationResult =
  { ok: true } | { ok: false; field: string; message: string }

const ALLOWED_KEYS = new Set([
  'type',
  'description',
  'properties',
  'required',
  'items',
  'enum',
  'nullable',
])
const ALLOWED_TYPES = new Set([
  'object',
  'string',
  'number',
  'integer',
  'boolean',
  'array',
])

export function isSchemaInSubset(schema: JsonSchema): boolean {
  if (typeof schema !== 'object' || schema === null) return false
  if (Object.keys(schema).some((key) => !ALLOWED_KEYS.has(key))) return false
  if (!ALLOWED_TYPES.has(schema.type)) return false
  if (schema.properties) {
    if (schema.type !== 'object') return false
    if (!Object.values(schema.properties).every(isSchemaInSubset)) return false
  }
  if (schema.required) {
    const declared = schema.properties ?? {}
    if (!schema.required.every((key) => key in declared)) return false
  }
  if (
    schema.items &&
    (schema.type !== 'array' || !isSchemaInSubset(schema.items))
  )
    return false
  return true
}

function fail(field: string, message: string): ValidationResult {
  return { ok: false, field, message }
}

function typeMatches(schema: JsonSchema, value: unknown): boolean {
  if (value === null) return schema.nullable === true
  switch (schema.type) {
    case 'object':
      return (
        typeof value === 'object' && value !== null && !Array.isArray(value)
      )
    case 'array':
      return Array.isArray(value)
    case 'integer':
      return Number.isInteger(value)
    case 'number':
      return typeof value === 'number' && Number.isFinite(value)
    case 'string':
      return typeof value === 'string'
    case 'boolean':
      return typeof value === 'boolean'
  }
}

function validateAt(
  schema: JsonSchema,
  value: unknown,
  field: string,
): ValidationResult {
  if (!typeMatches(schema, value)) return fail(field, `expected ${schema.type}`)
  if (schema.enum && !schema.enum.includes(value as string | number))
    return fail(field, `expected one of ${schema.enum.join(', ')}`)
  if (schema.type === 'array' && schema.items) {
    const items = value as unknown[]
    for (let i = 0; i < items.length; i++) {
      const result = validateAt(schema.items, items[i], `${field}[${i}]`)
      if (!result.ok) return result
    }
  }
  if (schema.type === 'object') {
    const record = value as Record<string, unknown>
    const properties = schema.properties ?? {}
    const prefix = field === '' ? '' : `${field}.`
    if (schema.properties) {
      for (const key of Object.keys(record)) {
        if (!(key in properties))
          return fail(`${prefix}${key}`, 'unknown property')
      }
    }
    for (const key of schema.required ?? []) {
      if (record[key] === undefined) return fail(`${prefix}${key}`, 'required')
    }
    for (const [key, child] of Object.entries(properties)) {
      if (record[key] === undefined) continue
      const result = validateAt(child, record[key], `${prefix}${key}`)
      if (!result.ok) return result
    }
  }
  return { ok: true }
}

export function validate(schema: JsonSchema, value: unknown): ValidationResult {
  return validateAt(schema, value, '')
}
