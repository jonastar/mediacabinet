import { useEffect, useState } from 'preact/hooks'
import { rememberedFailures } from '@core/caps'
import { describeFailure } from '@core/fallback'
import { hasMediaSource } from '@core/stream'
import { session } from '../api'
import { loadHls } from '../hls'
import { platform } from '../platform'

function chromium() {
  const found = /Chrome\/(\d+)/.exec(navigator.userAgent)
  return found ? found[1] : 'unknown'
}

/** What this TV can do, for finding how far back the app runs. */
export function Diagnostics() {
  const [hls, setHls] = useState('Checking…')
  useEffect(() => {
    loadHls().then(
      (Hls) => setHls(Hls.isSupported() ? 'Works' : 'Not supported'),
      (err: Error) => setHls(`Can't load: ${err.message}`),
    )
  }, [])
  const caps = platform.caps()
  const rows: [string, string][] = [
    ['Device', platform.deviceName],
    ['Engine', `Chromium ${chromium()}`],
    ['Server', session.server() ?? ''],
    [
      'Screen',
      `${window.screen.width}×${window.screen.height}, ${window.innerWidth}×${window.innerHeight} inside`,
    ],
    ['Media Source', hasMediaSource() ? 'Yes' : 'No'],
    ['hls.js', hls],
    ['Video', (caps.video_codecs ?? []).join(', ') || 'None'],
    ['Audio', (caps.audio_codecs ?? []).join(', ') || 'None'],
    ['Containers', (caps.containers ?? []).join(', ') || 'None'],
    ['HDR', caps.hdr ? 'Yes' : 'No'],
    ['Found not to play', rememberedFailures().map(describeFailure).join(', ') || 'Nothing'],
    ['User agent', navigator.userAgent],
  ]
  return (
    <main className="browse">
      <h1>About this TV</h1>
      <dl className="facts">
        {rows.map(([label, value]) => (
          <div key={label}>
            <dt>{label}</dt>
            <dd>{value}</dd>
          </div>
        ))}
      </dl>
      <p className="muted">Press Back to return.</p>
    </main>
  )
}
