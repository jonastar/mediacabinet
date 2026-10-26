import { useEffect, useRef } from 'react'

/**
 * One shared connection to the server's event stream (`/api/events`, SSE).
 * Components subscribe to event types and reload their data when they arrive,
 * instead of polling. The browser reconnects by itself, resuming with
 * Last-Event-ID, so nothing is missed while the server restarts.
 */
type Listener = (type: string) => void

const listeners = new Set<Listener>()
let source: EventSource | null = null

/** Every event type the server emits (crates/core/src/events.rs). */
const EVENT_TYPES = [
  'file.added',
  'file.changed',
  'file.ignored',
  'file.missing',
  'file.moved',
  'file.removed',
  'file.restored',
  'file.upgraded',
  'item.added',
  'item.edited',
  'item.refreshed',
  'item.unwanted',
  'item.wanted',
  'library.added',
  'library.removed',
  'link.created',
  'match.needs_review',
  'root.added',
  'root.changed',
  'root.removed',
  'scan.finished',
  'series.moved',
  'series.refreshed',
  'status.changed',
]

function connect() {
  if (source || typeof EventSource === 'undefined') {
    return
  }
  source = new EventSource('/api/events')
  // The stream sends named events (`event: file.added`); each name needs its own listener.
  for (const type of EVENT_TYPES) {
    source.addEventListener(type, () => dispatch(type))
  }
}

function dispatch(type: string) {
  for (const l of listeners) {
    l(type)
  }
}

/** Calls `onEvent` (debounced) whenever an event of one of `types` arrives. */
export function useLiveEvents(types: string[], onEvent: () => void, delayMs = 400) {
  const callback = useRef(onEvent)
  useEffect(() => {
    callback.current = onEvent
  })
  const key = types.join(',')
  useEffect(() => {
    connect()
    const wanted = new Set(key.split(','))
    let timer: number | undefined
    const listener: Listener = (type) => {
      if (wanted.has(type) || wanted.has('*')) {
        window.clearTimeout(timer)
        timer = window.setTimeout(() => callback.current(), delayMs)
      }
    }
    listeners.add(listener)
    return () => {
      listeners.delete(listener)
      window.clearTimeout(timer)
    }
  }, [key, delayMs])
}
