import { useCallback, useEffect, useMemo, useRef, useState } from 'preact/hooks'
import { rememberFailures } from '@core/caps'
import { episodeLine } from '@core/episode'
import { clock } from '@core/labels'
import { PlaybackSession } from '@core/session'
import { parseVtt, textAt, type Cue } from '@core/subtitles'
import { subtitleUrl, type PlayChoice } from '@core/tracks'
import type { Screen } from '../App'
import { api, mediaToken, mediaUrl, session as signIn, sessionApi, type Version } from '../api'
import { useBack, useInitialFocus, useKeys, usePointerMove, useRemote } from '../focus'
import { loadHls } from '../hls'
import { platform, type RemoteAction } from '../platform'
import { TracksPanel } from './TracksPanel'

const SEEK_STEP = 10
const CONTROLS_HIDE_MS = 4000
const MODE_LABEL = { direct: 'Playing the file', remux: 'Repackaging', transcode: 'Converting' }
/** How long before the end the next episode is offered. */
const UP_NEXT_SECONDS = 30
/** How long after the end the next episode starts by itself. */
const COUNTDOWN_SECONDS = 10

/** TVs have less memory to spare than computers. */
const HLS_LIMITS = { maxBufferLength: 30, maxMaxBufferLength: 60, backBufferLength: 30 }

/**
 * What's over the video. `timeline` is for feedback while seeking, and has no
 * buttons, so left and right keep seeking: only `controls` lets them move focus.
 */
type Overlay = 'hidden' | 'timeline' | 'controls' | 'panel'

/** Draws subtitles over the video, in the video's own time. */
function Subtitles({
  video,
  url,
  raised,
}: {
  video: HTMLVideoElement
  url: string
  raised: boolean
}) {
  const [cues, setCues] = useState<Cue[]>([])
  const [text, setText] = useState('')

  useEffect(() => {
    let current = true
    setCues([])
    setText('')
    api.subtitles(url).then(
      (vtt) => {
        if (current) {
          setCues(parseVtt(vtt))
        }
      },
      () => {
        if (current) {
          setText("These subtitles couldn't be loaded.")
        }
      },
    )
    return () => {
      current = false
    }
  }, [url])

  useEffect(() => {
    if (cues.length === 0) {
      return
    }
    const update = () => setText(textAt(cues, video.currentTime))
    update()
    video.addEventListener('timeupdate', update)
    video.addEventListener('seeked', update)
    return () => {
      video.removeEventListener('timeupdate', update)
      video.removeEventListener('seeked', update)
    }
  }, [video, cues])

  return text ? <p className={raised ? 'subtitles raised' : 'subtitles'}>{text}</p> : null
}

