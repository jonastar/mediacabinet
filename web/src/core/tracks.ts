import type { components } from '../api/schema.d.ts'

type PlayPlan = components['schemas']['PlayPlan']

export type PlayChoice = components['schemas']['Choice']

const same = <T>(a: T | null | undefined, b: T | null | undefined) => (a ?? null) === (b ?? null)

/**
 * Whether a change needs a new stream. Text subtitles are drawn over the video,
 * so only they can change without one; picture subtitles are burned in, both
 * when turning them on and when turning them off.
 */
export function needsNewStream(plan: PlayPlan, from: PlayChoice, to: PlayChoice): boolean {
  const sameStream =
    same(to.file_id, from.file_id) &&
    same(to.audio_stream, from.audio_stream) &&
    same(to.max_height, from.max_height)
  const track = plan.version.subtitles.filter((t) => t.id === to.subtitle)[0]
  const burning = plan.subtitles.some((s) => !s.url)
  return !sameStream || !(track?.text ?? true) || burning
}

/** Where the chosen subtitles are, when they're drawn over the video. */
export function subtitleUrl(plan: PlayPlan, choice: PlayChoice): string | null {
  return plan.subtitles.filter((s) => s.id === choice.subtitle)[0]?.url ?? null
}
