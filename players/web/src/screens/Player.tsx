import { useCallback, useEffect, useMemo, useRef, useState } from 'preact/hooks'
import { rememberFailures } from '@core/caps'
import { PlaybackSession } from '@core/session'
import { mediaToken, mediaUrl, session as signIn, sessionApi } from '../api'
import { useKeys, useRemote } from '../focus'
import { loadHls } from '../hls'
import { platform, type RemoteAction } from '../platform'

const SEEK_STEP = 10
const CONTROLS_HIDE_MS = 4000
const MODE_LABEL = { direct: 'Playing the file', remux: 'Repackaging', transcode: 'Converting' }

/** TVs have less memory to spare than computers. */
const HLS_LIMITS = { maxBufferLength: 30, maxMaxBufferLength: 60, backBufferLength: 30 }

function clock(seconds: number) {
  const total = Math.max(0, Math.floor(seconds))
  const minutes = Math.floor(total / 60)
  const two = (n: number) => (n < 10 ? `0${n}` : `${n}`)
  return minutes >= 60
    ? `${Math.floor(minutes / 60)}:${two(minutes % 60)}:${two(total % 60)}`
    : `${minutes}:${two(total % 60)}`
}

/** Plays an item, with controls drawn here because streams can start partway in. */
export function Player({
  screen,
  onLeave,
}: {
  screen: { itemId: number; title: string }
  onLeave: () => void
}) {
  const [video, setVideo] = useState<HTMLVideoElement | null>(null)
  const [token, setToken] = useState<string | null>(null)
  const [, setTick] = useState(0)
  const [controls, setControls] = useState(true)
  const hideTimerRef = useRef<number | undefined>(undefined)
  const redraw = useCallback(() => setTick((n) => n + 1), [])

  const showControls = useCallback(() => {
    setControls(true)
    window.clearTimeout(hideTimerRef.current)
    hideTimerRef.current = window.setTimeout(() => setControls(false), CONTROLS_HIDE_MS)
  }, [])

  useEffect(() => {
    mediaToken().then(setToken, (err: Error) => window.showCrash(err.message))
  }, [])

  const playback = useMemo(() => {
    if (!video || !token) {
      return null
    }
    return new PlaybackSession({
      video,
      itemId: screen.itemId,
      api: sessionApi,
      caps: platform.caps(),
      remember: rememberFailures,
      onChange: redraw,
      loadHls,
      hlsConfig: {
        ...HLS_LIMITS,
        xhrSetup: (xhr) => {
          xhr.setRequestHeader('Authorization', `Bearer ${signIn.token()}`)
        },
      },
      fileUrl: (url) => mediaUrl(url, token),
      playlistUrl: (url) => `${signIn.server()}${url}`,
    })
  }, [video, token, screen.itemId, redraw])

  useEffect(() => {
    if (!playback) {
      return
    }
    playback.start()
    return () => playback.destroy()
  }, [playback])

  useEffect(() => {
    showControls()
    const ticking = window.setInterval(redraw, 1000)
    return () => {
      window.clearInterval(ticking)
      window.clearTimeout(hideTimerRef.current)
    }
  }, [redraw, showControls])

  const onKey = useCallback(
    (keyCode: number) => {
      if (!playback) {
        return false
      }
      showControls()
      switch (keyCode) {
        case 37:
          playback.seekBy(-SEEK_STEP)
          return true
        case 39:
          playback.seekBy(SEEK_STEP)
          return true
        case 13:
          playback.togglePause()
          return true
        case 38:
        case 40:
          return true
        default:
          return false
      }
    },
    [playback, showControls],
  )
  useKeys(onKey)

  const onRemote = useCallback(
    (action: RemoteAction) => {
      if (!playback) {
        return
      }
      showControls()
      switch (action) {
        case 'playPause':
          playback.togglePause()
          break
        case 'play':
          if (video?.paused) {
            playback.togglePause()
          }
          break
        case 'pause':
          if (video && !video.paused) {
            playback.togglePause()
          }
          break
        case 'stop':
          onLeave()
          break
        case 'forward':
          playback.seekBy(SEEK_STEP * 3)
          break
        case 'rewind':
          playback.seekBy(-SEEK_STEP * 3)
          break
        default:
      }
    },
    [playback, video, showControls, onLeave],
  )
  useRemote(onRemote)

  const plan = playback?.plan
  const position = playback ? playback.position() : 0
  const duration = playback ? playback.duration() : 0
  const percent = duration > 0 ? Math.min(100, (position / duration) * 100) : 0

  return (
    <main className="player">
      <video
        ref={setVideo}
        onError={() => playback?.videoFailed()}
        onEnded={() => {
          playback?.report()
          onLeave()
        }}
      />
      {playback?.loading && !playback.error && <p className="player-status">Loading…</p>}
      {playback?.error && (
        <div className="center player-error">
          <p className="problem" role="alert">
            {playback.error}
          </p>
          <p className="muted">Press Back to return.</p>
        </div>
      )}
      <div className={controls ? 'controls' : 'controls hidden'}>
        <h1>{screen.title}</h1>
        <div className="timeline">
          <div className="timeline-played" style={{ width: `${percent}%` }} />
        </div>
        <div className="controls-row">
          <span>
            {playback?.paused() ? 'Paused · ' : ''}
            {clock(position)} / {clock(duration)}
          </span>
          {plan && (
            <span className="muted">
              {MODE_LABEL[plan.mode]}
              {plan.reasons.length > 0 ? `: ${plan.reasons.join(' ')}` : ''}
            </span>
          )}
        </div>
      </div>
    </main>
  )
}
