import { useCallback, useEffect, useRef, useState } from 'react'
import { api, type ClientCaps, type PlayChoice, type PlayPlan } from '../api/client'
import { useApi } from '../api/useApi'
import {
  BackIcon,
  FullscreenIcon,
  PauseIcon,
  PlayLargeIcon,
  SkipBackIcon,
  SkipForwardIcon,
  TracksIcon,
  VolumeIcon,
} from '../components/Icons'
import { playbackCaps } from '../lib/caps'
import { episodeLabel } from '../lib/format'
import { navigate, useQueryParam } from '../lib/router'
import { clock } from './player/labels'
import { Meter } from './player/meter'
import { StatsPanel } from './player/StatsPanel'
import { TracksMenu } from './player/TracksMenu'
import { useFallback } from './player/useFallback'
import { preloadHls, useSource } from './player/useSource'
import { useStats } from './player/useStats'
import styles from './player/Player.module.css'

/** How often progress is saved while playing. */
const REPORT_EVERY_MS = 10_000
const UP_NEXT_SECONDS = 10
const STATS_KEY = 'mc:player-stats'

/** Whether the stats panel was left open, so it stays open between episodes. */
function statsWereOpen(): boolean {
  try {
    return localStorage.getItem(STATS_KEY) === '1'
  } catch {
    return false
  }
}

const MODE_HINT: Record<PlayPlan['mode'], string> = {
  direct: 'Direct play',
  remux: 'Repackaging',
  transcode: 'Converting',
}

function title(plan: PlayPlan) {
  const c = plan.context
  if (c.kind === 'episode') {
    return {
      main: c.series_title ?? c.title,
      sub: `${c.number ? `${episodeLabel(c.number)} · ` : ''}${c.title}`,
    }
  }
  return { main: c.title, sub: c.year ? String(c.year) : null }
}

/**
 * Full-screen player for an episode or movie. The server decides how the file
 * is sent; an HLS stream may start partway into the file (`offset_seconds`), so
 * positions here are always in the file's time.
 */
