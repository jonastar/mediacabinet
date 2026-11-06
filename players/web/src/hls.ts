import type { HlsClass } from '@core/stream'

/** Loaded when first needed: it's the biggest thing in the app by far. */
export const loadHls = (): Promise<HlsClass> =>
  import('hls.js/dist/hls.light.mjs' as string).then((module) => module.default as HlsClass)
