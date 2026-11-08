/**
 * The locale for dates, numbers and language names. The app's wording is
 * English, so these are English too: an English locale the browser asks for
 * keeps its region's conventions (`en-US` writes `Nov 8`), and anything else
 * gets `en-GB` (`8 Nov`, 24-hour clock), close to most of the world's.
 */
export function englishLocale(languages: readonly string[]): string {
  return languages.find((l) => /^en(-|$)/i.test(l)) ?? 'en-GB'
}

export const LOCALE = englishLocale(
  typeof navigator === 'undefined' ? [] : (navigator.languages ?? [navigator.language]),
)
