/** Month grids and the local day things air on, for the calendar. */

const DAY = 86400

/** A month; `month` counts from 0 like `Date`. */
export interface Month {
  year: number
  month: number
}

/** Local midnight of the day `d` falls on. */
export function startOfDay(d: Date): Date {
  return new Date(d.getFullYear(), d.getMonth(), d.getDate())
}

/** Reads `2026-11`; anything else is the month `today` is in. */
export function parseMonth(value: string | null, today: Date): Month {
  const m = value?.match(/^(\d{4})-(\d{2})$/)
  if (m && Number(m[2]) >= 1 && Number(m[2]) <= 12) {
    return { year: Number(m[1]), month: Number(m[2]) - 1 }
  }
  return { year: today.getFullYear(), month: today.getMonth() }
}

/** `2026-11`, as `parseMonth` reads it. */
export function monthParam({ year, month }: Month): string {
  return `${year}-${String(month + 1).padStart(2, '0')}`
}

export function addMonths({ year, month }: Month, n: number): Month {
  const d = new Date(year, month + n, 1)
  return { year: d.getFullYear(), month: d.getMonth() }
}

export function sameMonth(d: Date, { year, month }: Month): boolean {
  return d.getFullYear() === year && d.getMonth() === month
}

/**
 * The weeks a month's grid shows, each seven local midnights, starting on
 * `firstDay` (0 is Sunday, like `Date.getDay`). Days of the months either side
 * fill the first and last weeks.
 */
export function monthGrid({ year, month }: Month, firstDay: number): Date[][] {
  const lead = (new Date(year, month, 1).getDay() - firstDay + 7) % 7
  const length = new Date(year, month + 1, 0).getDate()
  const weeks: Date[][] = []
  let day = 1 - lead
  while (day <= length) {
    const week: Date[] = []
    for (let i = 0; i < 7; i++) {
      week.push(new Date(year, month, day))
      day++
    }
    weeks.push(week)
  }
  return weeks
}

type WeekInfo = { firstDay: number }
type LocaleWithWeek = Intl.Locale & { getWeekInfo?: () => WeekInfo; weekInfo?: WeekInfo }

/** The day a locale's weeks start on, as `Date.getDay` counts; Monday if unknown. */
export function firstWeekday(locale: string): number {
  try {
    const l = new Intl.Locale(locale) as LocaleWithWeek
    const info = l.getWeekInfo?.() ?? l.weekInfo
    if (info) {
      return info.firstDay % 7
    }
  } catch {
    // An unknown locale falls back below.
  }
  return 1
}

/**
 * The local day something airs on. A date with no known time is stored as the
 * end of that day in UTC, so it keeps that date wherever the viewer is.
 */
export function airDay(airsAt: number, timeKnown: boolean): Date {
  if (timeKnown) {
    return startOfDay(new Date(airsAt * 1000))
  }
  const d = new Date((airsAt - DAY / 2) * 1000)
  return new Date(d.getUTCFullYear(), d.getUTCMonth(), d.getUTCDate())
}
