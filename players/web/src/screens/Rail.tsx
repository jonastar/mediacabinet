import { useEffect, useRef, useState } from 'preact/hooks'

/**
 * A row that scrolls sideways. Focus scrolls it with a remote; a pointer has
 * arrows at the ends instead, which aren't buttons so focus never lands on them.
 */
export function Rail({
  name,
  title,
  className,
  children,
}: {
  name: string
  title?: string
  className?: string
  children: preact.ComponentChildren
}) {
  const itemsRef = useRef<HTMLDivElement>(null)
  const [before, setBefore] = useState(false)
  const [after, setAfter] = useState(false)

  const measure = () => {
    const items = itemsRef.current
    if (items) {
      setBefore(items.scrollLeft > 0)
      setAfter(items.scrollLeft + items.clientWidth < items.scrollWidth - 1)
    }
  }

  useEffect(measure)

  const page = (way: number) => {
    const items = itemsRef.current
    if (items) {
      items.scrollLeft += way * Math.round(items.clientWidth * 0.75)
    }
  }

  return (
    <section className={className ?? 'rail'} data-rail={name}>
      {title && <h2>{title}</h2>}
      <div className="rail-body">
        <div className="rail-items" ref={itemsRef} onScroll={measure}>
          {children}
        </div>
        {before && (
          <div className="rail-arrow rail-before" onClick={() => page(-1)}>
            ‹
          </div>
        )}
        {after && (
          <div className="rail-arrow rail-after" onClick={() => page(1)}>
            ›
          </div>
        )}
      </div>
    </section>
  )
}
