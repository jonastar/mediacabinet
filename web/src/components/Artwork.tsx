import { useState } from 'react'

/** A cover image over a placeholder colour; the colour stays if the image fails. */
export function CoverImage({ src, alt = '' }: { src: string | null; alt?: string }) {
  const [failed, setFailed] = useState(false)
  if (!src || failed) {
    return null
  }
  return (
    <img
      src={src}
      alt={alt}
      loading="lazy"
      onError={() => setFailed(true)}
      style={{ position: 'absolute', inset: 0, width: '100%', height: '100%', objectFit: 'cover' }}
    />
  )
}
