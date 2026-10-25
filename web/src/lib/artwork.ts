/**
 * Background for a hero: the backdrop under a dark scrim so text keeps 4.5:1
 * contrast, over the placeholder colour (seen while loading, or without art).
 */
export function heroBackground(backdrop: string | null, tone: string): string {
  if (!backdrop) {
    return tone
  }
  const scrim =
    'linear-gradient(to top, rgba(15,17,21,0.96) 0%, rgba(15,17,21,0.7) 45%, rgba(15,17,21,0.35) 100%)'
  const side = 'linear-gradient(to right, rgba(15,17,21,0.85) 0%, rgba(15,17,21,0) 65%)'
  return `${scrim}, ${side}, url("${backdrop}") center / cover no-repeat, ${tone}`
}
