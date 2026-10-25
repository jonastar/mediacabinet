import { useCallback, useEffect, useRef, useState } from 'react'

export interface ApiState<T> {
  data: T | null
  error: Error | null
  loading: boolean
  /** Fetches again, keeping the current data on screen until the new data arrives. */
  reload: () => Promise<void>
}

interface Result<T> {
  key: string | null
  data: T | null
  error: Error | null
}

/**
 * Fetches with `load` whenever `key` changes; `key` names what `load` fetches.
 * Pass a null key to skip fetching. Data from the previous key stays until the
 * new one arrives, so switching views doesn't flash.
 */
export function useApi<T>(key: string | null, load: () => Promise<T>): ApiState<T> {
  const [result, setResult] = useState<Result<T>>({ key: null, data: null, error: null })
  const loadRef = useRef(load)
  useEffect(() => {
    loadRef.current = load
  })

  useEffect(() => {
    if (key === null) {
      return
    }
    let current = true
    loadRef.current().then(
      (data) => current && setResult({ key, data, error: null }),
      (error: Error) => current && setResult((r) => ({ ...r, key, error })),
    )
    return () => {
      current = false
    }
  }, [key])

  const reload = useCallback(async () => {
    try {
      const data = await loadRef.current()
      setResult({ key, data, error: null })
    } catch (error) {
      setResult((r) => ({ ...r, key, error: error as Error }))
    }
  }, [key])

  return {
    data: result.data,
    error: result.key === key ? result.error : null,
    loading: key !== null && result.key !== key,
    reload,
  }
}
