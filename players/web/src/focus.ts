import { useEffect } from 'preact/hooks'
import { platform, type RemoteAction } from './platform'

/**
 * Focus, by the rules in `docs/design-language.md`. Arrow keys move to the nearest
 * control in that direction, and coming back to a rail returns to the last item
 * used in it. A pointer moves the same focus by hovering.
 *
 * Controls are marked up for this:
 * - `data-first`: where focus starts on a screen
 * - `data-key`: names a control, so focus can find it again after a re-render
 * - `data-rail`, on a container: its controls form a rail
 * - `data-default`: where focus lands in a rail with nothing to return to
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

/** The nearest container of `el`, itself included, with this attribute. */
function holder(el: Element | null, attribute: string): Element | null {
  let at = el
  while (at && !at.hasAttribute(attribute)) {
    at = at.parentElement
  }
  return at
}

const railOf = (el: Element) => holder(el, 'data-rail')

/** The usable button that `el` is in, if any. */
function buttonAt(el: Element | null): HTMLButtonElement | null {
  let at = el
  while (at && at.tagName !== 'BUTTON') {
    at = at.parentElement
  }
  return at && !(at as HTMLButtonElement).disabled ? (at as HTMLButtonElement) : null
}

/** The key of the last item focused in each rail, for as long as the app is open. */
const lastInRail: Record<string, string> = {}

/** Where focus should land when moving to `best`, which may be in another rail. */
function landing(current: HTMLElement, best: HTMLElement): HTMLElement {
  const rail = railOf(best)
  if (!rail || rail === railOf(current)) {
    return best
  }
  const last = lastInRail[rail.getAttribute('data-rail') as string]
  const inRail = controls().filter((el) => railOf(el) === rail)
  return (
    inRail.filter((el) => el.getAttribute('data-key') === last)[0] ??
    inRail.filter((el) => el.hasAttribute('data-default'))[0] ??
    best
  )
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
    focus(landing(current, best))
  }
}

/** Set by screens that use the arrow keys themselves, such as the player. */
let handlingArrows = false

/** A pointer is in use, so what only a pointer needs is showing. */
function setPointer(active: boolean) {
  document.documentElement.className = active ? 'pointer' : ''
}

/** Whether focus just moved, or would next move, because of the pointer and not a key. */
export const usingPointer = () => document.documentElement.className === 'pointer'

const pointerListeners: Array<() => void> = []

/** Hears the pointer moving, for screens that show things when it does. */
export function usePointerMove(onMove: () => void) {
  useEffect(() => {
    pointerListeners.push(onMove)
    return () => {
      pointerListeners.splice(pointerListeners.indexOf(onMove), 1)
    }
  }, [onMove])
}

/**
 * LG's Magic Remote, and any mouse: hovering moves focus, so there's only ever
 * one highlighted control, and the wheel scrolls, since nothing here has
 * scrollbars. Pressing an arrow key hides LG's cursor and carries on from there.
 */
function startPointer() {
  let lastX = -1
  let lastY = -1
  document.addEventListener('mousemove', (event) => {
    // Browsers also send this when the page moves under a still pointer, which
    // mustn't take focus from wherever the remote has just moved it.
    if (event.screenX === lastX && event.screenY === lastY) {
      return
    }
    lastX = event.screenX
    lastY = event.screenY
    setPointer(true)
    // Not text fields: focusing one can open the on-screen keyboard.
    const button = buttonAt(event.target as Element)
    if (button && button !== document.activeElement) {
      button.focus({ preventScroll: true })
    }
    for (const listener of pointerListeners.slice()) {
      listener()
    }
  })
  // A click on nothing would drop focus, and something is always focused.
  document.addEventListener('mousedown', (event) => {
    const target = event.target as Element
    if (!buttonAt(target) && target.tagName !== 'INPUT') {
      event.preventDefault()
    }
  })
  document.addEventListener('wheel', (event) => {
    let at = event.target as HTMLElement | null
    while (at) {
      const scrolls =
        window.getComputedStyle(at).overflowY === 'hidden' && at.scrollHeight > at.clientHeight
      if (scrolls) {
        at.scrollTop += event.deltaY
        return
      }
      at = at.parentElement
    }
  })
  // webOS says when its cursor goes away, which it does by itself after a while.
  document.addEventListener('cursorStateChange', (event) => {
    if (!(event as CustomEvent<{ visibility: boolean }>).detail.visibility) {
      setPointer(false)
    }
  })
}

/** Handles arrow keys and the pointer for the whole app. */
export function startFocus() {
  startPointer()
  document.addEventListener('focusin', (event) => {
    const el = event.target as Element
    const rail = railOf(el)
    const key = el.getAttribute('data-key')
    if (rail && key) {
      lastInRail[rail.getAttribute('data-rail') as string] = key
    }
  })
  document.addEventListener('keydown', (event) => {
    setPointer(false)
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

/** Where focus was on a screen, to go back to. */
export interface Place {
  key: string | null
  rail: string | null
}

export function currentPlace(): Place | null {
  const el = document.activeElement
  if (!el || el === document.body) {
    return null
  }
  return { key: el.getAttribute('data-key'), rail: railOf(el)?.getAttribute('data-rail') ?? null }
}

let returningTo: Place | null = null

/** The next screen to take focus goes back to this place, as far as it still exists. */
export function returnTo(place: Place | null | undefined) {
  returningTo = place ?? null
}

/** The control focus is going back to, for a screen that must first render it. */
export function returningKey() {
  return returningTo?.key ?? null
}

/** The control that was at `place`, or the first one in its rail if that one's gone. */
function find(all: HTMLElement[], place: Place | null) {
  if (!place) {
    return null
  }
  return (
    all.filter((el) => place.key !== null && el.getAttribute('data-key') === place.key)[0] ??
    all.filter((el) => railOf(el)?.getAttribute('data-rail') === place.rail)[0] ??
    null
  )
}

/**
 * Something is always focused: where the person was when coming back to a screen,
 * or else the control marked `data-first`, or else the first.
 */
export function useInitialFocus(ready: unknown) {
  useEffect(() => {
    const all = controls()
    if (all.length === 0) {
      return
    }
    const first =
      find(all, returningTo) ?? all.filter((el) => el.hasAttribute('data-first'))[0] ?? all[0]
    returningTo = null
    focus(first)
  }, [ready])
}

/** Hears every key, and takes the arrow keys from focus movement while `takesArrows`. */
export function useKeys(
  onKey: (keyCode: number, event: KeyboardEvent) => boolean | void,
  takesArrows = true,
) {
  useEffect(() => {
    handlingArrows = takesArrows
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
  }, [onKey, takesArrows])
}

let backHandler: (() => boolean) | null = null

/** Gives this screen first say on Back, to close its own layers; true if it did something. */
export function useBack(handler: () => boolean) {
  useEffect(() => {
    backHandler = handler
    return () => {
      backHandler = null
    }
  }, [handler])
}

/** Whether the screen showing dealt with Back itself. */
export function screenHandledBack() {
  return backHandler ? backHandler() : false
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
