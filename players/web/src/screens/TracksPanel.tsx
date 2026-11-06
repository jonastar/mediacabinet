import { audioLabel, QUALITIES, resolution, subtitleLabel, versionLabel } from '@core/labels'
import type { PlayChoice } from '@core/tracks'
import type { PlayPlan, Version } from '../api'

const MODES: Record<PlayPlan['mode'], string> = {
  direct: 'Playing the file directly',
  remux: 'Repackaging for this TV',
  transcode: 'Converting for this TV',
}

/** The first option that's on gets focus when the panel opens. */
function Option({
  selected,
  first,
  label,
  onSelect,
}: {
  selected: boolean
  first?: boolean
  label: string
  onSelect: () => void
}) {
  return (
    <button
      className={selected ? 'option selected' : 'option'}
      data-first={first ? '' : undefined}
      onClick={onSelect}
    >
      {label}
    </button>
  )
}

/**
 * Picking a version, quality, audio or subtitles. The playback session decides
 * whether a change needs a new stream.
 */
export function TracksPanel({
  plan,
  versions,
  choice,
  onChoose,
}: {
  plan: PlayPlan
  versions: Version[]
  choice: PlayChoice
  onChoose: (next: PlayChoice) => void
}) {
  const version = plan.version
  const qualities = QUALITIES.filter(
    ([height]) => height === null || (version.height ?? 0) > height,
  )
  // Subtitles and audio are what people come here for, so focus starts there.
  const hasAudioChoice = version.audio.length > 1
  return (
    <aside className="tracks">
      {/* First, because the panel only scrolls as far as what can take focus. */}
      <h2>How it's playing</h2>
      <p className="how-playing">{MODES[plan.mode]}.</p>
      {plan.reasons.map((reason) => (
        <p key={reason} className="how-playing">
          {reason}
        </p>
      ))}
      {hasAudioChoice && (
        <>
          <h2>Audio</h2>
          {version.audio.map((track, i) => (
            <Option
              key={track.index}
              selected={track.index === plan.audio_stream}
              first={track.index === plan.audio_stream}
              label={audioLabel(track, i)}
              onSelect={() => onChoose({ ...choice, audio_stream: track.index })}
            />
          ))}
        </>
      )}
      <h2>Subtitles</h2>
      <Option
        selected={!choice.subtitle}
        first={!hasAudioChoice && !choice.subtitle}
        label="Off"
        onSelect={() => onChoose({ ...choice, subtitle: null })}
      />
      {version.subtitles.map((track, i) => (
        <Option
          key={track.id}
          selected={choice.subtitle === track.id}
          first={!hasAudioChoice && choice.subtitle === track.id}
          label={subtitleLabel(track, i)}
          onSelect={() => onChoose({ ...choice, subtitle: track.id })}
        />
      ))}
      <h2>Quality</h2>
      {qualities.map(([height, label]) => (
        <Option
          key={label}
          selected={(choice.max_height ?? null) === height}
          label={
            height === null && version.height ? `${label} (${resolution(version.height)})` : label
          }
          onSelect={() => onChoose({ ...choice, max_height: height })}
        />
      ))}
      {versions.length > 1 && (
        <>
          <h2>Version</h2>
          {versions.map((option) => (
            <Option
              key={option.file_id}
              selected={option.file_id === version.file_id}
              label={versionLabel(option)}
              // Tracks differ between files; start from the new file's defaults.
              onSelect={() => onChoose({ file_id: option.file_id, max_height: choice.max_height })}
            />
          ))}
        </>
      )}
    </aside>
  )
}
