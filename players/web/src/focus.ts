import { useEffect } from 'preact/hooks'
import { platform, type RemoteAction } from './platform'

/**
 * Spatial focus: arrow keys move to the nearest control in that direction. A
 * first version of the rules in `docs/design-language.md`, without remembering
 * the last item used in each rail.
 */

const SELECTOR = 'button:not([disabled]), input'
const ARROWS: Record<number, 'left' | 'up' | 'right' | 'down'> = {
  37: 'left',
  38: 'up',
  39: 'right',
  40: 'down',
}

function controls(): HTMLElement[] {
  return Array.prototype.slice.call(document.querySelectorAll(SELECTOR))
}

/** How far `to` is from `from` going this way, or null if it isn't that way. */
function distance(from: ClientRect, to: ClientRect, way: string): number | null {
  const fromX = from.left + from.width / 2
  const fromY = from.top + from.height / 2
  const toX = to.left + to.width / 2
  const toY = to.top + to.height / 2
  // Straight ahead counts for more than off to the side, so rails stay rails.
  switch (way) {
    case 'left':
      return to.right <= from.left + 1 ? fromX - toX + Math.abs(fromY - toY) * 3 : null
    case 'right':
      return to.left >= from.right - 1 ? toX - fromX + Math.abs(fromY - toY) * 3 : null
    case 'up':
      return to.bottom <= from.top + 1 ? fromY - toY + Math.abs(fromX - toX) : null
    default:
      return to.top >= from.bottom - 1 ? toY - fromY + Math.abs(fromX - toX) : null
  }
}

/** Scrolls whatever holds `el` until it's on screen, with room around it. */
function reveal(el: HTMLElement) {
  const ROOM = 96
  let parent = el.parentElement
  while (parent) {
    const style = window.getComputedStyle(parent)
    const rect = el.getBoundingClientRect()
    const box = parent.getBoundingClientRect()
    if (style.overflowX === 'hidden' && parent.scrollWidth > parent.clientWidth) {
      if (rect.right + ROOM > box.right) {
        parent.scrollLeft += rect.right + ROOM - box.right
      } else if (rect.left - ROOM < box.left) {
        parent.scrollLeft -= box.left - rect.left + ROOM
      }
    }
    if (style.overflowY === 'hidden' && parent.scrollHeight > parent.clientHeight) {
      if (rect.bottom + ROOM > box.bottom) {
        parent.scrollTop += rect.bottom + ROOM - box.bottom
      } else if (rect.top - ROOM < box.top) {
        parent.scrollTop -= box.top - rect.top + ROOM
      }
    }
    parent = parent.parentElement
  }
}

function focus(el: HTMLElement) {
  el.focus()
  reveal(el)
}

function move(way: string) {
  const all = controls()
  const current = document.activeElement as HTMLElement | null
  if (!current || all.indexOf(current) < 0) {
    if (all[0]) {
      focus(all[0])
    }
    return
  }
  const from = current.getBoundingClientRect()
  let best: HTMLElement | null = null
  let bestDistance = Infinity
  for (const el of all) {
    if (el !== current) {
      const d = distance(from, el.getBoundingClientRect(), way)
      if (d !== null && d < bestDistance) {
        best = el
        bestDistance = d
      }
    }
  }
  if (best) {
    focus(best)
  }
}

/** Set by screens that use the arrow keys themselves, such as the player. */
let handlingArrows = false

/** Handles arrow keys for the whole app. */
export function startFocus() {
  document.addEventListener('keydown', (event) => {
    const way = ARROWS[event.keyCode]
    if (!way || handlingArrows) {
      return
    }
    const target = event.target as HTMLElement
    // Left and right move the caret inside a text field that has text.
    if (
      target.tagName === 'INPUT' &&
      (target as HTMLInputElement).value &&
      (way === 'left' || way === 'right')
    ) {
      return
    }
    event.preventDefault()
    move(way)
  })
}

/** Something is always focused: the control marked `data-first`, or else the first. */
export function useInitialFocus(ready: unknown) {
  useEffect(() => {
    const all = controls()
    const marked = all.filter((el) => el.hasAttribute('data-first'))[0]
    const first = marked ?? all[0]
    if (first) {
      focus(first)
    }
  }, [ready])
}

/** Takes the arrow keys while this screen is showing. */
export function useKeys(onKey: (keyCode: number, event: KeyboardEvent) => boolean | void) {
  useEffect(() => {
    handlingArrows = true
    const listener = (event: KeyboardEvent) => {
      if (onKey(event.keyCode, event)) {
        event.preventDefault()
      }
    }
    document.addEventListener('keydown', listener)
    return () => {
      handlingArrows = false
      document.removeEventListener('keydown', listener)
    }
  }, [onKey])
}

/** Calls `handler` for remote buttons with a meaning, such as Back. */
export function useRemote(handler: (action: RemoteAction) => void) {
  useEffect(() => {
    const listener = (event: KeyboardEvent) => {
      const action = platform.keys[event.keyCode]
      if (!action) {
        return
      }
      // Backspace edits a text field; it's only Back elsewhere.
      if (event.keyCode === 8 && (event.target as HTMLElement).tagName === 'INPUT') {
        return
      }
      event.preventDefault()
      handler(action)
    }
    document.addEventListener('keydown', listener)
    return () => document.removeEventListener('keydown', listener)
  }, [handler])
}
