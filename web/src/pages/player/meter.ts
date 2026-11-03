import type Hls from 'hls.js'

/** A segment hls.js loaded: where it sits in the stream, its size and how long it took. */
interface Segment {
  start: number
  duration: number
  bytes: number
  ms: number
}

/**
 * The steps from asking for a stream to its first frame: the server's answer,
 * hls.js loaded, the playlist, the first segment, the file's metadata (the
 * first segment's, for HLS) and the first frame.
 */
export type Step = 'planned' | 'hls' | 'playlist' | 'segment' | 'metadata' | 'frame'

/** How many recent segments the download rate is averaged over. */
const RATE_SEGMENTS = 5

/**
 * Figures for the stats panel that can't be read off the video at any moment:
 * segment downloads, codecs, the steps of starting and stalls. One per player;
 * `begin` starts over for each stream.
 */
export class Meter {
  private segments: Segment[] = []
  private hls: Hls | null = null
  private begun = performance.now()
  private stallStart: number | null = null
  /** When each step of starting was reached, in ms after asking for the stream. */
  marks: Partial<Record<Step, number>> = {}
  stalls = 0
  stalledMs = 0
  codecs: { video: string | null; audio: string | null } = { video: null, audio: null }

  /** A new stream is being asked for. */
  begin() {
    this.segments = []
    this.hls = null
    this.begun = performance.now()
    this.stallStart = null
    this.marks = {}
    this.stalls = 0
    this.stalledMs = 0
    this.codecs = { video: null, audio: null }
  }

  /** Notes a step of starting, the first time it's reached. */
  mark(step: Step) {
    this.marks[step] ??= performance.now() - this.begun
  }

  /** Listens to an hls.js player before it loads anything. */
  attach(player: Hls, Events: typeof Hls.Events) {
    this.hls = player
    this.mark('hls')
    player.on(Events.MANIFEST_LOADED, () => this.mark('playlist'))
    player.on(Events.FRAG_LOADED, (_event, { frag }) => {
      if (frag.type !== 'main') {
        return
      }
      this.mark('segment')
      const { loaded, loading } = frag.stats
      this.segments.push({
        start: frag.start,
        duration: frag.duration,
        bytes: loaded,
        ms: Math.max(loading.end - loading.start, 1),
      })
      if (this.segments.length > 400) {
        this.segments.shift()
      }
    })
    player.on(Events.BUFFER_CODECS, (_event, data) => {
      this.codecs = {
        video: data.video?.codec ?? data.audiovideo?.codec ?? null,
        audio: data.audio?.codec ?? null,
      }
    })
  }

  waiting(seeking: boolean) {
    // Waiting for the first frame, or after a seek, isn't a stall.
    if (this.marks.frame === undefined || seeking || this.stallStart !== null) {
      return
    }
    this.stallStart = performance.now()
    this.stalls += 1
  }

  playing() {
    const now = performance.now()
    this.mark('frame')
    if (this.stallStart !== null) {
      this.stalledMs += now - this.stallStart
      this.stallStart = null
    }
  }

  /** Stall time so far, counting one still going. */
  stalled(): number {
    const ongoing = this.stallStart === null ? 0 : performance.now() - this.stallStart
    return this.stalledMs + ongoing
  }

  /** Bits per second over the last few segments, as the network delivered them. */
  downloadRate(): number | null {
    const recent = this.segments.slice(-RATE_SEGMENTS)
    if (recent.length === 0) {
      return null
    }
    const bytes = recent.reduce((sum, s) => sum + s.bytes, 0)
    const ms = recent.reduce((sum, s) => sum + s.ms, 0)
    return (bytes * 8 * 1000) / ms
  }

  /** hls.js's own bandwidth estimate, in bits per second. */
  estimate(): number | null {
    const value = this.hls?.bandwidthEstimate
    return value && Number.isFinite(value) ? value : null
  }

  /** The bitrate of the segment playing at `time` (stream time), in bits per second. */
  bitrateAt(time: number): number | null {
    const segment = this.segments.findLast(
      (s) => time >= s.start && time < s.start + s.duration && s.duration > 0,
    )
    return segment ? (segment.bytes * 8) / segment.duration : null
  }

  /** Bits per second over everything loaded so far. */
  averageBitrate(): number | null {
    const seconds = this.segments.reduce((sum, s) => sum + s.duration, 0)
    const bytes = this.segments.reduce((sum, s) => sum + s.bytes, 0)
    return seconds > 0 ? (bytes * 8) / seconds : null
  }

  get usingHls(): boolean {
    return this.hls !== null
  }
}
