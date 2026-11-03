import { useEffect, useRef, useState, type RefObject } from 'react'
import { api, type PlayPlan, type StreamStats } from '../../api/client'
import type { Meter } from './meter'

const SAMPLE_MS = 1000
const SERVER_MS = 2000

/** What the stats panel shows, read once a second. Rates are in bits per second. */
export interface Snapshot {
  /** The video's own time. */
  time: number
  bufferAhead: number
  download: number | null
  /** Worked out from how fast the buffer grows (direct play), not measured. */
  downloadEstimated: boolean
  hlsEstimate: number | null
  bitrate: number | null
  /** The bitrate is the file's average, not the part playing. */
  bitrateAverage: boolean
  decoded: { width: number; height: number } | null
  /** The video's box on screen, in device pixels. */
  display: { width: number; height: number }
  dropped: number
  frames: number
  stalls: number
  stalledMs: number
  startup: Meter['marks']
  codecs: Meter['codecs']
  server: StreamStats | null
}

/** The end of the buffered range the playhead is in. */
function bufferedEnd(video: HTMLVideoElement): number {
  const t = video.currentTime
  for (let i = 0; i < video.buffered.length; i++) {
    if (video.buffered.start(i) <= t + 0.5 && video.buffered.end(i) >= t) {
      return video.buffered.end(i)
    }
  }
  return t
}

/**
 * Samples playback while the stats panel is `open`: the video and `meter` each
 * second, and the server's view of an HLS session every two.
 */
export function useStats(
  video: RefObject<HTMLVideoElement | null>,
  plan: PlayPlan | null,
  meter: Meter,
  open: boolean,
): Snapshot | null {
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null)
  /** The server's view, and the session it's of. */
  const [server, setServer] = useState<{ session: string; stats: StreamStats | null } | null>(null)
  /** Recent buffer ends, to estimate the download rate of direct play. */
  const history = useRef<{ at: number; end: number }[]>([])

  const session = plan?.session_id ?? null
  useEffect(() => {
    if (!open || !session) {
      return
    }
    let cancelled = false
    const poll = () =>
      api.streamStats(session).then(
        (s) => {
          if (!cancelled) {
            setServer({ session, stats: s })
          }
        },
        () => {
          if (!cancelled) {
            setServer({ session, stats: null })
          }
        },
      )
    poll()
    const timer = window.setInterval(poll, SERVER_MS)
    return () => {
      cancelled = true
      window.clearInterval(timer)
    }
  }, [open, session])

  useEffect(() => {
    history.current = []
    if (!open || !plan) {
      return
    }
    const v = plan.version
    const fileBitrate =
      v.duration_seconds && v.duration_seconds > 0 ? (v.size * 8) / v.duration_seconds : null
    const sample = () => {
      const el = video.current
      if (!el) {
        return
      }
      const end = bufferedEnd(el)
      const now = performance.now()
      const past = history.current
      if (past.length > 0 && end < past[past.length - 1].end) {
        // Seeked somewhere else; start measuring again.
        past.length = 0
      }
      past.push({ at: now, end })
      if (past.length > 4) {
        past.shift()
      }

      let download = meter.downloadRate()
      let downloadEstimated = false
      if (!meter.usingHls && fileBitrate !== null && past.length > 1) {
        const first = past[0]
        download = ((end - first.end) * fileBitrate) / ((now - first.at) / 1000)
        downloadEstimated = true
      }
      const segmentRate = meter.bitrateAt(el.currentTime)
      const quality = el.getVideoPlaybackQuality?.()
      const box = el.getBoundingClientRect()
      const dpr = window.devicePixelRatio || 1
      setSnapshot({
        time: el.currentTime,
        bufferAhead: Math.max(end - el.currentTime, 0),
        download,
        downloadEstimated,
        hlsEstimate: meter.estimate(),
        bitrate: segmentRate ?? (meter.usingHls ? null : fileBitrate),
        bitrateAverage: segmentRate === null,
        decoded: el.videoWidth > 0 ? { width: el.videoWidth, height: el.videoHeight } : null,
        display: { width: Math.round(box.width * dpr), height: Math.round(box.height * dpr) },
        dropped: quality?.droppedVideoFrames ?? 0,
        frames: quality?.totalVideoFrames ?? 0,
        stalls: meter.stalls,
        stalledMs: meter.stalled(),
        startup: { ...meter.marks },
        codecs: meter.codecs,
        server: null,
      })
    }
    sample()
    const timer = window.setInterval(sample, SAMPLE_MS)
    return () => window.clearInterval(timer)
  }, [open, plan, video, meter])

  const stats = open && server?.session === session ? server.stats : null
  return snapshot && { ...snapshot, server: stats }
}
