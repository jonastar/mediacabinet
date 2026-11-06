import type { components } from '../api/schema.d.ts'
import { nextFailure, without, type Failure } from './fallback'
import { attachStream, startPlaying, type StreamOptions } from './stream'
import { NO_PICTURE_SECONDS, watchForPicture } from './watchdog'

type ClientCaps = components['schemas']['ClientCaps']
type PlayPlan = components['schemas']['PlayPlan']

/** The server calls a session needs. */
export interface SessionApi {
  play: (itemId: number, caps: ClientCaps, startSeconds?: number) => Promise<PlayPlan>
  stop: (sessionId: string) => Promise<unknown>
  progress: (itemId: number, position: number, duration?: number | null) => Promise<unknown>
}

export interface SessionOptions extends Pick<
  StreamOptions,
  'loadHls' | 'hlsConfig' | 'fileUrl' | 'playlistUrl'
> {
  video: HTMLVideoElement
  itemId: number
  api: SessionApi
  /** What to ask for first. */
  caps: ClientCaps
  /** Called with what turned out not to play, once something else did. */
  remember: (failures: Failure[]) => void
  /** Called whenever there's something new to show. */
  onChange: () => void
}

const REPORT_EVERY_MS = 15000
/** Waits for more presses, so seeking doesn't start a stream per press. */
const SEEK_SETTLE_MS = 600
const NETWORK_RETRIES = 2
const RETRIES_FORGIVEN_AFTER_MS = 5 * 60 * 1000

/**
 * Playing one item: asks the server how, attaches the stream, recovers from
 * failures, seeks past what's converted, and reports progress. It has no UI, so
 * each app draws its own controls from `plan`, `position()` and `error`.
 *
 * Times here are the file's, while the video's start at `offset_seconds`.
 */
export class PlaybackSession {
  plan: PlayPlan | null = null
  error: string | null = null
  /** A request is out, or a seek is waiting to become one. */
  loading = true

  private readonly options: SessionOptions
  private caps: ClientCaps
  private unproven: Failure[] = []
  private started = false
  private retries = 0
  private lastRetryAt = 0
  private request = 0
  private seekTarget: number | null = null
  private seekTimer: number | undefined
  private reportTimer: number | undefined
  private detach = () => {}
  private unwatch = () => {}
  private lastPosition = 0

  constructor(options: SessionOptions) {
    this.options = options
    this.caps = options.caps
  }

  /** Starts playing, at `at` or from where the viewer left off. */
  start(at?: number) {
    this.reportTimer = window.setInterval(() => this.report(), REPORT_EVERY_MS)
    this.load(at)
  }

  /** Where playback is in the file; where it's headed while a seek is pending. */
  position(): number {
    if (this.seekTarget !== null) {
      return this.seekTarget
    }
    if (!this.plan) {
      return 0
    }
    if (this.options.video.readyState === 0) {
      return this.lastPosition
    }
    this.lastPosition = this.options.video.currentTime + this.plan.offset_seconds
    return this.lastPosition
  }

  duration(): number {
    return this.plan?.version.duration_seconds ?? 0
  }

  paused() {
    return this.options.video.paused
  }

  togglePause() {
    const { video } = this.options
    if (video.paused) {
      startPlaying(video)
    } else {
      video.pause()
    }
    this.options.onChange()
  }

  /** Moves by this many seconds, in steps that add up while keys are pressed. */
  seekBy(seconds: number) {
    this.seekTo(this.position() + seconds)
  }

  seekTo(target: number) {
    const plan = this.plan
    if (!plan) {
      return
    }
    const { video } = this.options
    const end = this.duration() || target
    const to = Math.min(Math.max(target, 0), end)
    const inStream = to - plan.offset_seconds
    if (plan.mode === 'direct') {
      video.currentTime = to
    } else if (inStream >= 0 && inStream <= this.convertedUntil()) {
      video.currentTime = inStream
      this.cancelPendingSeek()
    } else {
      // Past what's converted: a new stream starts there, once the presses stop.
      this.seekTarget = to
      this.loading = true
      window.clearTimeout(this.seekTimer)
      this.seekTimer = window.setTimeout(() => this.load(to), SEEK_SETTLE_MS)
    }
    this.options.onChange()
  }

