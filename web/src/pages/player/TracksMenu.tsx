import type { PlayChoice, PlayPlan, Version } from '../../api/client'
import { audioLabel, subtitleLabel, versionLabel } from './labels'
import styles from './Player.module.css'

const QUALITIES: [number | null, string][] = [
  [null, 'Original'],
  [1080, '1080p'],
  [720, '720p'],
  [480, '480p'],
]

const MODES: Record<PlayPlan['mode'], string> = {
  direct: 'Playing the file directly',
  remux: 'Repackaging for this browser',
  transcode: 'Converting for this browser',
}

function Option({
  selected,
  label,
  onSelect,
}: {
  selected: boolean
  label: string
  onSelect: () => void
}) {
  return (
    <button
      type="button"
      role="menuitemradio"
      aria-checked={selected}
      className={styles.option}
      onClick={onSelect}
    >
      {label}
    </button>
  )
}

/**
 * Picking a version, quality, audio or subtitles. Changes that need a new
 * stream go through `onChange`; the player decides whether to restart.
 */
export function TracksMenu({
  plan,
  versions,
  choice,
  onChange,
}: {
  plan: PlayPlan
  versions: Version[]
  choice: PlayChoice
  onChange: (next: PlayChoice) => void
}) {
  const v = plan.version
  const encoder = plan.encoder === 'software' ? 'in software' : `on the GPU (${plan.encoder})`
  return (
    <div className={styles.menu} role="menu" aria-label="Playback settings">
      {versions.length > 1 && (
        <div role="group" aria-label="Version" className={styles.group}>
          <span className={styles.groupLabel}>Version</span>
          {versions.map((option) => (
            <Option
              key={option.file_id}
              selected={option.file_id === v.file_id}
              label={versionLabel(option)}
              // Tracks differ between files; start from the new file's defaults.
              onSelect={() => onChange({ file_id: option.file_id, max_height: choice.max_height })}
            />
          ))}
        </div>
      )}
      <div role="group" aria-label="Quality" className={styles.group}>
        <span className={styles.groupLabel}>Quality</span>
        {QUALITIES.filter(([h]) => h === null || (v.height ?? 0) > h).map(([height, label]) => (
          <Option
            key={label}
            selected={(choice.max_height ?? null) === height}
            label={label}
            onSelect={() => onChange({ ...choice, max_height: height })}
          />
        ))}
      </div>
      {v.audio.length > 1 && (
        <div role="group" aria-label="Audio" className={styles.group}>
          <span className={styles.groupLabel}>Audio</span>
          {v.audio.map((track, i) => (
            <Option
              key={track.index}
              selected={track.index === plan.audio_stream}
              label={audioLabel(track, i)}
              onSelect={() => onChange({ ...choice, audio_stream: track.index })}
            />
          ))}
        </div>
      )}
      <div role="group" aria-label="Subtitles" className={styles.group}>
        <span className={styles.groupLabel}>Subtitles</span>
        <Option
          selected={!choice.subtitle}
          label="Off"
          onSelect={() => onChange({ ...choice, subtitle: null })}
        />
        {v.subtitles.map((track, i) => (
          <Option
            key={track.id}
            selected={choice.subtitle === track.id}
            label={subtitleLabel(track, i)}
            onSelect={() => onChange({ ...choice, subtitle: track.id })}
          />
        ))}
      </div>
      <div className={styles.group}>
        <span className={styles.groupLabel}>How it's playing</span>
        <p className={styles.info}>
          {MODES[plan.mode]}
          {plan.mode === 'transcode' && ` ${encoder}`}.
        </p>
        {plan.reasons.map((reason) => (
          <p key={reason} className={styles.reason}>
            {reason}
          </p>
        ))}
      </div>
    </div>
  )
}
