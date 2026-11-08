import { useEffect, useState } from 'react'
import { countdown } from '../core/countdown'

const REFRESH_MS = 30_000

/** Live text for the time until an episode airs. */
export function Countdown({
  airsAt,
  timeKnown,
  short = false,
}: {
  airsAt: number
  timeKnown: boolean
  short?: boolean
}) {
  const [now, setNow] = useState(() => Date.now() / 1000)
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now() / 1000), REFRESH_MS)
    return () => clearInterval(timer)
  }, [])
  const left = countdown(airsAt, timeKnown, now)
  return <>{short ? left.short : left.long}</>
}
