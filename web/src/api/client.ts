import type { components } from './schema'

export type Health = components['schemas']['Health']

async function get<T>(path: string): Promise<T> {
  const res = await fetch(path)
  if (!res.ok) {
    throw new Error(`GET ${path}: ${res.status}`)
  }
  return res.json() as Promise<T>
}

export const api = {
  health: () => get<Health>('/api/health'),
}