export function PlayerPage({ id }: { id: number }) {
  const fileParam = useQueryParam('file')
  const fromStart = useQueryParam('start') === '0'
  const videoRef = useRef<HTMLVideoElement>(null)
  const frameRef = useRef<HTMLDivElement>(null)
  const versions = useApi(`versions:${id}`, () => api.versions(id))

  const [plan, setPlan] = useState<PlayPlan | null>(null)
  const [choice, setChoice] = useState<PlayChoice>({})
  const [error, setError] = useState<string | null>(null)
  /** The video's own time; add the plan's offset for the file's. */
  const [time, setTime] = useState(0)
  /** How far the stream can be seeked without restarting, in the video's time. */
  const [ready, setReady] = useState(0)
  const [videoDuration, setVideoDuration] = useState(0)
  const [restartingAt, setRestartingAt] = useState<number | null>(null)
  const [scrub, setScrub] = useState<number | null>(null)
  const [playing, setPlaying] = useState(false)
  const [waiting, setWaiting] = useState(true)
  const [muted, setMuted] = useState(false)
  const [menuOpen, setMenuOpen] = useState(false)
  const [idle, setIdle] = useState(false)
  const [fullscreen, setFullscreen] = useState(false)
  const [countdown, setCountdown] = useState<number | null>(null)
  const [statsOpen, setStatsOpen] = useState(statsWereOpen)
  const [meter] = useState(() => new Meter())

  // Latest values for callbacks that outlive a render (unmount, timers).
  const planRef = useRef<PlayPlan | null>(null)
  const choiceRef = useRef<PlayChoice>({})
  const positionRef = useRef(0)
  const requestRef = useRef(0)
  const retriesRef = useRef(0)
  const capsRef = useRef(playbackCaps())
  /** A seek waiting to restart the stream, and where it goes. */
  const restartTimer = useRef<number | undefined>(undefined)
  const restartTarget = useRef<number | null>(null)

  const load = useCallback(
    (next: PlayChoice, at?: number) => {
      const request = ++requestRef.current
      meter.begin()
      return api.play(id, next, capsRef.current, at).then(
        (p) => {
          if (request !== requestRef.current) {
            // Overtaken by a later request.
            if (p.session_id) {
              api.stopPlay(p.session_id).catch(() => undefined)
            }
            return
          }
          meter.mark('planned')
          const old = planRef.current
          if (old?.session_id && old.session_id !== p.session_id) {
            api.stopPlay(old.session_id).catch(() => undefined)
          }
          const settled = {
            ...next,
            file_id: p.version.file_id,
            audio_stream: p.audio_stream ?? undefined,
          }
          planRef.current = p
          choiceRef.current = settled
          restartTarget.current = null
          positionRef.current = p.start_seconds
          setPlan(p)
          setChoice(settled)
          setTime(0)
          setReady(0)
          setRestartingAt(null)
          setError(null)
          setCountdown(null)
        },
        (e: Error) => {
          if (request === requestRef.current) {
            setRestartingAt(null)
            setError(e.message)
          }
        },
      )
    },
    [id, meter],
  )

  useEffect(preloadHls, [])

  useEffect(() => {
    load({ file_id: fileParam ? Number(fileParam) : undefined }, fromStart ? 0 : undefined)
  }, [load, fileParam, fromStart])

  const report = useCallback(
    (position = positionRef.current) => {
      const p = planRef.current
      if (p) {
        api.progress(id, position, p.version.duration_seconds).catch(() => undefined)
      }
    },
    [id],
  )

  // Save the position and stop the stream when leaving, however that happens.
  useEffect(() => {
    const leave = () => {
      report()
      const session = planRef.current?.session_id
      if (session) {
        api.stopPlay(session).catch(() => undefined)
      }
    }
    window.addEventListener('pagehide', leave)
    return () => {
      window.removeEventListener('pagehide', leave)
      leave()
    }
  }, [report])

  useEffect(() => {
    if (!playing) {
      return
    }
    const timer = window.setInterval(() => report(), REPORT_EVERY_MS)
    return () => window.clearInterval(timer)
  }, [playing, report])

  const restart = useCallback(() => {
    setRestartingAt(positionRef.current)
    load(choiceRef.current, positionRef.current)
  }, [load])
  const currentCaps = useCallback(() => capsRef.current, [])
  const restartWith = useCallback(
    (caps: ClientCaps) => {
      capsRef.current = caps
      restart()
    },
    [restart],
  )
  const giveUp = useCallback((message: string) => {
    setError(`Playback stopped: ${message}`)
  }, [])
  const fallback = useFallback(videoRef, plan, currentCaps, restartWith, giveUp)
  const { decodeFailed } = fallback

  const onFatal = useCallback(
    (message: string, cause: 'network' | 'decode') => {
      if (cause === 'decode') {
        decodeFailed(message)
        return
      }
      // A stream that ended on the server (idle too long) restarts where it was.
      if (retriesRef.current < 2) {
        retriesRef.current += 1
        restart()
      } else {
        giveUp(message)
      }
    },
    [decodeFailed, restart, giveUp],
  )
  useSource(videoRef, plan, onFatal, meter)
  const snapshot = useStats(videoRef, plan, meter, statsOpen)

  const toggleStats = () => {
    setMenuOpen(false)
    setStatsOpen((open) => {
      try {
        localStorage.setItem(STATS_KEY, open ? '0' : '1')
      } catch {
        // Not remembered; it still toggles.
      }
      return !open
    })
  }

  const offset = plan?.offset_seconds ?? 0
  const duration = plan?.version.duration_seconds ?? offset + videoDuration
  const position = scrub ?? restartingAt ?? offset + time
  const next = plan?.context.next ?? null

  const subtitle = plan?.subtitles.find((s) => s.id === choice.subtitle && s.url) ?? null
  const subtitleUrl = subtitle?.url ?? null
  useEffect(() => {
    const video = videoRef.current
    if (!video) {
      return
    }
    for (const track of Array.from(video.textTracks)) {
      track.mode = subtitleUrl ? 'showing' : 'disabled'
    }
  }, [subtitleUrl])

  // Counting down to the next episode.
  useEffect(() => {
    if (countdown === null || !next) {
      return
    }
    if (countdown <= 0) {
      navigate(`/play/${next.item_id}`, { replace: true })
      return
    }
    const timer = window.setTimeout(() => setCountdown((c) => (c === null ? null : c - 1)), 1000)
    return () => window.clearTimeout(timer)
  }, [countdown, next])

  useEffect(() => {
    const onChange = () => setFullscreen(document.fullscreenElement !== null)
    document.addEventListener('fullscreenchange', onChange)
    return () => document.removeEventListener('fullscreenchange', onChange)
  }, [])

  // Controls fade after a few still seconds while playing.
  const idleTimer = useRef<number | undefined>(undefined)
  const wake = () => {
    setIdle(false)
    window.clearTimeout(idleTimer.current)
    idleTimer.current = window.setTimeout(() => setIdle(true), 3000)
  }
  useEffect(() => () => window.clearTimeout(idleTimer.current), [])

  useEffect(() => () => window.clearTimeout(restartTimer.current), [])

  /** The current position in the file, including seeks not shown yet. */
  const now = () =>
    restartTarget.current ?? (plan?.offset_seconds ?? 0) + (videoRef.current?.currentTime ?? 0)

  const seek = (target: number) => {
    const video = videoRef.current
    if (!video || !plan) {
      return
    }
    const t = Math.min(Math.max(target, 0), Math.max(duration - 1, 0))
    const local = t - plan.offset_seconds
    if (plan.mode === 'direct' || (local >= 0 && local <= ready)) {
      window.clearTimeout(restartTimer.current)
      restartTarget.current = null
      setRestartingAt(null)
      video.currentTime = Math.max(local, 0)
      return
    }
    // Not converted yet (or before this stream's start): start again from
    // there, once the seeking settles.
    setRestartingAt(t)
    restartTarget.current = t
    window.clearTimeout(restartTimer.current)
    restartTimer.current = window.setTimeout(() => load(choiceRef.current, t), 400)
  }

  const toggle = () => {
    const video = videoRef.current
    if (!video) {
      return
    }
    if (video.paused) {
      video.play().catch(() => undefined)
    } else {
      video.pause()
    }
  }

  const toggleFullscreen = () => {
    if (document.fullscreenElement) {
      document.exitFullscreen().catch(() => undefined)
    } else {
      frameRef.current?.requestFullscreen().catch(() => undefined)
    }
  }

  const back = () => {
    const c = plan?.context
    if (c?.series_id) {
      navigate(`/tv/${c.series_id}`)
    } else if (c) {
      navigate(`/movies/${c.item_id}`)
    } else {
      window.history.back()
    }
  }

  const change = (nextChoice: PlayChoice) => {
    setMenuOpen(false)
    if (!plan) {
      return
    }
    const track = plan.version.subtitles.find((t) => t.id === nextChoice.subtitle)
    const burning = plan.subtitles.some((s) => !s.url)
    const sameStream =
      nextChoice.file_id === choice.file_id &&
      nextChoice.audio_stream === choice.audio_stream &&
      (nextChoice.max_height ?? null) === (choice.max_height ?? null)
    // Text subtitles are drawn by the browser; anything else needs a new stream.
    if (sameStream && (track?.text ?? true) && !burning) {
      choiceRef.current = nextChoice
      setChoice(nextChoice)
      return
    }
    setRestartingAt(position)
    load(nextChoice, position)
  }

  const commitScrub = () => {
    if (scrub !== null) {
      seek(scrub)
      setScrub(null)
    }
  }

  const onKey = (e: KeyboardEvent) => {
    if (e.target instanceof HTMLInputElement || e.target instanceof HTMLButtonElement) {
      if (e.key === ' ' || e.key === 'Enter' || e.target instanceof HTMLInputElement) {
        return
      }
    }
    const keys: Record<string, () => void> = {
      ' ': toggle,
      k: toggle,
      ArrowLeft: () => seek(now() - 10),
      ArrowRight: () => seek(now() + 30),
      f: toggleFullscreen,
      i: toggleStats,
      m: () => {
        if (videoRef.current) {
          videoRef.current.muted = !videoRef.current.muted
        }
      },
      Escape: () => setMenuOpen(false),
    }
    const action = keys[e.key]
    if (action) {
      e.preventDefault()
      wake()
      action()
    }
  }

  // Shortcuts work whatever has focus; the listener calls the latest handler.
  const keyRef = useRef(onKey)
  useEffect(() => {
    keyRef.current = onKey
  })
  useEffect(() => {
    const listener = (e: KeyboardEvent) => keyRef.current(e)
    window.addEventListener('keydown', listener)
    return () => window.removeEventListener('keydown', listener)
  }, [])

  const pct = (seconds: number) => `${duration > 0 ? (seconds / duration) * 100 : 0}%`
  const heading = plan ? title(plan) : null
  const hideControls = idle && playing && !menuOpen && countdown === null

  return (
    <div ref={frameRef} className={styles.frame} data-idle={hideControls} onPointerMove={wake}>
      <video
        ref={videoRef}
        className={styles.video}
        playsInline
        onClick={toggle}
        onDoubleClick={toggleFullscreen}
        onTimeUpdate={(e) => {
          const v = e.currentTarget
          setTime(v.currentTime)
          positionRef.current = offset + v.currentTime
          if (v.seekable.length > 0) {
            setReady(v.seekable.end(v.seekable.length - 1))
          }
          if (v.currentTime > 5) {
            retriesRef.current = 0
          }
        }}
        onDurationChange={(e) =>
          setVideoDuration(Number.isFinite(e.currentTarget.duration) ? e.currentTarget.duration : 0)
        }
        onLoadedMetadata={() => meter.mark('metadata')}
        onPlay={() => setPlaying(true)}
        onPause={() => {
          setPlaying(false)
          report()
        }}
        onWaiting={(e) => {
          setWaiting(true)
          meter.waiting(e.currentTarget.seeking)
        }}
        onPlaying={() => {
          setWaiting(false)
          meter.playing()
        }}
        onCanPlay={() => setWaiting(false)}
        onVolumeChange={(e) => setMuted(e.currentTarget.muted)}
        onEnded={() => {
          setPlaying(false)
          report(duration)
          if (next) {
            setCountdown(UP_NEXT_SECONDS)
          }
        }}
        onError={(e) => {
          if (plan?.mode === 'direct') {
            const network = e.currentTarget.error?.code === MediaError.MEDIA_ERR_NETWORK
            onFatal('the browser could not read the file', network ? 'network' : 'decode')
          }
        }}
      >
        {subtitle?.url && (
          <track
            key={subtitle.url}
            kind="subtitles"
            src={subtitle.url}
            srcLang={subtitle.language ?? undefined}
            label="Subtitles"
            default
          />
        )}
      </video>

      {(waiting || restartingAt !== null) && !error && (
        <div className={styles.spinner} role="status" aria-label="Loading" />
      )}

      {error && (
        <div className={styles.error} role="alert">
          <p>{error}</p>
          <div className={styles.errorActions}>
            <button
              type="button"
              className={styles.pill}
              onClick={() => {
                retriesRef.current = 0
                capsRef.current = playbackCaps()
                fallback.reset()
                setError(null)
                load(choiceRef.current, positionRef.current)
              }}
            >
              Try again
            </button>
            <button type="button" className={styles.pill} onClick={back}>
              Back
            </button>
          </div>
        </div>
      )}

      <header className={styles.top}>
        <button type="button" className={styles.icon} aria-label="Back" onClick={back}>
          <BackIcon />
        </button>
        {heading && (
          <div className={styles.titles}>
            <h1>{heading.main}</h1>
            {heading.sub && <p>{heading.sub}</p>}
          </div>
        )}
      </header>

      {statsOpen && plan && <StatsPanel plan={plan} snapshot={snapshot} onClose={toggleStats} />}

      {countdown !== null && next && (
        <section className={styles.upNext} aria-label="Up next">
          <span className={styles.upNextLabel}>Up next in {countdown}</span>
          <b>
            {next.number ? `${episodeLabel(next.number)} · ` : ''}
            {next.title}
          </b>
          <div className={styles.errorActions}>
            <button
              type="button"
              className={`${styles.pill} ${styles.primary}`}
              onClick={() => navigate(`/play/${next.item_id}`, { replace: true })}
            >
              Play now
            </button>
            <button type="button" className={styles.pill} onClick={() => setCountdown(null)}>
              Cancel
            </button>
          </div>
        </section>
      )}

      <div className={styles.bottom}>
        <div className={styles.timeline}>
          <div className={styles.track} aria-hidden="true">
            {plan && plan.mode !== 'direct' && (
              <div
                className={styles.converted}
                style={{ left: pct(offset), width: pct(Math.max(ready, time)) }}
              />
            )}
            <div className={styles.played} style={{ width: pct(position) }} />
          </div>
          <input
            type="range"
            className={styles.range}
            min={0}
            max={Math.max(Math.floor(duration), 1)}
            step={1}
            value={Math.floor(position)}
            aria-label="Position"
            aria-valuetext={`${clock(position)} of ${clock(duration)}`}
            onChange={(e) => setScrub(Number(e.target.value))}
            onPointerUp={commitScrub}
            onKeyUp={commitScrub}
          />
        </div>
        <div className={styles.controls}>
          <button
            type="button"
            className={styles.icon}
            aria-label={playing ? 'Pause' : 'Play'}
            onClick={toggle}
          >
            {playing ? <PauseIcon /> : <PlayLargeIcon />}
          </button>
          <button
            type="button"
            className={styles.icon}
            aria-label="Back 10 seconds"
            onClick={() => seek(now() - 10)}
          >
            <SkipBackIcon />
          </button>
          <button
            type="button"
            className={styles.icon}
            aria-label="Forward 30 seconds"
            onClick={() => seek(now() + 30)}
          >
            <SkipForwardIcon />
          </button>
          <button
            type="button"
            className={styles.icon}
            aria-label={muted ? 'Unmute' : 'Mute'}
            onClick={() => {
              if (videoRef.current) {
                videoRef.current.muted = !videoRef.current.muted
              }
            }}
          >
            <VolumeIcon muted={muted} />
          </button>
          <span className={styles.time}>
            {clock(position)} / {clock(duration)}
          </span>
          <span className={styles.spacer} />
          {plan && <span className={styles.mode}>{MODE_HINT[plan.mode]}</span>}
          <button
            type="button"
            className={styles.icon}
            aria-label="Audio, subtitles and quality"
            aria-expanded={menuOpen}
            aria-haspopup="menu"
            disabled={!plan}
            onClick={() => setMenuOpen((open) => !open)}
          >
            <TracksIcon />
          </button>
          <button
            type="button"
            className={styles.icon}
            aria-label={fullscreen ? 'Exit full screen' : 'Full screen'}
            onClick={toggleFullscreen}
          >
            <FullscreenIcon on={fullscreen} />
          </button>
        </div>
        {menuOpen && plan && (
          <TracksMenu
            plan={plan}
            versions={versions.data ?? [plan.version]}
            choice={choice}
            onChange={change}
            onStats={toggleStats}
          />
        )}
      </div>
    </div>
  )
}
