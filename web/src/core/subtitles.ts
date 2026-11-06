/** A subtitle: its time on screen in seconds, and its text without markup. */
export interface Cue {
  start: number
  end: number
  text: string
}

const TIMING = /(\d+:)?(\d+):(\d+)[.,](\d+)\s*-->\s*(\d+:)?(\d+):(\d+)[.,](\d+)/

function seconds(hours: string | undefined, minutes: string, whole: string, fraction: string) {
  const hour = hours ? parseInt(hours, 10) : 0
  return hour * 3600 + parseInt(minutes, 10) * 60 + parseInt(whole, 10) + Number(`0.${fraction}`)
}

function plainText(lines: string[]) {
  return lines
    .join('\n')
    .replace(/<[^>]*>/g, '')
    .replace(/&nbsp;/g, ' ')
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&amp;/g, '&')
    .trim()
}

/** Reads WebVTT. Styling and positions are left out: cues are shown as plain text. */
export function parseVtt(vtt: string): Cue[] {
  const cues: Cue[] = []
  for (const block of vtt.replace(/\r\n?/g, '\n').split(/\n{2,}/)) {
    const lines = block.split('\n')
    for (let i = 0; i < lines.length; i++) {
      const timing = TIMING.exec(lines[i])
      if (timing) {
        const text = plainText(lines.slice(i + 1))
        if (text) {
          cues.push({
            start: seconds(timing[1], timing[2], timing[3], timing[4]),
            end: seconds(timing[5], timing[6], timing[7], timing[8]),
            text,
          })
        }
        break
      }
    }
  }
  return cues
}

/** What's on screen at this time. */
export function textAt(cues: Cue[], time: number): string {
  return cues
    .filter((cue) => cue.start <= time && time < cue.end)
    .map((cue) => cue.text)
    .join('\n')
}
