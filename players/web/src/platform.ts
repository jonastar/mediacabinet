import { playbackCaps } from '@core/caps'
import type { ClientCaps } from './api'

export type RemoteAction = 'back' | 'playPause' | 'play' | 'pause' | 'stop' | 'forward' | 'rewind'

/**
 * What differs between TVs. Generic for now; capabilities and HDR move to
 * `players/tizen` and `players/webos` once the spike shows what they need.
 */
export interface Platform {
  /** What this device is called until someone renames it. */
  deviceName: string
  keys: Record<number, RemoteAction>
  caps: () => ClientCaps
  /** Asks for keys that aren't delivered by default. */
  setUp: () => void
}

interface TizenApi {
  tvinputdevice: { registerKey: (name: string) => void }
}

declare global {
  interface Window {
    tizen?: TizenApi
    PalmSystem?: unknown
    showCrash: (message: string) => void
  }
}

const COMMON: Record<number, RemoteAction> = {
  // MediaPlay, MediaPause, MediaStop, MediaFastForward, MediaRewind on both.
  415: 'play',
  19: 'pause',
  413: 'stop',
  417: 'forward',
  412: 'rewind',
  // A desktop browser standing in for a remote.
  27: 'back',
  8: 'back',
}

const TIZEN_MEDIA_KEYS = [
  'MediaPlay',
  'MediaPause',
  'MediaStop',
  'MediaPlayPause',
  'MediaFastForward',
  'MediaRewind',
]

export function detectPlatform(): Platform {
  const generic = { caps: playbackCaps }
  if (window.tizen) {
    const { tizen } = window
    return {
      ...generic,
      deviceName: 'Samsung TV',
      keys: { ...COMMON, 10009: 'back', 10252: 'playPause' },
      setUp: () => {
        for (const key of TIZEN_MEDIA_KEYS) {
          try {
            tizen.tvinputdevice.registerKey(key)
          } catch {
            // Without a key, the on-screen controls still work.
          }
        }
      },
    }
  }
  if (window.PalmSystem) {
    return {
      ...generic,
      deviceName: 'LG TV',
      keys: { ...COMMON, 461: 'back' },
      setUp: () => undefined,
    }
  }
  return { ...generic, deviceName: 'TV app', keys: COMMON, setUp: () => undefined }
}

export const platform = detectPlatform()