  /** Tries again after an error, from what's remembered. */
  retry(caps: ClientCaps) {
    this.caps = caps
    this.unproven = []
    this.retries = 0
    this.lastRetryAt = 0
    this.load(this.position())
  }

  /** Reports the position and ends the stream, when leaving. */
  destroy() {
    this.request++
    this.report()
    this.cancelPendingSeek()
    window.clearInterval(this.reportTimer)
    this.unwatch()
    this.detach()
    this.stopServerSide(this.plan)
    this.plan = null
  }

  report() {
    if (this.plan) {
      const { api, itemId } = this.options
      api.progress(itemId, this.position(), this.duration() || null).catch(() => undefined)
    }
  }

  private convertedUntil() {
    const { seekable } = this.options.video
    return seekable.length > 0 ? seekable.end(seekable.length - 1) : 0
  }

  private cancelPendingSeek() {
    window.clearTimeout(this.seekTimer)
    this.seekTarget = null
  }

  private stopServerSide(plan: PlayPlan | null) {
    if (plan?.session_id) {
      this.options.api.stop(plan.session_id).catch(() => undefined)
    }
  }

  private load(at?: number) {
    const { api, itemId, onChange } = this.options
    const request = ++this.request
    this.loading = true
    onChange()
    api.play(itemId, this.caps, at).then(
      (plan) => {
        if (request !== this.request) {
          // Overtaken by a later request.
          this.stopServerSide(plan)
          return
        }
        this.attach(plan)
      },
      (err: Error) => {
        if (request === this.request) {
          this.loading = false
          this.seekTarget = null
          this.error = err.message
          onChange()
        }
      },
    )
  }

  private attach(plan: PlayPlan) {
    const { video, loadHls, hlsConfig, fileUrl, playlistUrl, onChange } = this.options
    this.unwatch()
    this.detach()
    this.stopServerSide(this.plan)
    this.plan = plan
    this.lastPosition = plan.start_seconds
    this.seekTarget = null
    this.error = null
    this.started = false
    this.detach = attachStream(video, plan, {
      loadHls,
      hlsConfig,
      fileUrl,
      playlistUrl,
      onFatal: (message, cause) => this.onFatal(message, cause),
    })
    this.unwatch = watchForPicture(
      video,
      () => this.onStarted(),
      () => this.onDecodeFailed(`no picture after ${NO_PICTURE_SECONDS} seconds`),
    )
    this.loading = false
    onChange()
  }

  private onStarted() {
    this.started = true
    this.options.remember(this.unproven)
    this.unproven = []
  }

  private onFatal(message: string, cause: 'network' | 'decode') {
    if (cause === 'decode') {
      this.onDecodeFailed(message)
      return
    }
    // Retries are forgiven by a quiet spell, not by starting to play: a stream
    // that plays briefly and breaks each time would otherwise restart forever.
    const now = Date.now()
    if (now - this.lastRetryAt > RETRIES_FORGIVEN_AFTER_MS) {
      this.retries = 0
    }
    if (this.retries < NETWORK_RETRIES) {
      // A stream that ended on the server (idle too long) restarts where it was.
      this.retries += 1
      this.lastRetryAt = now
      this.load(this.position())
    } else {
      this.giveUp(message)
    }
  }

  private onDecodeFailed(message: string) {
    const failure = this.plan && nextFailure(this.plan, this.caps)
    if (!failure) {
      this.giveUp(message)
      return
    }
    this.caps = without(this.caps, failure)
    // Something that broke partway through a file says little about the device.
    if (!this.started) {
      this.unproven.push(failure)
    }
    this.load(this.position())
  }

  private giveUp(message: string) {
    this.error = `Playback stopped: ${message}`
    this.options.onChange()
  }

  /** For direct play, the video reports errors itself. */
  videoFailed() {
    const code = this.options.video.error?.code
    if (this.plan?.mode === 'direct' && code) {
      // MEDIA_ERR_NETWORK; the constant isn't there on every old engine.
      this.onFatal('the file could not be read', code === 2 ? 'network' : 'decode')
    }
  }
}