/** Plays an item, with controls drawn here because streams can start partway in. */
export function Player({
  screen,
  onLeave,
  onReplace,
}: {
  screen: { itemId: number; title: string }
  onLeave: () => void
  onReplace: (screen: Screen) => void
}) {
  const [video, setVideo] = useState<HTMLVideoElement | null>(null)
  const [token, setToken] = useState<string | null>(null)
  const [versions, setVersions] = useState<Version[] | null>(null)
  const [, setTick] = useState(0)
  const [overlay, setOverlay] = useState<Overlay>('timeline')
  const [ended, setEnded] = useState(false)
  const [dismissed, setDismissed] = useState(false)
  const [countdown, setCountdown] = useState(COUNTDOWN_SECONDS)
  const hideTimerRef = useRef<number | undefined>(undefined)
  /** Back from the panel returns focus to the button that opened it. */
  const [focusTracks, setFocusTracks] = useState(false)
  const redraw = useCallback(() => setTick((n) => n + 1), [])

  /** Fades what's showing after a while, unless paused, when it's wanted. */
  const hideLater = useCallback(() => {
    window.clearTimeout(hideTimerRef.current)
    hideTimerRef.current = window.setTimeout(() => {
      setOverlay((now) => (now === 'panel' || video?.paused ? now : 'hidden'))
    }, CONTROLS_HIDE_MS)
  }, [video])

  const show = useCallback(
    (next: Overlay) => {
      setFocusTracks(false)
      setOverlay(next)
      hideLater()
    },
    [hideLater],
  )

  useEffect(() => {
    mediaToken().then(setToken, (err: Error) => window.showCrash(err.message))
    api.versions(screen.itemId).then(setVersions, () => undefined)
  }, [screen.itemId])

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
    hideLater()
    const ticking = window.setInterval(redraw, 1000)
    return () => {
      window.clearInterval(ticking)
      window.clearTimeout(hideTimerRef.current)
    }
  }, [redraw, hideLater])

  const plan = playback?.plan
  const context = plan?.context
  const next = context?.next ?? null
  const position = playback ? playback.position() : 0
  const duration = playback ? playback.duration() : 0
  const percent = duration > 0 ? Math.min(100, (position / duration) * 100) : 0

  const nearEnd = duration > 0 && position > 0 && duration - position <= UP_NEXT_SECONDS
  const upNext = next && !dismissed && (ended || nearEnd) ? next : null
  // What keys go to: an error or the next episode comes before anything else.
  let layer: Overlay | 'error' | 'upNext' = overlay
  if (playback?.error) {
    layer = 'error'
  } else if (upNext) {
    layer = 'upNext'
  }
  const seeks = layer === 'hidden' || layer === 'timeline'
  useInitialFocus(layer)

  const playNext = useCallback(() => {
    if (next) {
      onReplace({
        name: 'player',
        itemId: next.item_id,
        title: context?.series_title ?? next.title,
      })
    }
  }, [next, context?.series_title, onReplace])

  // Once it's over: on to the next episode, or back if there's none or it's been turned down.
  useEffect(() => {
    if (!ended) {
      return
    }
    if (!upNext) {
      onLeave()
    } else if (countdown <= 0) {
      playNext()
    } else {
      const timer = window.setTimeout(() => setCountdown((n) => n - 1), 1000)
      return () => window.clearTimeout(timer)
    }
  }, [ended, upNext, countdown, onLeave, playNext])

  const onKey = useCallback(
    (keyCode: number) => {
      if (!playback) {
        return false
      }
      if (layer === 'controls') {
        hideLater()
        return false
      }
      if (!seeks) {
        return false
      }
      switch (keyCode) {
        case 37:
          playback.seekBy(-SEEK_STEP)
          show('timeline')
          return true
        case 39:
          playback.seekBy(SEEK_STEP)
          show('timeline')
          return true
        case 13:
        case 38:
        case 40:
          show('controls')
          return true
        default:
          return false
      }
    },
    [playback, layer, seeks, hideLater, show],
  )
  useKeys(onKey, seeks)

  const onBack = useCallback(() => {
    if (layer === 'upNext' && !ended) {
      setDismissed(true)
      return true
    }
    if (layer === 'panel') {
      setFocusTracks(true)
      setOverlay('controls')
      hideLater()
      return true
    }
    if (layer === 'controls' || layer === 'timeline') {
      setOverlay('hidden')
      return true
    }
    return false
  }, [layer, ended, hideLater])
  useBack(onBack)

  // A pointer can't ask for the controls with a key, so moving it brings them up.
  const onPointerMove = useCallback(() => {
    if (seeks) {
      show('controls')
    } else if (layer === 'controls') {
      hideLater()
    }
  }, [seeks, layer, show, hideLater])
  usePointerMove(onPointerMove)

  /** A click on the picture puts away whatever's over it. */
  const onVideoClick = () => {
    if (layer === 'panel' || layer === 'controls' || layer === 'timeline') {
      onBack()
    }
  }

  const seekToClick = (event: MouseEvent) => {
    const bar = (event.currentTarget as HTMLElement).getBoundingClientRect()
    if (playback && duration > 0) {
      const target = ((event.clientX - bar.left) / bar.width) * duration
      playback.seekBy(target - position)
      hideLater()
    }
  }

  const onRemote = useCallback(
    (action: RemoteAction) => {
      if (!playback || action === 'back') {
        return
      }
      if (seeks) {
        show('timeline')
      } else {
        hideLater()
      }
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
    [playback, video, seeks, show, hideLater, onLeave],
  )
  useRemote(onRemote)

  const choose = (choice: PlayChoice) => {
    playback?.choose(choice)
    setFocusTracks(true)
    setOverlay('controls')
    hideLater()
  }

  const subtitles = plan && playback ? subtitleUrl(plan, playback.choice) : null
  const isEpisode = context?.kind === 'episode'

  return (
    <main className="player">
      <video
        ref={setVideo}
        onClick={onVideoClick}
        onError={() => playback?.videoFailed()}
        onEnded={() => {
          playback?.report()
          setEnded(true)
        }}
      />
      {video && subtitles && (
        <Subtitles video={video} url={subtitles} raised={overlay !== 'hidden'} />
      )}
      {playback?.loading && !playback.error && <p className="player-status">Loading…</p>}
      {playback?.error && (
        <div className="center player-error">
          <p className="problem" role="alert">
            {playback.error}
          </p>
          <div className="actions">
            <button onClick={() => playback.retry(platform.caps())}>Try again</button>
            <button onClick={onLeave}>Back</button>
          </div>
        </div>
      )}
      <div className={overlay === 'hidden' ? 'controls hidden' : 'controls'}>
        <h1>{context?.series_title ?? context?.title ?? screen.title}</h1>
        {isEpisode && context && <p className="episode">{episodeLine(context)}</p>}
        <div className="timeline-target" onClick={seekToClick}>
          <div className="timeline">
            <div className="timeline-played" style={{ width: `${percent}%` }} />
          </div>
        </div>
        <div className="controls-row">
          <span>
            {playback?.paused() ? 'Paused · ' : ''}
            {clock(position)} / {clock(duration)}
          </span>
          {plan && <span className="muted">{MODE_LABEL[plan.mode]}</span>}
        </div>
        {layer === 'controls' && playback && (
          <div className="player-buttons">
            <button
              data-first={focusTracks ? undefined : ''}
              onClick={() => playback.togglePause()}
            >
              {playback.paused() ? 'Play' : 'Pause'}
            </button>
            <button onClick={() => playback.seekBy(-SEEK_STEP)}>Back 10</button>
            <button onClick={() => playback.seekBy(SEEK_STEP)}>Forward 10</button>
            <span className="spacer" />
            {plan && (
              <button data-first={focusTracks ? '' : undefined} onClick={() => setOverlay('panel')}>
                Audio and subtitles
              </button>
            )}
          </div>
        )}
      </div>
      {upNext && !playback?.error && (
        <section className="up-next">
          <p className="up-next-label">{ended ? `Up next in ${countdown}` : 'Up next'}</p>
          <p className="up-next-title">{episodeLine(upNext)}</p>
          <div className="actions">
            <button data-first="" onClick={playNext}>
              Play now
            </button>
            <button onClick={() => setDismissed(true)}>{ended ? 'Back' : 'Keep watching'}</button>
          </div>
        </section>
      )}
      {layer === 'panel' && plan && playback && (
        <TracksPanel
          plan={plan}
          versions={versions ?? [plan.version]}
          choice={playback.choice}
          onChoose={choose}
        />
      )}
    </main>
  )
}
